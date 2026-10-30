use crate::model::{Book, Column, Scan, SeriesValue, Settings};
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::{collections::BTreeMap, path::Path};
pub fn discover(library: &str) -> Result<Vec<Column>> {
    let connection = Connection::open_with_flags(
        Path::new(library).join("metadata.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut statement = connection
        .prepare("SELECT id,label,name FROM custom_columns WHERE datatype='series' ORDER BY id")?;
    let columns = statement
        .query_map([], |r| {
            Ok(Column {
                id: r.get(0)?,
                label: r.get(1)?,
                name: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

pub fn scan(settings: &Settings) -> Result<Scan> {
    let path = Path::new(&settings.library)
        .canonicalize()
        .context("Library directory not found")?;
    let connection = Connection::open_with_flags(
        path.join("metadata.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Cannot read Calibre metadata.db")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch("BEGIN DEFERRED;")?;
    let columns: Vec<Column> = connection
        .prepare("SELECT id,label,name FROM custom_columns WHERE datatype='series'")?
        .query_map([], |row| {
            Ok(Column {
                id: row.get(0)?,
                label: row.get(1)?,
                name: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut selected = Vec::new();
    for label in &settings.columns {
        let column = columns
            .iter()
            .find(|column| &column.label == label)
            .with_context(|| format!("Select an existing series-type custom column for {label}"))?;
        if column.id <= 0 || selected.contains(&column.id) {
            bail!("Custom series columns must be distinct and valid");
        }
        selected.push(column.id);
    }
    let mut books = BTreeMap::new();
    let mut statement = connection.prepare("SELECT id,uuid,title,series_index FROM books")?;
    for row in statement.query_map([], |row| {
        let mut hierarchy = Default::default();
        let value = SeriesValue {
            name: None,
            index: Some(row.get::<_, f64>(3)?),
        };
        let hierarchy: &mut [SeriesValue; 4] = &mut hierarchy;
        hierarchy[0] = value;
        Ok(Book {
            id: row.get(0)?,
            uuid: row.get(1)?,
            title: row.get(2)?,
            hierarchy: hierarchy.clone(),
            authors: Vec::new(),
            identifiers: BTreeMap::new(),
            publisher: None,
            has_comments: false,
            formats: Vec::new(),
        })
    })? {
        let book = row?;
        books.insert(book.id, book);
    }
    let mut statement = connection.prepare(
        "SELECT l.book,a.name FROM books_authors_link l JOIN authors a ON a.id=l.author ORDER BY l.id"
    )?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, author) = row?;
        if let Some(book) = books.get_mut(&id) {
            book.authors.push(author);
        }
    }
    let mut statement = connection.prepare("SELECT book,type,val FROM identifiers")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (id, kind, value) = row?;
        if let Some(book) = books.get_mut(&id) {
            book.identifiers.insert(kind, value);
        }
    }
    for (sql, target) in [
        ("SELECT l.book,s.name FROM books_series_link l JOIN series s ON s.id=l.series", 0),
        ("SELECT l.book,p.name FROM books_publishers_link l JOIN publishers p ON p.id=l.publisher", 1),
        ("SELECT book,format FROM data", 2),
    ] {
        for row in connection.prepare(sql)?.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })? {
            let (id, value) = row?;
            if let Some(book) = books.get_mut(&id) {
                match target {
                    0 => book.hierarchy[0].name = Some(value),
                    1 => book.publisher = Some(value),
                    _ => book.formats.push(value),
                }
            }
        }
    }
    for row in connection
        .prepare("SELECT book FROM comments WHERE text IS NOT NULL AND length(trim(text))>0")?
        .query_map([], |row| row.get::<_, i64>(0))?
    {
        if let Some(book) = books.get_mut(&row?) {
            book.has_comments = true;
        }
    }
    for (slot, column) in selected.iter().enumerate() {
        let sql = format!(
            "SELECT l.book,v.value,l.extra FROM books_custom_column_{column}_link l
             JOIN custom_column_{column} v ON v.id=l.value"
        );
        for row in connection.prepare(&sql)?.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
            ))
        })? {
            let (id, name, index) = row?;
            if let Some(book) = books.get_mut(&id) {
                book.hierarchy[slot + 1] = SeriesValue {
                    name: Some(name),
                    index,
                };
            }
        }
    }
    Ok(Scan {
        library: path.to_string_lossy().into(),
        books: books.into_values().collect(),
        columns,
    })
}
