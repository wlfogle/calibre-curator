use curator_core::{matching::*, model::*, sources, store::Store};
use std::{collections::BTreeMap, fs};
fn book() -> Book {
    Book {
        id: 1,
        uuid: "uuid-1".into(),
        title: "A Work".into(),
        authors: vec!["Author".into()],
        identifiers: BTreeMap::new(),
        hierarchy: [
            SeriesValue {
                name: Some("Old".into()),
                index: Some(4.0),
            },
            SeriesValue::default(),
            SeriesValue::default(),
            SeriesValue::default(),
        ],
        publisher: None,
        has_comments: false,
        formats: vec!["EPUB".into()],
    }
}
#[test]
fn partial_rules_preserve_primary_and_compare_effective_targets() {
    let b = book();
    let mut partial = reference();
    partial.field_mask = Some([false, true, false, false]);
    let mut data = AppData {
        references: vec![partial.clone()],
        ..Default::default()
    };
    let result = propose(&b, &data, "library", &issues(&data.references));
    assert_eq!(result.after.unwrap()[0], b.hierarchy[0]);
    let mut full = partial;
    full.id = new_id();
    full.field_mask = None;
    data.references.push(full);
    assert!(propose(&b, &data, "library", &issues(&data.references))
        .after
        .is_none());
}
fn reference() -> Reference {
    Reference {
        title: "A Work".into(),
        authors: vec!["Author".into()],
        book_uuids: vec!["uuid-1".into()],
        approved: true,
        evidence: "Reviewed fixture".into(),
        hierarchy: [
            SeriesValue {
                name: Some("World".into()),
                index: Some(1.0),
            },
            SeriesValue {
                name: Some("Branch".into()),
                index: Some(2.0),
            },
            SeriesValue::default(),
            SeriesValue::default(),
        ],
        ..Default::default()
    }
}
#[test]
fn never_certifies_populated_metadata_without_a_rule() {
    let p = propose(&book(), &AppData::default(), "library", &BTreeMap::new());
    assert_eq!(p.status, "unresolved");
    assert!(p.blocked);
    assert!(p.after.is_none());
}
#[test]
fn conflict_drafts_stale_identity_and_per_field_pins() {
    let mut data = AppData {
        references: vec![reference()],
        ..Default::default()
    };
    data.references[0].approved = false;
    assert!(propose(&book(), &data, "library", &BTreeMap::new())
        .after
        .is_none());
    data.references[0].approved = true;
    data.pins
        .insert(pin_key("library", "uuid-1"), [true, false, false, false]);
    let p = propose(&book(), &data, "library", &BTreeMap::new());
    assert_eq!(p.status, "protected");
    assert!(!p.blocked);
    let mut other = reference();
    other.hierarchy[0].index = Some(9.0);
    data.references.push(other);
    assert!(propose(&book(), &data, "library", &BTreeMap::new())
        .after
        .is_none());
    data.references.pop();
    let mut changed = book();
    changed.authors = vec!["Different author".into()];
    assert!(propose(&changed, &data, "library", &BTreeMap::new())
        .after
        .is_none());
}
#[test]
fn collection_scope_is_not_silently_treated_as_a_novel() {
    let mut b = book();
    b.title = "A Work Omnibus".into();
    let mut r = reference();
    r.title = b.title.clone();
    r.edition = "novel".into();
    let data = AppData {
        references: vec![r],
        ..Default::default()
    };
    assert!(propose(&b, &data, "library", &BTreeMap::new())
        .after
        .is_none());
}
#[test]
fn imports_cannot_smuggle_approval_and_odt_preserves_rows() {
    let temp = tempfile::tempdir().unwrap();
    let json = temp.path().join("source.json");
    fs::write(&json, serde_json::to_vec(&vec![reference()]).unwrap()).unwrap();
    let (_, refs) = sources::import(&json, "", None).unwrap();
    assert!(!refs[0].approved);
    let path = temp.path().join("document.odt");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("content.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"<office xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><t:h>Cycle</t:h><table:table-row><table:table-cell><t:p>1</t:p></table:table-cell><table:table-cell><t:p>Book</t:p></table:table-cell></table:table-row></office>"#).unwrap();
        zip.finish().unwrap();
    }
    let (source, _) = sources::import(&path, "", None).unwrap();
    assert_eq!(source.segments.len(), 2);
    assert_eq!(source.segments[0].kind, "heading");
    assert_eq!(source.segments[1].text, "1 | Book");
}
#[test]
fn workspace_cannot_be_a_library_or_symlink_into_it() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    fs::write(library.join("metadata.db"), b"test sentinel").unwrap();
    assert!(Store::open(&library.join("workspace")).is_err());
    #[cfg(unix)]
    {
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&library, &alias).unwrap();
        assert!(Store::open(&alias.join("workspace")).is_err());
    }
    assert_eq!(
        fs::read(library.join("metadata.db")).unwrap(),
        b"test sentinel"
    );
}
