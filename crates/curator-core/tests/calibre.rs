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
