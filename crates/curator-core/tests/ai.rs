use curator_core::{ai::*, app::App, matching, model::*, sources};
use serde_json::{json, Value};
#[test]
#[ignore = "Explicit local-model diagnostic; fictional source only"]
fn real_native_fixture_citations() {
    let settings = AiSettings {
        enabled: true,
        ..Default::default()
    };
    let client = Ollama::new(&settings).unwrap();
    let model = client
        .models()
        .unwrap()
        .into_iter()
        .find(|m| m.name == settings.model)
        .unwrap();
    let segments=vec![
        Segment{ordinal:1,kind:"p".into(),text:"Universe: Test Universe. The following entries give whole-book chronological positions.".into()},
        Segment{ordinal:2,kind:"p".into(),text:"Chronological position: 1. Title: First Light. Author: Test Author. Subseries: Dawn Cycle. Subseries number: 2. Edition: novel.".into()}
    ];
    let response = client.generate(&model, &segments, "Test Universe").unwrap();
    let mut result: Extraction =
        serde_json::from_str(response["message"]["content"].as_str().unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    let count = complete_identity_context(&mut result, &segments).unwrap();
    println!("Backend-added identity context expansions: {count}");
    validate(&result, &segments, "Test Universe").unwrap();
}
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

fn source() -> Source {
    let text = "1. First Light by Test Author, in Dawn Cycle book 2.";
    Source {
        id: "source".into(),
        name: "Fictional timeline".into(),
        location: "fictional fixture".into(),
        sha256: sources::hash(text.as_bytes()),
        kind: "html".into(),
        segments: vec![Segment {
            ordinal: 1,
            kind: "paragraph".into(),
            text: text.into(),
        }],
    }
}

#[test]
fn explicit_binding_preserves_metadata_and_stale_evidence_is_refused() {
    let mock = Mock::new();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app(temp.path(), &mock);
    let book = Book {
        id: 7,
        uuid: "explicit-fixture-uuid".into(),
        title: "First Light".into(),
        authors: vec!["Test Author".into()],
        identifiers: BTreeMap::new(),
        hierarchy: [
            SeriesValue {
                name: Some("Old World".into()),
                index: Some(6.0),
            },
            SeriesValue {
                name: Some("Retain Branch".into()),
                index: Some(8.0),
            },
            SeriesValue::default(),
            SeriesValue::default(),
        ],
        publisher: None,
        has_comments: true,
        formats: vec!["EPUB".into()],
    };
    app.scan = Some(Scan {
        library: "fixture".into(),
        books: vec![book.clone()],
        columns: vec![],
    });
    app.data
        .pins
        .insert(matching::pin_key("fixture", &book.uuid), [true; 4]);
    app.extract_ai("source", 1, 1, "Test Universe", false)
        .unwrap();
    let id = app.data.ai_runs[0].id.clone();
    app.materialize_ai(&id, 0, Some(7)).unwrap();
    assert_eq!(app.data.references[0].book_uuids, vec![book.uuid.clone()]);
    assert_eq!(app.data.references[0].hierarchy[1], book.hierarchy[1]);
    assert!(!app.data.references[0].approved);
    assert_eq!(
        app.data.pins[&matching::pin_key("fixture", &book.uuid)],
        [true; 4]
    );
    app.data.sources[0].segments[0].text = "Source was edited".into();
    let mut edited = app.data.references[0].clone();
    edited.approved = true;
    assert!(app.save_reference(edited).is_err());
}

#[test]
fn hostile_response_creates_no_inbox_or_rule() {
    let mock = Mock::new();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app(temp.path(), &mock);
    let mut hostile = serde_json::to_value(extraction()).unwrap();
    hostile["candidates"][0]["book_uuids"] = json!(["do-not-bind"]);
    mock.reply.lock().unwrap()["message"]["content"] = json!(hostile.to_string());
    assert!(app
        .extract_ai("source", 1, 1, "Test Universe", false)
        .is_err());
    assert!(app.data.ai_runs.is_empty());
    assert!(app.data.references.is_empty());
}
#[test]
fn completes_missing_title_only_from_an_already_cited_segment() {
    let segments = source().segments;
    let mut result = extraction();
    result.candidates[0].citations[0].quote = "Test Author".into();
    assert!(validate(&result, &segments, "Test Universe").is_err());
    assert_eq!(
        complete_identity_context(&mut result, &segments).unwrap(),
        1
    );
    assert_eq!(result.candidates[0].citations[0].quote, segments[0].text);
    validate(&result, &segments, "Test Universe").unwrap();
    let mut fabricated = extraction();
    fabricated.candidates[0].citations[0].quote = "Invented Test Author quote".into();
    assert!(complete_identity_context(&mut fabricated, &segments).is_err());
    let mut absent = extraction();
    absent.candidates[0].title = "Absent Novel".into();
    complete_identity_context(&mut absent, &segments).unwrap();
    assert!(validate(&absent, &segments, "Test Universe").is_err());
}
#[test]
fn identity_completion_cannot_search_uncited_segments() {
    let segments = vec![
        Segment {
            ordinal: 1,
            kind: "p".into(),
            text: "1. Author: Test Author.".into(),
        },
        Segment {
            ordinal: 2,
            kind: "p".into(),
            text: "Title: First Light.".into(),
        },
    ];
    let mut result = extraction();
    result.candidates[0].citations = vec![Citation {
        segment: 1,
        quote: segments[0].text.clone(),
    }];
    result.candidates[0].fields[0].citations = result.candidates[0].citations.clone();
    assert_eq!(
        complete_identity_context(&mut result, &segments).unwrap(),
        0
    );
    assert!(validate(&result, &segments, "Test Universe").is_err());
}
fn extraction() -> Extraction {
    let citation = Citation {
        segment: 1,
        quote: source().segments[0].text.clone(),
    };
    Extraction {
        status: "candidates".into(),
        explanation: "Explicit numbered fixture.".into(),
        warnings: vec![],
        candidates: vec![Candidate {
            title: "First Light".into(),
            authors: vec!["Test Author".into()],
            edition: "novel".into(),
            notes: "Unverified fixture extraction.".into(),
            citations: vec![citation.clone()],
            fields: vec![ExtractedField {
                slot: 0,
                name: "Test Universe".into(),
                index: Some(1.0),
                citations: vec![citation],
            }],
        }],
    }
}
struct Mock {
    endpoint: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    reply: Arc<Mutex<Value>>,
    digest: Arc<Mutex<String>>,
    remote: Arc<AtomicBool>,
    status: Arc<AtomicU16>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Mock {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let calls = requests.clone();
        let reply = Arc::new(Mutex::new(
            json!({"done":true,"done_reason":"stop","message":{"content":serde_json::to_string(&extraction()).unwrap()},
            "prompt_eval_count":25,"eval_count":50,"total_duration":1_000_000}),
        ));
        let response = reply.clone();
        let digest = Arc::new(Mutex::new("a".repeat(64)));
        let digest_value = digest.clone();
        let remote = Arc::new(AtomicBool::new(false));
        let remote_value = remote.clone();
        let status = Arc::new(AtomicU16::new(200));
        let code = status.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(_) => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut length = 0;
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                if reader.read_exact(&mut bytes).is_err() {
                    continue;
                }
                let body = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
                calls.lock().unwrap().push((path.clone(), body));
                let value = match path.as_str() {
                    "/api/tags" => {
                        json!({"models":[{"name":"fixture:latest","digest":*digest_value.lock().unwrap(),"size":2_000_000,
                        "details":{"format":"gguf"},"capabilities":["completion"]}]})
                    }
                    "/api/show" => {
                        if remote_value.load(Ordering::SeqCst) {
                            json!({"remote_host":"https://ollama.com","remote_model":"cloud-model"})
                        } else {
                            json!({"model_info":{"general.parameter_count":123},"capabilities":["completion"]})
                        }
                    }
                    _ => response.lock().unwrap().clone(),
                };
                let text = value.to_string();
                let status = code.load(Ordering::SeqCst);
                let header=format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nLocation: /redirected\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",text.len());
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(text.as_bytes());
            }
        });
        Self {
            endpoint,
            requests,
            reply,
            digest,
            remote,
            status,
            stop,
            thread: Some(worker),
        }
    }
    fn settings(&self) -> AiSettings {
        AiSettings {
            enabled: true,
            endpoint: self.endpoint.clone(),
            model: "fixture:latest".into(),
            ..Default::default()
        }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.thread.take().unwrap().join();
    }
}
fn app(root: &std::path::Path, mock: &Mock) -> App {
    let mut app = App::open(root, PathBuf::new()).unwrap();
    app.data.sources.push(source());
    app.configure_ai(mock.settings()).unwrap();
    app
}
#[test]
fn endpoint_privacy_and_opt_in() {
    for endpoint in [
        "https://ollama.com",
        "http://169.254.169.254",
        "http://0.0.0.0",
        "file:///tmp/test",
        "http://user:password@127.0.0.1:11434",
        "http://127.0.0.1:11434/path",
        "http://127.0.0.1:11434/?key=x",
    ] {
        assert!(
            Ollama::new(&AiSettings {
                endpoint: endpoint.into(),
                allow_lan: true,
                ..Default::default()
            })
            .is_err(),
            "{endpoint}"
        );
    }
    assert!(Ollama::new(&AiSettings {
        endpoint: "http://192.168.1.2:11434".into(),
        ..Default::default()
    })
    .is_err());
    assert!(Ollama::new(&AiSettings {
        endpoint: "http://192.168.1.2:11434".into(),
        allow_lan: true,
        ..Default::default()
    })
    .is_ok());
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::open(temp.path(), PathBuf::new()).unwrap();
    app.data.sources.push(source());
    assert!(app
        .extract_ai("source", 1, 1, "Test Universe", false)
        .is_err());
}
#[test]
fn rejects_hallucinations_injection_and_membership_overflow() {
    let segments = source().segments;
    let valid = extraction();
    assert!(validate(&valid, &segments, "Test Universe").is_ok());
    let mut wrong = valid.clone();
    wrong.candidates[0].citations[0].quote = "Fabricated First Light by Test Author".into();
    assert!(validate(&wrong, &segments, "Test Universe").is_err());
    wrong = valid.clone();
    wrong.candidates[0].fields[0].index = Some(99.0);
    assert!(validate(&wrong, &segments, "Test Universe").is_err());
    wrong = valid.clone();
    wrong.candidates[0].fields[0].slot = 4;
    assert!(validate(&wrong, &segments, "Test Universe").is_err());
    let mut injection = serde_json::to_value(valid).unwrap();
    injection["candidates"][0]["approved"] = json!(true);
    assert!(serde_json::from_value::<Extraction>(injection).is_err());
    let insufficient = Extraction {
        status: "insufficient_evidence".into(),
        explanation: "No numbers given.".into(),
        warnings: vec![],
        candidates: vec![],
    };
    assert!(validate(&insufficient, &segments, "").is_ok());
    let mut giant = source();
    giant.segments[0].text = "x".repeat(INPUT_LIMIT + 1);
    assert!(selected(&giant, 1, 1).is_err());
    assert!(selected(&source(), 0, 1).is_err());
}
#[test]
fn protocol_cache_digest_invalidation_and_remembered_decisions() {
    let mock = Mock::new();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app(temp.path(), &mock);
    app.extract_ai("source", 1, 1, "Test Universe", false)
        .unwrap();
    assert!(app.data.references.is_empty());
    let run = app.data.ai_runs[0].clone();
    app.decide_ai(&run.id, 0, "dismissed").unwrap();
    app.extract_ai("source", 1, 1, "Test Universe", false)
        .unwrap();
    assert!(app.data.ai_runs[0].cached);
    assert_eq!(
        mock.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "/api/chat")
            .count(),
        1
    );
    assert_eq!(
        app.data.ai_decisions[&app.data.ai_runs[0].fingerprints[0]].status,
        "dismissed"
    );
    *mock.digest.lock().unwrap() = "b".repeat(64);
    app.extract_ai("source", 1, 1, "Test Universe", false)
        .unwrap();
    assert!(!app.data.ai_runs[0].cached);
    assert_eq!(
        mock.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "/api/chat")
            .count(),
        2
    );
    let calls = mock.requests.lock().unwrap();
    let request = &calls.iter().find(|r| r.0 == "/api/chat").unwrap().1;
    assert_eq!(request["stream"], false);
    assert_eq!(request["format"]["additionalProperties"], false);
    assert!(request.get("tools").is_none());
    assert!(!request["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("book_uuids"));
}
#[test]
fn blocks_cloud_models_redirects_and_oversized_responses() {
    let mock = Mock::new();
    let client = Ollama::new(&mock.settings()).unwrap();
    let model = client.models().unwrap().remove(0);
    mock.remote.store(true, Ordering::SeqCst);
    assert!(client
        .generate(&model, &source().segments, "Test Universe")
        .is_err());
    assert!(!mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.0 == "/api/chat"));
    mock.status.store(307, Ordering::SeqCst);
    assert!(client.models().is_err());
    assert!(!mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.0 == "/redirected"));
    mock.status.store(200, Ordering::SeqCst);
    mock.remote.store(false, Ordering::SeqCst);
    *mock.reply.lock().unwrap() = json!({"huge":"x".repeat(300_000)});
    assert!(client
        .generate(&model, &source().segments, "Test Universe")
        .is_err());
}
#[test]
fn drafts_never_approve_or_clear_unspecified_fields_even_without_binding() {
    let mock = Mock::new();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app(temp.path(), &mock);
    app.extract_ai("source", 1, 1, "Test Universe", false)
        .unwrap();
    let run = app.data.ai_runs[0].id.clone();
    app.materialize_ai(&run, 0, None).unwrap();
    assert!(!app.data.references[0].approved);
    assert!(app.data.references[0].book_uuids.is_empty());
    assert_eq!(
        app.data.references[0].field_mask,
        Some([true, false, false, false])
    );
    assert!(app.materialize_ai(&run, 0, None).is_err());
    let book = Book {
        id: 1,
        uuid: "private-library-uuid".into(),
        title: "First Light".into(),
        authors: vec!["Test Author".into()],
        identifiers: BTreeMap::new(),
        hierarchy: [
            SeriesValue {
                name: Some("Original World".into()),
                index: Some(4.0),
            },
            SeriesValue {
                name: Some("Keep Branch".into()),
                index: Some(7.0),
            },
            SeriesValue::default(),
            SeriesValue::default(),
        ],
        publisher: None,
        has_comments: true,
        formats: vec!["EPUB".into()],
    };
    let mut reference = app.data.references[0].clone();
    reference.approved = true;
    app.save_reference(reference).unwrap();
    let proposal = matching::propose(
        &book,
        &app.data,
        "fixture",
        &matching::issues(&app.data.references),
    );
    assert_eq!(proposal.after.unwrap()[1], book.hierarchy[1]);
    app.clear_ai_cache().unwrap();
    assert_eq!(app.data.references.len(), 1);
    assert!(app.data.ai_runs.is_empty());
    let reopened = App::open(temp.path(), PathBuf::new()).unwrap();
    assert_eq!(reopened.data.ai_decisions.len(), 1);
}
#[test]
fn legacy_state_loads_with_ai_disabled() {
    let data: AppData = serde_json::from_value(
        json!({"references":[{"title":"Legacy"}],"pins":{"library::id":[true,true,true,true]}}),
    )
    .unwrap();
    assert!(!data.ai_settings.enabled);
    assert!(data.references[0].ai_provenance.is_none());
    assert!(data.references[0].field_mask.is_none());
    assert!(data.pins["library::id"][0]);
}
#[test]
#[ignore = "Explicit opt-in: makes a small real local Ollama inference, using fictional text only"]
fn real_local_ollama_extraction() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::open(temp.path(), PathBuf::new()).unwrap();
    app.data.sources.push(source());
    app.configure_ai(AiSettings {
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    app.extract_ai("source", 1, 1, "Test Universe", true)
        .unwrap();
    let run = &app.data.ai_runs[0];
    assert!(
        run.extraction.status == "candidates" || run.extraction.status == "insufficient_evidence"
    );
    assert!(app.data.references.is_empty());
    println!(
        "Local model {}: {}, {} candidate(s), {} output tokens, {} ms",
        run.model,
        run.extraction.status,
        run.extraction.candidates.len(),
        run.output_tokens,
        run.duration_ms
    );
}
