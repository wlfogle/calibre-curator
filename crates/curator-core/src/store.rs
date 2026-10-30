use crate::model::AppData;
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Store {
    connection: Connection,
    pub root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        if let Some(existing) = root.ancestors().find(|p| p.exists()) {
            if existing
                .canonicalize()?
                .ancestors()
                .any(|p| p.join("metadata.db").is_file())
            {
                bail!("The workspace must not resolve into a Calibre library");
            }
        }
        if root
            .ancestors()
            .any(|parent| parent.join("metadata.db").is_file())
        {
            bail!("The application workspace must be outside a Calibre library or backup");
        }
        fs::create_dir_all(root)?;
        if root.join("state.sqlite3").is_symlink() || root.join("operations").is_symlink() {
            bail!("Workspace database and operation directory must not be symbolic links");
        }
        let connection = Connection::open(root.join("state.sqlite3"))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS state(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS ai_cache(key TEXT PRIMARY KEY,value TEXT NOT NULL);",
        )?;
        fs::create_dir_all(root.join("operations"))?;
        Ok(Self {
            connection,
            root: root.canonicalize()?,
        })
    }
    pub fn load(&self) -> Result<AppData> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM state WHERE key='app'")?;
        let mut rows = statement.query([])?;
        if let Some(row) = rows.next()? {
            let raw: String = row.get(0)?;
            Ok(serde_json::from_str(&raw).context("Stored application state is invalid")?)
        } else {
            Ok(AppData::default())
        }
    }
    pub fn save(&self, data: &AppData) -> Result<()> {
        self.connection.execute(
            "INSERT INTO state(key,value) VALUES('app',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [serde_json::to_string(data)?],
        )?;
        Ok(())
    }
    pub fn cached_value<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .connection
            .query_row("SELECT value FROM ai_cache WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn cache_value(&self, key: &str, value: &impl Serialize) -> Result<()> {
        self.connection.execute("INSERT INTO ai_cache(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key,&serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn lookup_cached(&self, key: &str) -> Result<Option<crate::metadata::CachedMetadata>> {
        self.cached_value(&format!("metadata:{key}"))
    }
    pub fn lookup_cache(&self, key: &str, value: &crate::metadata::CachedMetadata) -> Result<()> {
        self.cache_value(&format!("metadata:{key}"), value)
    }
    pub fn cached(&self, key: &str) -> Result<Option<crate::ai::AiRun>> {
        let raw: Option<String> = self
            .connection
            .query_row("SELECT value FROM ai_cache WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn cache(&self, run: &crate::ai::AiRun) -> Result<()> {
        self.connection.execute("INSERT INTO ai_cache(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [&run.cache_key,&serde_json::to_string(run)?])?;
        Ok(())
    }
    pub fn clear_ai_cache(&self) -> Result<()> {
        self.connection
            .execute("DELETE FROM ai_cache WHERE key NOT LIKE 'metadata:%'", [])?;
        Ok(())
    }
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("File needs a parent directory")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
    }
    fs::rename(temporary, path)?;
    if let Ok(directory) = fs::File::open(parent) {
        directory.sync_all()?;
    }
    Ok(())
}
