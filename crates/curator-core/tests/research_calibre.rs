use curator_core::{
    app::App,
    chronology, library, metadata,
    model::*,
    repair::RepairRequest,
    research::{Lead, ResearchRun},
    sources,
};
use std::{fs, path::Path, process::Command};

#[test]
fn automated_research_full_backup_apply_and_revert_without_reference_documents() {
    if !Path::new("/usr/bin/calibre-debug").is_file() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let output = Command::new("/usr/bin/calibre-debug")
        .args(["-c", include_str!("../../../tests/fixture.py")])
        .env("CURATOR_TEST_LIBRARY", &library)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path: String = rusqlite::Connection::open(library.join("metadata.db"))
        .unwrap()
        .query_row("SELECT path FROM books WHERE id=4", [], |r| r.get(0))
        .unwrap();
    let ebook = Path::new(&path).join("fictional.epub");
    fs::write(
        library.join(&ebook),
        b"Independent fictional ebook snapshot",
    )
    .unwrap();
    fs::write(library.join("auxiliary.txt"), b"Keep this too").unwrap();
    let production =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/bridge.py");
    let script=format!("import sys,json,pathlib\nif pathlib.Path(json.loads(pathlib.Path(sys.argv[1]).read_text())['library']).resolve()!=pathlib.Path({:?}).resolve():\n raise RuntimeError('Not disposable')\nimport calibre.utils.lock as test_lock\ntest_lock.__appname__={:?}\n{}",
        library.to_string_lossy(),format!("research-test-{}",new_id()),fs::read_to_string(production).unwrap());
    let isolated = temp.path().join("bridge.py");
    fs::write(&isolated, &script).unwrap();
    let mut app = App::open(&temp.path().join("workspace"), isolated.clone()).unwrap();
    app.settings(Settings {
        library: library.to_string_lossy().into(),
        ..Default::default()
    })
    .unwrap();
    let original = library::scan(&app.data.settings).unwrap();
    let page=chronology::parse_page(b"<p>Test Author</p><h1>Complete chronological order of Fixture Universe</h1><ol><li>Earlier Unowned Work</li><li>Ungrouped</li></ol>","https://author.example/chronology").unwrap();
    for query in [
        "\"Ungrouped\" Test Author chronological order subseries",
        "Test Author  universe complete chronology bibliography",
    ] {
        let run = ResearchRun {
            query: query.into(),
            fetched_at: metadata::now(),
            cached: false,
            status: "complete".into(),
            notes: vec![],
            leads: vec![Lead {
                url: page.source.location.clone(),
                title: "Fixture chronology".into(),
                engine: "DuckDuckGo fixture".into(),
            }],
        };
        app.store
            .cache_value(
                &format!("research:{}", sources::hash(query.as_bytes())),
                &run,
            )
            .unwrap();
    }
    app.store
        .cache_value(
            &format!(
                "research:page:{}",
                sources::hash(page.source.location.as_bytes())
            ),
            &page,
        )
        .unwrap();
    app.research_book(4, false).unwrap();
    assert!(app.data.references.is_empty());
    let run = app.data.chronology_runs[0].clone();
    assert_eq!(run.status, "ready");
    assert_eq!(run.hierarchy[0].index, Some(2.0));
    let failing = temp.path().join("no-space.py");
    let needle = "if shutil.disk_usage(backup.parent).free < required:";
    assert!(script.contains(needle));
    fs::write(&failing, script.replace(needle, "if True:")).unwrap();
    app.bridge = failing;
    assert!(app
        .apply_repairs(RepairRequest {
            series: vec![],
            fills: vec![],
            research: vec![run.id.clone()]
        })
        .unwrap_err()
        .to_string()
        .contains("Insufficient backup space"));
    assert_eq!(
        serde_json::to_value(library::scan(&app.data.settings).unwrap().books).unwrap(),
        serde_json::to_value(&original.books).unwrap()
    );
    assert!(app
        .history()
        .unwrap()
        .iter()
        .all(|o| o.phase == "rejected" && o.backup.is_none()));
    app.bridge = isolated;
    app.apply_repairs(RepairRequest {
        series: vec![],
        fills: vec![],
        research: vec![run.id],
    })
    .unwrap();
    let operation = app
        .history()
        .unwrap()
        .into_iter()
        .find(|o| o.phase == "verified")
        .unwrap();
    assert_eq!(operation.backup_coverage, "full-library");
    assert!(operation.backup_verified);
    assert_eq!(operation.research_evidence.len(), 1);
    assert_eq!(
        operation.research_evidence[0].evidence[0].url,
        "https://author.example/chronology"
    );
    let backup = Path::new(operation.backup.as_ref().unwrap());
    let snapshot = backup.join("library");
    assert_eq!(
        fs::read(snapshot.join(&ebook)).unwrap(),
        b"Independent fictional ebook snapshot"
    );
    assert_eq!(
        fs::read(snapshot.join("auxiliary.txt")).unwrap(),
        b"Keep this too"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["files"][ebook.to_str().unwrap()]["sha256"],
        sources::hash(b"Independent fictional ebook snapshot")
    );
    assert_eq!(
        app.scan
            .as_ref()
            .unwrap()
            .books
            .iter()
            .find(|b| b.id == 4)
            .unwrap()
            .hierarchy[0]
            .index,
        Some(2.0)
    );
    let reverted = app.reverse(&operation.id).unwrap();
    assert!(reverted.backup_verified);
    assert_eq!(
        serde_json::to_value(&app.scan.as_ref().unwrap().books).unwrap(),
        serde_json::to_value(&original.books).unwrap()
    );
    assert_eq!(
        fs::read(library.join(&ebook)).unwrap(),
        b"Independent fictional ebook snapshot"
    );
}
