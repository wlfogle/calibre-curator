use curator_core::{ai::*, app::App, metadata::*, model::*, synopsis::*};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
const DESCRIPTION:&str="Mira, a botanist, arrives on a remote island to investigate why its gardens are dying. She works with the local gardener to trace the damage to a blocked stream, while a storm threatens the village and forces them to cooperate.";
fn book() -> Book {
    Book {
        id: 1,
        uuid: "fixture-book".into(),
        title: "Sample Book".into(),
        authors: vec!["Test Author".into()],
        identifiers: BTreeMap::new(),
        hierarchy: [
            SeriesValue {
                name: None,
                index: Some(1.0),
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
fn identifiers_do_not_override_different_work_titles() {
    let mut query = book();
    query
        .identifiers
        .insert("isbn".into(), "9780553296129".into());
    let mut result = parse_opf(opf("Sample Book Two", "Test Author").as_bytes()).unwrap();
    result
        .identifiers
        .insert("isbn".into(), "9780553296129".into());
    assert_eq!(identity(&query, &result).0, "mismatch");
    result.title = "Sample Book: A Novel".into();
    assert_eq!(identity(&query, &result).0, "compatible");
    result.title = "Sample Book".into();
    result
        .identifiers
        .insert("isbn".into(), "9780553293357".into());
    assert_eq!(identity(&query, &result).0, "different_edition");
}
fn opf(title: &str, author: &str) -> String {
    format!("<package xmlns=\"http://www.idpf.org/2007/opf\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:opf=\"http://www.idpf.org/2007/opf\"><metadata><dc:title>{title}</dc:title><dc:creator>{author}</dc:creator><dc:description>&lt;p&gt;{DESCRIPTION}&lt;/p&gt;&lt;script&gt;DO NOT EXECUTE&lt;/script&gt;</dc:description></metadata></package>")
}
fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn fixture(root: &Path) -> App {
    let debug = root.join("calibre-debug");
    executable(&debug,"#!/usr/bin/python3\nprint('CURATOR_PLUGINS:[{\"name\":\"Fixture\",\"version\":\"1\",\"identify\":true,\"enabled\":true}]')\n");
    let fetch = root.join("fetch-ebook-metadata");
    executable(
        &fetch,
        &format!(
            "#!/usr/bin/python3\nprint({:?})\n",
            opf("Sample Book", "Test Author")
        ),
    );
    let library = root.join("library");
    fs::create_dir(&library).unwrap();
    rusqlite::Connection::open(library.join("metadata.db"))
        .unwrap()
        .execute(
            "CREATE TABLE comments(book INTEGER PRIMARY KEY,text TEXT)",
            [],
        )
        .unwrap();
    let mut app = App::open(&root.join("workspace"), root.join("unused-bridge.py")).unwrap();
    app.data.settings.library = library.to_string_lossy().into();
    app.data.settings.calibre_binary = debug.to_string_lossy().into();
    app.scan = Some(Scan {
        library: app.data.settings.library.clone(),
        books: vec![book()],
        columns: vec![],
    });
    app.configure_metadata(MetadataSettings {
        fetch_binary: fetch.to_string_lossy().into(),
        providers: vec!["Fixture".into()],
        ..Default::default()
    })
    .unwrap();
    app
}
#[test]
fn parses_public_fields_and_rejects_identity_conflicts() {
    let result =
        parse_opf(format!("startup noise\n{}\n", opf("Sample Book", "Test Author")).as_bytes())
            .unwrap();
    assert!(result.description.contains("blocked stream"));
    assert!(!result.description.contains("DO NOT EXECUTE"));
    assert_eq!(identity(&book(), &result).0, "compatible");
    let wrong = parse_opf(opf("Different Work", "Different Author").as_bytes()).unwrap();
    assert_eq!(identity(&book(), &wrong).0, "mismatch");
    assert!(parse_opf(b"<package><bad").is_err());
    let mut ids = BTreeMap::new();
    ids.insert(
        "MOBI-ASIN".into(),
        "4253f6d5-d9b2-4768-a871-90aefa452066".into(),
    );
    ids.insert("revision".into(), "1.1".into());
    ids.insert("GOODREADS".into(), "123".into());
    assert_eq!(safe_ids(&ids).len(), 1);
}
#[test]
fn subprocess_bounds_and_timeout_are_real() {
    let start = Instant::now();
    assert!(run_bounded(
        Command::new("/bin/sh").args(["-c", "sleep 10"]),
        Duration::from_millis(150)
    )
    .unwrap_err()
    .to_string()
    .contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(run_bounded(
        Command::new("/usr/bin/python3").args(["-c", "import sys;sys.stdout.write('x'*2200000)"]),
        Duration::from_secs(5)
    )
    .is_err());
}
#[test]
fn lookup_confirmation_cache_and_protected_comments() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = fixture(temp.path());
    app.lookup_metadata(1, false).unwrap();
    let first = app.data.metadata_results[0].id.clone();
    assert!(!app.data.metadata_results[0].identity_confirmed);
    assert!(app.generate_synopsis(&first, false, false).is_err());
    app.confirm_metadata(&first, true).unwrap();
    app.generate_synopsis(&first, false, false).unwrap();
    assert!(!app.data.synopsis_drafts[0].approved);
    assert_eq!(app.data.synopsis_drafts[0].text, DESCRIPTION);
    let draft = app.data.synopsis_drafts[0].clone();
    app.save_synopsis(&draft.id, &draft.text, true).unwrap();
    assert!(app.data.synopsis_drafts[0].approved);
    app.lookup_metadata(1, false).unwrap();
    assert!(app.data.metadata_results[0].cached);
    assert!(!app.data.metadata_results[0].identity_confirmed);
    assert!(app.data.metadata_results.iter().any(|r| r.id == first));
    app.confirm_metadata(&first, false).unwrap();
    assert!(!app.data.synopsis_drafts[0].approved);
    let latest = app.data.metadata_results[0].id.clone();
    app.confirm_metadata(&latest, true).unwrap();
    let db = rusqlite::Connection::open(Path::new(&app.data.settings.library).join("metadata.db"))
        .unwrap();
    db.execute(
        "INSERT INTO comments VALUES(1,'Keep existing synopsis')",
        [],
    )
    .unwrap();
    assert!(app.generate_synopsis(&latest, false, false).is_err());
}
#[test]
fn strict_synopsis_evidence_and_html_escaping() {
    let segments = vec![
        Segment {
            ordinal: 1,
            kind: "identity".into(),
            text: "Sample Book by Test Author".into(),
        },
        Segment {
            ordinal: 2,
            kind: "description".into(),
            text: DESCRIPTION.into(),
        },
    ];
    let mut output=SynopsisOutput{status:"draft".into(),synopsis:"A botanist investigates dying gardens on a remote island while a storm threatens its village.".into(),
        citations:vec![Citation{segment:2,quote:"Mira, a botanist, arrives on a remote island".into()}],notes:String::new()};
    validate_output(&output, &segments).unwrap();
    output.citations[0].quote = "A fabricated plot quotation that never appeared".into();
    assert!(validate_output(&output, &segments).is_err());
    assert!(!as_html("<script>alert(1)</script> & text").contains("<script>"));
    assert!(as_html("<script>").contains("&lt;script&gt;"));
    let hostile = serde_json::json!({"status":"draft","synopsis":DESCRIPTION,"citations":[],"notes":"","approved":true});
    assert!(serde_json::from_value::<SynopsisOutput>(hostile).is_err());
}
#[test]
#[ignore = "Explicit opt-in: real local Ollama, fictional provider description only"]
fn real_local_synopsis() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = fixture(temp.path());
    app.lookup_metadata(1, false).unwrap();
    let id = app.data.metadata_results[0].id.clone();
    app.confirm_metadata(&id, true).unwrap();
    app.configure_ai(AiSettings {
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    app.generate_synopsis(&id, true, true).unwrap();
    let draft = &app.data.synopsis_drafts[0];
    assert!(!draft.approved);
    println!(
        "Synopsis result: {}; model {}; {} output tokens; {} chars",
        draft.state,
        draft.model,
        draft.output_tokens,
        draft.text.len()
    );
}
#[test]
#[ignore = "Explicit opt-in: one public Google metadata lookup, no library access"]
fn real_metadata_provider() {
    let provider = discover("/usr/bin/calibre-debug")
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Google")
        .unwrap();
    let mut book = book();
    book.title = "Heir to the Empire".into();
    book.authors = vec!["Timothy Zahn".into()];
    book.identifiers
        .insert("isbn".into(), "9780553296129".into());
    let result = fetch(&MetadataSettings::default(), &provider, &book).unwrap();
    assert_eq!(identity(&book, &result).0, "compatible");
    println!(
        "Provider {}; returned title {}; description {} characters",
        provider.name,
        result.title,
        result.description.len()
    );
}
