use curator_core::{
    app::App, documents::*, matching, metadata::FetchedMetadata, model::*, repair, sources,
};
use std::{collections::BTreeMap, fs, io::Write};

fn book(title: &str) -> Book {
    Book {
        id: 1,
        uuid: new_id(),
        title: title.into(),
        authors: vec!["Fixture Author".into()],
        identifiers: BTreeMap::new(),
        hierarchy: [
            SeriesValue {
                name: None,
                index: Some(1.0),
            },
            SeriesValue::default(),
            SeriesValue {
                name: Some("Preserved".into()),
                index: Some(7.0),
            },
            SeriesValue::default(),
        ],
        publisher: None,
        has_comments: false,
        formats: vec!["EPUB".into()],
    }
}
#[test]
fn source_verification_does_not_remove_a_previously_approved_decision() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.odt");
    odt(&path, &["A (2010) – Branch #1"]);
    let (source, _) = sources::import(&path, "", None).unwrap();
    let mut app = App::open(&temp.path().join("workspace"), temp.path().join("unused")).unwrap();
    let b = book("A");
    app.scan = Some(Scan {
        library: "fixture".into(),
        books: vec![b.clone()],
        columns: vec![],
    });
    let mut r = Reference {
        title: b.title.clone(),
        authors: b.authors.clone(),
        source_id: source.id.clone(),
        approved: true,
        evidence: "Independent reviewed decision".into(),
        ..Default::default()
    };
    r.hierarchy[0] = SeriesValue {
        name: Some("Fixture Universe".into()),
        index: Some(9.0),
    };
    let id = r.id.clone();
    app.data.sources.push(source.clone());
    app.data.references.push(r);
    app.verify_document(&source.id, review()).unwrap();
    assert!(app.data.references.iter().any(|r| r.id == id && r.approved));
    assert!(matching::propose(
        &b,
        &app.data,
        "fixture",
        &matching::issues(&app.data.references)
    )
    .after
    .is_none());
}
fn review() -> SourceReview {
    SourceReview {
        universe: "Fixture Universe".into(),
        authors: vec!["Fixture Author".into()],
        aliases: BTreeMap::from([("Night Trilogy".into(), "Night".into())]),
        ..Default::default()
    }
}
fn odt(path: &std::path::Path, paragraphs: &[&str]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("content.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    let xml = format!("<office xmlns:t=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><t:h>Phase 1</t:h>{}</office>",
        paragraphs.iter().map(|p|format!("<t:p>{p}</t:p>")).collect::<String>());
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap();
}
#[test]
fn verified_order_is_global_and_preserves_unsupported_fields_pins_and_missing_books() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.odt");
    odt(
        &path,
        &[
            "First (2010) – Island #1",
            "Not Owned (2010) – Island #2",
            "Interlude (2011) – Novella",
            "Nightfall (2012) – Night Trilogy #1",
        ],
    );
    let (source, _) = sources::import(&path, "", None).unwrap();
    let mut app = App::open(&temp.path().join("workspace"), temp.path().join("unused")).unwrap();
    app.data.sources.push(source.clone());
    let b = book("Nightfall");
    app.scan = Some(Scan {
        library: "fixture".into(),
        books: vec![b.clone()],
        columns: vec![],
    });
    let report = app.preview_document(&source.id, &review()).unwrap();
    assert_eq!(report.entries.len(), 4);
    assert_eq!(
        report.entries[2].reference.field_mask,
        Some([true, false, false, false])
    );
    assert_eq!(report.entries[3].reference.hierarchy[0].index, Some(4.0));
    assert_eq!(
        report.entries[3].reference.hierarchy[1].name.as_deref(),
        Some("Night")
    );
    assert_eq!(report.entries[3].reference.hierarchy[1].index, Some(1.0));
    app.verify_document(&source.id, review()).unwrap();
    let p = matching::propose(
        &b,
        &app.data,
        "fixture",
        &matching::issues(&app.data.references),
    );
    assert!(!p.blocked);
    assert_eq!(p.after.unwrap()[2], b.hierarchy[2]);
    app.data.pins.insert(
        matching::pin_key("fixture", &b.uuid),
        [true, true, true, true],
    );
    assert!(matching::propose(&b, &app.data, "fixture", &BTreeMap::new()).blocked);
    app.data.pins.clear();
    app.data.sources[0].segments[1].text.push_str(" changed");
    assert!(
        matching::propose(&b, &app.data, "fixture", &BTreeMap::new())
            .after
            .is_none()
    );
    app.data.sources[0] = source.clone();
    app.revoke_document(&source.id).unwrap();
    assert!(
        matching::propose(&b, &app.data, "fixture", &BTreeMap::new())
            .after
            .is_none()
    );
    assert!(app.store.load().unwrap().source_reviews.is_empty());
}
#[test]
fn ambiguous_duplicate_titles_and_conflicting_rules_are_held() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.odt");
    odt(
        &path,
        &[
            "First (2010) – Branch #1",
            "First (2010) – Branch #2",
            "Other (2011) – Novella",
        ],
    );
    let (source, _) = sources::import(&path, "", None).unwrap();
    let scan = Scan {
        library: "fixture".into(),
        books: vec![book("Other"), book("Other")],
        columns: vec![],
    };
    let p = preview(&source, &review(), &scan).unwrap();
    assert_eq!(p.entries[0].status, "duplicate source entry");
    assert_eq!(p.entries[1].status, "duplicate source entry");
    assert_eq!(p.entries[2].status, "ambiguous editions");
    let mut app = App::open(&temp.path().join("workspace"), temp.path().join("unused")).unwrap();
    app.scan = Some(Scan {
        library: "fixture".into(),
        books: vec![book("First")],
        columns: vec![],
    });
    app.data.sources.push(source.clone());
    app.verify_document(&source.id, review()).unwrap();
    assert!(app
        .data
        .references
        .iter()
        .all(|r| !r.approved || r.title == "Other"));
    // Separate, valid conflicting source rules cannot silently override each other.
    let b = book("A");
    let mut a = Reference {
        title: b.title.clone(),
        authors: b.authors.clone(),
        approved: true,
        evidence: "Fixture decision".into(),
        ..Default::default()
    };
    a.hierarchy[0] = SeriesValue {
        name: Some("World".into()),
        index: Some(1.0),
    };
    let mut other = a.clone();
    other.id = new_id();
    other.hierarchy[0].index = Some(2.0);
    let data = AppData {
        references: vec![a, other],
        ..Default::default()
    };
    assert!(matching::propose(&b, &data, "fixture", &BTreeMap::new())
        .after
        .is_none());
}
#[test]
fn csv_supports_all_nested_slots_without_changing_independent_numbers() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.csv");
    fs::write(&path,"title,authors,series2,index2,series3,index3,series4,index4\nA,Fixture Author,Branch,9,Arc,3,Mini,2\n").unwrap();
    let (source, _) = sources::import(&path, "", None).unwrap();
    let scan = Scan {
        library: "fixture".into(),
        books: vec![book("A")],
        columns: vec![],
    };
    let r = &preview(&source, &review(), &scan).unwrap().entries[0].reference;
    assert_eq!(
        r.hierarchy.iter().map(|v| v.index).collect::<Vec<_>>(),
        vec![Some(1.0), Some(9.0), Some(3.0), Some(2.0)]
    );
    assert_eq!(r.field_mask, Some([true; 4]));
}
#[test]
fn missing_metadata_fills_never_replace_populated_values_or_invalid_ids() {
    let mut b = book("A");
    b.publisher = Some("Keep".into());
    b.identifiers.insert("isbn".into(), "9780553296129".into());
    let metadata = FetchedMetadata {
        title: "A".into(),
        authors: b.authors.clone(),
        publisher: Some("Other".into()),
        description: String::new(),
        url: None,
        identifiers: BTreeMap::from([
            ("isbn".into(), "9780553293357".into()),
            ("goodreads".into(), "1234".into()),
            ("unsafe".into(), "garbage".into()),
        ]),
    };
    let edit = repair::fill_values(&b, &metadata, true, true);
    assert_eq!(edit.after.publisher, b.publisher);
    assert_eq!(edit.after.identifiers["isbn"], b.identifiers["isbn"]);
    assert_eq!(edit.after.identifiers["goodreads"], "1234");
    assert!(!edit.after.identifiers.contains_key("unsafe"));
}
#[test]
#[ignore = "Explicitly selected private document, read-only; set CURATOR_TEST_DOCUMENT"]
fn selected_local_document_read_only() {
    let path = std::env::var("CURATOR_TEST_DOCUMENT").expect("Select a document");
    let (source, _) = sources::import(std::path::Path::new(&path), "", None).unwrap();
    let report = preview(
        &source,
        &review(),
        &Scan {
            library: String::new(),
            books: vec![],
            columns: vec![],
        },
    )
    .unwrap();
    println!(
        "Parsed {} ordered works, {} nested assignments; {} excluded non-entry segments",
        report.entries.len(),
        report
            .entries
            .iter()
            .filter(|e| e.reference.hierarchy[1].name.is_some())
            .count(),
        report.skipped.len()
    );
    assert!(!report.entries.is_empty());
}
