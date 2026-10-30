use curator_core::{app::App, model::*};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

fn command(script: &str, library: &Path) {
    let output = Command::new("/usr/bin/calibre-debug")
        .args(["-c", script])
        .env("CURATOR_TEST_LIBRARY", library)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn synopsis_comments_roundtrip_recovery_and_existing_protection() {
    use curator_core::metadata::MetadataSettings;
    use std::os::unix::fs::PermissionsExt;
    if !Path::new("/usr/bin/calibre-debug").is_file() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    std::fs::create_dir(&library).unwrap();
    command(include_str!("../../../tests/fixture.py"), &library);
    command("import os; from calibre.db.legacy import LibraryDatabase; db=LibraryDatabase(os.environ['CURATOR_TEST_LIBRARY']); db.new_api.set_field('comments',{4:None}); db.new_api.dump_metadata(book_ids={4}); db.close()",&library);
    let bridge = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/bridge.py");
    let namespace = format!("synopsis-test-{}", new_id());
    let source=format!("import sys,json,pathlib\nif pathlib.Path(json.loads(pathlib.Path(sys.argv[1]).read_text())['library']).resolve()!=pathlib.Path({:?}).resolve():\n    raise RuntimeError('Not the disposable library')\nimport calibre.utils.lock as test_lock\ntest_lock.__appname__={:?}\n{}",
        library.to_string_lossy(),namespace,std::fs::read_to_string(&bridge).unwrap());
    let isolated = temp.path().join("isolated-bridge.py");
    std::fs::write(&isolated, &source).unwrap();
    let debug = temp.path().join("calibre-debug");
    std::fs::write(&debug,"#!/usr/bin/python3\nimport sys,os\nif '-c' in sys.argv:\n print('CURATOR_PLUGINS:[{\"name\":\"Fixture\",\"version\":\"1\",\"identify\":true,\"enabled\":true}]')\nelse:\n os.execv('/usr/bin/calibre-debug',['/usr/bin/calibre-debug']+sys.argv[1:])\n").unwrap();
    std::fs::set_permissions(&debug, std::fs::Permissions::from_mode(0o755)).unwrap();
    let fetch = temp.path().join("fetch-ebook-metadata");
    let xml="<package xmlns=\"http://www.idpf.org/2007/opf\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><metadata><dc:title>Ungrouped</dc:title><dc:creator>Test Author</dc:creator><dc:description>A botanist investigates dying gardens on an isolated island. Working with a local gardener, she traces the damage to a blocked stream while a storm threatens the village.</dc:description></metadata></package>";
    std::fs::write(&fetch, format!("#!/usr/bin/python3\nprint({xml:?})\n")).unwrap();
    std::fs::set_permissions(&fetch, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = App::open(&temp.path().join("workspace"), isolated.clone()).unwrap();
    app.settings(Settings {
        library: library.to_string_lossy().into(),
        calibre_binary: debug.to_string_lossy().into(),
        ..Default::default()
    })
    .unwrap();
    let hierarchy: Vec<_> = app
        .scan
        .as_ref()
        .unwrap()
        .books
        .iter()
        .map(|b| (b.id, b.hierarchy.clone()))
        .collect();
    app.configure_metadata(MetadataSettings {
        fetch_binary: fetch.to_string_lossy().into(),
        providers: vec!["Fixture".into()],
        ..Default::default()
    })
    .unwrap();
    app.lookup_metadata(4, false).unwrap();
    let lookup = app.data.metadata_results[0].id.clone();
    app.confirm_metadata(&lookup, true).unwrap();
    app.generate_synopsis(&lookup, false, false).unwrap();
    let draft = app.data.synopsis_drafts[0].clone();
    assert!(app.apply_synopses(std::slice::from_ref(&draft.id)).is_err());
    app.save_synopsis(&draft.id, &draft.text, true).unwrap();
    let operation = app.apply_synopses(std::slice::from_ref(&draft.id)).unwrap();
    assert_eq!(operation.metadata_field, "comments");
    assert_eq!(operation.phase, "verified");
    assert_eq!(
        operation.verification.as_ref().unwrap()["all_book_comments_checked"],
        4
    );
    assert_eq!(
        hierarchy,
        app.scan
            .as_ref()
            .unwrap()
            .books
            .iter()
            .map(|b| (b.id, b.hierarchy.clone()))
            .collect::<Vec<_>>()
    );
    assert!(curator_core::library::comments(&app.data.settings, 4)
        .unwrap()
        .unwrap()
        .contains("botanist"));
    assert_eq!(
        curator_core::library::comments(&app.data.settings, 1)
            .unwrap()
            .as_deref(),
        Some("<p>Keep synopsis.</p>")
    );
    assert!(app.generate_synopsis(&lookup, false, false).is_err());
    app.reverse(&operation.id).unwrap();
    assert_eq!(
        curator_core::library::comments(&app.data.settings, 4).unwrap(),
        None
    );
    assert_eq!(app.data.synopsis_drafts[0].state, "undone");
    app.generate_synopsis(&lookup, false, false).unwrap();
    let draft = app.data.synopsis_drafts[0].clone();
    app.save_synopsis(&draft.id, &draft.text, true).unwrap();
    let broken = temp.path().join("interrupted-synopsis.py");
    let needle = "            checkpoint()\n        else:\n            write_fields";
    assert!(source.contains(needle));
    std::fs::write(&broken,source.replace(needle,"            checkpoint()\n            import os\n            os._exit(77)\n        else:\n            write_fields")).unwrap();
    app.bridge = broken;
    assert!(app.apply_synopses(std::slice::from_ref(&draft.id)).is_err());
    let interrupted = app
        .history()
        .unwrap()
        .into_iter()
        .find(|o| o.phase == "writing")
        .unwrap();
    app.bridge = isolated;
    app.reverse(&interrupted.id).unwrap();
    assert_eq!(
        curator_core::library::comments(&app.data.settings, 4).unwrap(),
        None
    );
    app.generate_synopsis(&lookup, false, false).unwrap();
    let draft = app.data.synopsis_drafts[0].clone();
    app.save_synopsis(&draft.id, &draft.text, true).unwrap();
    let last = app.apply_synopses(std::slice::from_ref(&draft.id)).unwrap();
    command("import os; from calibre.db.legacy import LibraryDatabase; db=LibraryDatabase(os.environ['CURATOR_TEST_LIBRARY']); db.new_api.set_field('comments',{4:'Intervening synopsis edit'}); db.close()",&library);
    assert!(app
        .reverse(&last.id)
        .unwrap_err()
        .to_string()
        .contains("Intervening"));
    assert_eq!(
        curator_core::library::comments(&app.data.settings, 4)
            .unwrap()
            .as_deref(),
        Some("Intervening synopsis edit")
    );
}
fn request(app: &App, id: i64) -> Change {
    let p = app
        .snapshot()
        .unwrap()
        .proposals
        .into_iter()
        .find(|p| p.book.id == id)
        .unwrap();
    Change {
        id,
        uuid: p.book.uuid,
        title: p.book.title,
        before: p.book.hierarchy,
        after: p.after.unwrap(),
        authors: p.book.authors,
        identifiers: p.book.identifiers,
        evidence: String::new(),
        sources: Vec::new(),
        comments: None,
    }
}

#[test]
fn calibre_api_roundtrip_recovery_lock_and_guards() {
    if !Path::new("/usr/bin/calibre-debug").is_file() {
        eprintln!("Calibre integration skipped: executable unavailable");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    std::fs::create_dir(&library).unwrap();
    command(include_str!("../../../tests/fixture.py"), &library);
    let bridge = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/resources/bridge.py")
        .canonicalize()
        .unwrap();
    // Linux Calibre mutexes are global per user, not per library. A test-only
    // namespace lets this disposable fixture coexist with the user's open GUI.
    // Assert the target directory before changing this test process's namespace.
    let namespace = format!("curator-test-{}", new_id());
    let source=format!("import sys, json, pathlib\nif pathlib.Path(json.loads(pathlib.Path(sys.argv[1]).read_text())['library']).resolve() != pathlib.Path({:?}).resolve():\n    raise RuntimeError('Test target is not the disposable library')\nimport calibre.utils.lock as test_lock\ntest_lock.__appname__ = {:?}\n{}",
        library.to_string_lossy(),namespace,std::fs::read_to_string(&bridge).unwrap());
    let isolated = temp.path().join("isolated-bridge.py");
    std::fs::write(&isolated, &source).unwrap();
    let mut app = App::open(&temp.path().join("workspace"), isolated.clone()).unwrap();
    app.settings(Settings {
        library: library.to_string_lossy().into(),
        ..Default::default()
    })
    .unwrap();
    let book = app.scan.as_ref().unwrap().books[0].clone();
    let original = book.hierarchy.clone();
    let mut reference = Reference {
        title: book.title.clone(),
        authors: book.authors.clone(),
        book_uuids: vec![book.uuid.clone()],
        hierarchy: book.hierarchy.clone(),
        evidence: "Known disposable fixture; whole-book chronological rule.".into(),
        approved: true,
        ..Default::default()
    };
    reference.hierarchy[0] = SeriesValue {
        name: Some("Test Universe".into()),
        index: Some(1.0),
    };
    reference.hierarchy[1] = SeriesValue {
        name: Some("Original Cycle".into()),
        index: Some(2.0),
    };
    reference.hierarchy[2] = SeriesValue {
        name: Some("Original Branch".into()),
        index: Some(4.0),
    };
    reference.hierarchy[3] = SeriesValue::default();
    app.save_reference(reference).unwrap();
    let updates = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let received = updates.clone();
    app.progress = Some(Box::new(move |event| {
        received.lock().unwrap().push(event.into())
    }));
    let stale = request(&app, book.id);
    let operation = app.apply(vec![stale.clone()]).unwrap();
    assert_eq!(operation.phase, "verified");
    let events = updates.lock().unwrap().clone();
    assert!(events.iter().any(|event| event.contains("backed_up")));
    assert!(events.iter().any(|event| event.contains("verified")));
    assert!(events
        .iter()
        .all(|event| serde_json::from_str::<serde_json::Value>(event).is_ok()));
    assert_eq!(
        operation.verification.as_ref().unwrap()["all_book_series_checked"],
        4
    );
    assert!(app.apply(vec![stale]).is_err());
    let protected = app
        .snapshot()
        .unwrap()
        .proposals
        .into_iter()
        .find(|p| p.book.title == "Protected Foundation")
        .unwrap();
    assert!(protected.pinned.iter().all(|p| *p));
    app.reverse(&operation.id).unwrap();
    assert_eq!(app.scan.as_ref().unwrap().books[0].hierarchy, original);

    // A real process exit after committed, durably checkpointed writes.
    // Production bridge contains no failure-injection hooks.
    let broken = temp.path().join("interrupted-bridge.py");
    std::fs::write(&broken,source.replace("write_fields(db.new_api, targets, checkpoint)",
        "write_fields(db.new_api, targets, checkpoint)\n        import os\n        os._exit(77)")).unwrap();
    app.bridge = broken;
    let next = request(&app, book.id);
    assert!(app.apply(vec![next]).is_err());
    let interrupted = app
        .history()
        .unwrap()
        .into_iter()
        .find(|o| o.phase == "writing")
        .unwrap();
    app.bridge = isolated;
    let recovery = app.reverse(&interrupted.id).unwrap();
    assert_eq!(recovery.kind, "recover");
    assert_eq!(app.scan.as_ref().unwrap().books[0].hierarchy, original);

    // Hold exactly the singleton lock DBCtx uses, in a child this test owns.
    let lock_script=format!("import calibre.utils.lock as lock; lock.__appname__ = {:?}; import time; held=lock.singleinstance('db'); print('LOCK_READY' if held else 'LOCK_FAILED',flush=True); time.sleep(60)",namespace);
    let mut child = Command::new("/usr/bin/calibre-debug")
        .args(["-c", &lock_script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut ready = false;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        if line.unwrap() == "LOCK_READY" {
            ready = true;
            break;
        }
    }
    assert!(ready);
    let locked_request = request(&app, book.id);
    let locked = app.apply(vec![locked_request]);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(locked.is_err(), "Calibre lock must block application");
    assert_eq!(app.scan.as_ref().unwrap().books[0].hierarchy, original);

    let next = request(&app, book.id);
    let applied = app.apply(vec![next]).unwrap();
    command("import os; from calibre.db.legacy import LibraryDatabase; db=LibraryDatabase(os.environ['CURATOR_TEST_LIBRARY']); db.new_api.set_field('comments',{1:'Intervening user edit'}); db.close()",&library);
    assert!(app
        .reverse(&applied.id)
        .unwrap_err()
        .to_string()
        .contains("Intervening"));
    let connection = rusqlite::Connection::open(library.join("metadata.db")).unwrap();
    let comment: String = connection
        .query_row("SELECT text FROM comments WHERE book=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(comment, "Intervening user edit");
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
