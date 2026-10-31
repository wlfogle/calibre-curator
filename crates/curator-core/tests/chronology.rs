use anyhow::{bail, Result};
use curator_core::{
    ai::{Candidate, Citation, ExtractedField},
    app::App,
    chronology::*,
    metadata,
    model::*,
    research::{Lead, ResearchRun},
    sources,
};
use std::collections::BTreeMap;

fn book() -> Book {
    Book {
        id: 1,
        uuid: "fictional-book".into(),
        title: "First Published".into(),
        authors: vec!["Fixture Author".into()],
        hierarchy: [
            SeriesValue {
                name: None,
                index: Some(1.0),
            },
            SeriesValue::default(),
            SeriesValue::default(),
            SeriesValue::default(),
        ],
        identifiers: BTreeMap::new(),
        publisher: None,
        has_comments: false,
        formats: vec!["EPUB".into()],
    }
}
#[test]
#[ignore = "Explicit installed local Ollama inference with fictional cached web evidence only"]
fn real_local_research_extraction() {
    let temporary = tempfile::tempdir().unwrap();
    let mut app = App::open(temporary.path(), temporary.path().join("unused")).unwrap();
    app.data.settings.library = "fictional-model-test".into();
    app.scan = Some(Scan {
        library: app.data.settings.library.clone(),
        books: vec![book()],
        columns: vec![],
    });
    app.data.ai_settings.enabled = true;
    let page=page("<p>First Published is a novel by Fixture Author in the Fixture Universe.</p><p>Chronological order of the Fixture Universe: First Published has chronological position 3.</p><p>First Published belongs to the Night Cycle subseries. First Published is Night Cycle book 2.</p>","fictional");
    for query in [
        "\"First Published\" Fixture Author chronological order subseries",
        "Fixture Author  universe complete chronology bibliography",
    ] {
        let run = ResearchRun {
            query: query.into(),
            fetched_at: metadata::now(),
            cached: false,
            status: "complete".into(),
            notes: vec![],
            leads: vec![Lead {
                url: page.source.location.clone(),
                title: "Fictional evidence".into(),
                engine: "cached fixture".into(),
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
    app.research_book(1, false).unwrap();
    let run = &app.data.chronology_runs[0];
    println!(
        "Local model result: status={}, fields={:?}, notes={:?}",
        run.status, run.field_mask, run.notes
    );
    assert_eq!(run.status, "ready");
    assert!(run.field_mask[0]);
    assert_eq!(run.hierarchy[0].index, Some(3.0));
    assert!(run.field_mask[1]);
    assert_eq!(run.hierarchy[1].index, Some(2.0));
    assert!(app.data.references.is_empty());
}
#[test]
#[ignore = "Explicit public DuckDuckGo and bounded source acquisition; no library access"]
fn public_automated_book_research() {
    let temporary = tempfile::tempdir().unwrap();
    let mut app = App::open(temporary.path(), temporary.path().join("unused")).unwrap();
    app.data.settings.library = "public-research-fixture".into();
    let mut b = book();
    b.title = "Ringworld".into();
    b.authors = vec!["Larry Niven".into()];
    app.scan = Some(Scan {
        library: app.data.settings.library.clone(),
        books: vec![b],
        columns: vec![],
    });
    app.research_book(1, true).unwrap();
    let run = &app.data.chronology_runs[0];
    assert_eq!(run.queries.len(), 2);
    assert!(app.data.sources.len() <= 6);
    assert!(app.data.references.is_empty());
    println!(
        "Automated public research: status={}, pages={}, validated findings={}, notes={:?}",
        run.status,
        app.data.sources.len(),
        run.evidence.len(),
        run.notes
    );
}
fn page(body: &str, name: &str) -> Page {
    parse_page(
        body.as_bytes(),
        &format!("https://{name}.example/chronology"),
    )
    .unwrap()
}
fn timeline(heading: &str, items: &str, name: &str) -> Page {
    page(
        &format!("<p>Fixture Author</p><h1>{heading}</h1><ol>{items}</ol>"),
        name,
    )
}
fn complete(name: &str) -> Page {
    timeline(
        "Complete chronological order of Fixture Universe",
        "<li>Earlier Prequel</li><li>Later Prequel</li><li>First Published</li>",
        name,
    )
}
fn quote(source: &Source, index: usize) -> Citation {
    Citation {
        segment: source.segments[index].ordinal,
        quote: source.segments[index].text.clone(),
    }
}
fn prose() -> (Page, Finding) {
    let p=page("<p>Chronological order in Fixture Universe.</p><p>First Published by Fixture Author is in Night Cycle book 2.</p>","prose");
    let f = Finding {
        candidate: Candidate {
            title: book().title,
            authors: book().authors,
            edition: "novel".into(),
            fields: vec![ExtractedField {
                slot: 1,
                name: "Night Cycle".into(),
                index: Some(2.0),
                citations: vec![quote(&p.source, 1)],
            }],
            citations: vec![quote(&p.source, 1)],
            notes: "Explicit independent subseries number".into(),
        },
        order_kind: "chronological".into(),
        scope: "Fixture Universe".into(),
        primary_basis: "unknown".into(),
        chronology_citations: vec![quote(&p.source, 0)],
        before_titles: vec![],
        after_titles: vec![],
    };
    (p, f)
}
struct MockWeb {
    pages: Vec<Page>,
    findings: BTreeMap<String, Vec<Finding>>,
    queries: usize,
    fetches: usize,
    blocked: bool,
}
impl MockWeb {
    fn new(pages: Vec<Page>) -> Self {
        Self {
            pages,
            findings: BTreeMap::new(),
            queries: 0,
            fetches: 0,
            blocked: false,
        }
    }
}
impl ResearchWeb for MockWeb {
    fn search(&mut self, q: &str) -> Result<ResearchRun> {
        self.queries += 1;
        if self.blocked {
            bail!("DuckDuckGo challenge");
        }
        Ok(ResearchRun {
            query: q.into(),
            fetched_at: metadata::now(),
            cached: false,
            status: "complete".into(),
            notes: vec![],
            leads: self
                .pages
                .iter()
                .map(|p| Lead {
                    url: p.source.location.clone(),
                    title: "fixture".into(),
                    engine: "DuckDuckGo fixture".into(),
                })
                .collect(),
        })
    }
    fn page(&mut self, url: &str) -> Result<Page> {
        self.fetches += 1;
        self.pages
            .iter()
            .find(|p| p.source.location == url)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown fixture URL"))
    }
    fn extract(&mut self, _book: &Book, source: &Source) -> Result<Vec<Finding>> {
        Ok(self.findings.get(&source.id).cloned().unwrap_or_default())
    }
}
#[test]
fn automated_complete_chronology_places_prequels_before_first_published_book() {
    let mut web = MockWeb::new(vec![complete("author")]);
    let data = AppData::default();
    let (run, fetched) = collect(&book(), &data, &mut web).unwrap();
    assert_eq!(run.status, "ready");
    assert_eq!(run.hierarchy[0].index, Some(3.0));
    assert_eq!(web.queries, 2);
    assert_eq!(web.fetches, 1);
    let mut data = data;
    data.sources = fetched;
    validate_run(&run, &book(), &data).unwrap();
    assert!(data.references.is_empty()); // Research never auto-approves or writes.
    data.sources[0].segments[0].text.push_str(" changed");
    assert!(validate_run(&run, &book(), &data).is_err());
}
#[test]
fn publication_recommended_partial_lists_and_years_do_not_become_chronology() {
    for heading in [
        "Publication order of Fixture Universe",
        "Recommended reading order of Fixture Universe",
        "Chronological order of Fixture Universe",
    ] {
        assert!(deterministic(
            &book(),
            &timeline(heading, "<li>First Published</li>", "fixture").source
        )
        .unwrap()
        .is_empty());
    }
    let p=page("<p>Fixture Author</p><h1>Complete chronological order of Fixture Universe</h1><ol start='1975'><li>First Published</li></ol>","years");
    assert!(deterministic(&book(), &p.source).unwrap().is_empty());
    let (p, mut finding) = prose();
    finding.candidate.fields[0].index = Some(2001.0);
    assert!(validate_finding(&book(), &p.source, &finding).is_err());
    let p=page("<p>Fixture Author</p><h1>Complete chronological order of Fixture Universe</h1><ol><li>Unowned</li></ol><ol><li>First Published</li></ol>","split");
    assert!(deterministic(&book(), &p.source).unwrap().is_empty());
}
#[test]
fn nested_number_is_independent_and_publication_evidence_cannot_set_main_rank() {
    let (p, mut finding) = prose();
    let mut web = MockWeb::new(vec![p.clone()]);
    web.findings
        .insert(p.source.id.clone(), vec![finding.clone()]);
    let (run, _) = collect(&book(), &AppData::default(), &mut web).unwrap();
    assert_eq!(run.field_mask, [false, true, false, false]);
    assert_eq!(run.hierarchy[1].index, Some(2.0));
    finding.candidate.fields[0].slot = 0;
    finding.candidate.fields[0].name = "Night Cycle".into();
    finding.scope = "Night Cycle".into();
    finding.order_kind = "publication".into();
    finding.primary_basis = "explicit_number".into();
    finding.chronology_citations = vec![quote(&p.source, 1)];
    assert!(validate_finding(&book(), &p.source, &finding).is_err());
    let mut b = book();
    b.hierarchy[0].name = Some("Larger Universe".into());
    let mut web = MockWeb::new(vec![complete("narrow")]);
    let (run, _) = collect(&b, &AppData::default(), &mut web).unwrap();
    assert_eq!(run.status, "unresolved");
    assert!(!run.field_mask[0]);
}
#[test]
fn fabricated_identity_quotes_and_conflicting_sources_are_held() {
    let (p, mut finding) = prose();
    finding.candidate.citations[0].quote = "Invented quotation".into();
    assert!(validate_finding(&book(), &p.source, &finding).is_err());
    let (p, mut finding) = prose();
    finding.candidate.authors = vec!["Another Author".into()];
    assert!(validate_finding(&book(), &p.source, &finding).is_err());
    let mut web = MockWeb::new(vec![
        complete("one"),
        timeline(
            "Complete chronological order of Fixture Universe",
            "<li>Prequel</li><li>First Published</li>",
            "two",
        ),
    ]);
    let (run, _) = collect(&book(), &AppData::default(), &mut web).unwrap();
    assert_eq!(run.status, "conflict");
    assert_eq!(run.field_mask, [false; 4]);
}
#[test]
fn shortened_field_citations_expand_only_within_the_same_cited_segment() {
    let (p, mut finding) = prose();
    finding.candidate.fields[0].citations[0].quote = "book 2".into();
    assert!(validate_finding(&book(), &p.source, &finding).is_err());
    complete_context(&mut finding, &p.source).unwrap();
    validate_finding(&book(), &p.source, &finding).unwrap();
    assert!(finding.candidate.notes.contains("Backend expanded"));
    finding.candidate.fields[0].citations[0].quote = "Fabricated Night Cycle book 2".into();
    assert!(complete_context(&mut finding, &p.source).is_err());
}
#[test]
fn relative_order_requires_explicit_direction_two_current_anchors_and_no_ties() {
    let p=page("<p>Chronological story order in Fixture Universe. First Published by Fixture Author. First Published is set after Earlier. First Published is set before Later.</p>","relative");
    let f = Finding {
        candidate: Candidate {
            title: book().title,
            authors: book().authors,
            edition: "novel".into(),
            fields: vec![ExtractedField {
                slot: 0,
                name: "Fixture Universe".into(),
                index: None,
                citations: vec![quote(&p.source, 0)],
            }],
            citations: vec![quote(&p.source, 0)],
            notes: "Relative placement, no global number".into(),
        },
        order_kind: "chronological".into(),
        scope: "Fixture Universe".into(),
        primary_basis: "relative_only".into(),
        chronology_citations: vec![quote(&p.source, 0)],
        before_titles: vec!["Later".into()],
        after_titles: vec!["Earlier".into()],
    };
    let mut data = AppData::default();
    for (title, number) in [("Earlier", 2.0), ("Later", 4.0)] {
        let mut r = Reference {
            title: title.into(),
            authors: book().authors,
            approved: true,
            evidence: "Reviewed fixture anchor".into(),
            ..Default::default()
        };
        r.hierarchy[0] = SeriesValue {
            name: Some("Fixture Universe".into()),
            index: Some(number),
        };
        data.references.push(r);
    }
    let mut web = MockWeb::new(vec![p.clone()]);
    web.findings.insert(p.source.id.clone(), vec![f.clone()]);
    let (run, fetched) = collect(&book(), &data, &mut web).unwrap();
    assert_eq!(run.hierarchy[0].index, Some(3.0));
    assert_eq!(run.anchors.len(), 2);
    data.sources = fetched;
    validate_run(&run, &book(), &data).unwrap();
    data.references[0].approved = false;
    assert!(validate_run(&run, &book(), &data).is_err());
    let mut reversed = f;
    reversed.before_titles = vec!["Earlier".into()];
    assert!(validate_finding(&book(), &p.source, &reversed).is_err());
}
#[test]
fn blocked_search_and_unsafe_redirects_do_not_require_browser_verification() {
    let mut web = MockWeb::new(vec![]);
    web.blocked = true;
    let (run, _) = collect(&book(), &AppData::default(), &mut web).unwrap();
    assert_eq!(run.status, "blocked");
    assert_eq!(web.fetches, 0);
    let base = reqwest::Url::parse("https://author.example/").unwrap();
    for url in [
        "http://author.example/",
        "https://127.0.0.1/",
        "https://localhost/",
        "file:///etc/passwd",
        "https://user:pass@author.example/",
    ] {
        assert!(redirect_destination(&base, url).is_err());
    }
    assert_eq!(
        redirect_destination(&base, "/chronology").unwrap().as_str(),
        "https://author.example/chronology"
    );
    assert!(fetch_page("https://127.0.0.1/").is_err());
}
#[test]
fn table_headers_bind_positions_without_reinterpreting_fictional_years() {
    let p=page("<p>Fixture Author</p><h1>Chronological order of Fixture Universe</h1><table><tr><th>Title</th><th>Chronological position</th><th>Fictional year</th></tr><tr><td>First Published</td><td>3</td><td>2850</td></tr></table>","table");
    let row = p
        .source
        .segments
        .iter()
        .find(|s| s.kind == "tr" && s.text.contains("First Published"))
        .unwrap();
    assert!(row.text.contains("Chronological position: 3"));
    assert!(row.text.contains("Fictional year: 2850"));
    let finding = Finding {
        candidate: Candidate {
            title: book().title,
            authors: book().authors,
            edition: "novel".into(),
            fields: vec![ExtractedField {
                slot: 0,
                name: "Fixture Universe".into(),
                index: Some(3.0),
                citations: vec![
                    quote(&p.source, 1),
                    Citation {
                        segment: row.ordinal,
                        quote: row.text.clone(),
                    },
                ],
            }],
            citations: vec![
                quote(&p.source, 0),
                Citation {
                    segment: row.ordinal,
                    quote: row.text.clone(),
                },
            ],
            notes: String::new(),
        },
        order_kind: "chronological".into(),
        scope: "Fixture Universe".into(),
        primary_basis: "explicit_number".into(),
        chronology_citations: vec![quote(&p.source, 1)],
        before_titles: vec![],
        after_titles: vec![],
    };
    validate_finding(&book(), &p.source, &finding).unwrap();
    let mut wrong = finding;
    wrong.candidate.fields[0].index = Some(2850.0);
    assert!(validate_finding(&book(), &p.source, &wrong).is_err());
}
#[test]
fn a_sourced_nested_label_can_promote_an_unverified_old_main_series() {
    let p=page("<p>First Published by Fixture Author. Chronological order of Fixture Universe: First Published has chronological position 3 and is Night Cycle book 2.</p>","promotion");
    let proof = quote(&p.source, 0);
    let finding = Finding {
        candidate: Candidate {
            title: book().title,
            authors: book().authors,
            edition: "novel".into(),
            fields: vec![
                ExtractedField {
                    slot: 0,
                    name: "Fixture Universe".into(),
                    index: Some(3.0),
                    citations: vec![proof.clone()],
                },
                ExtractedField {
                    slot: 1,
                    name: "Night Cycle".into(),
                    index: Some(2.0),
                    citations: vec![proof.clone()],
                },
            ],
            citations: vec![proof.clone()],
            notes: String::new(),
        },
        order_kind: "chronological".into(),
        scope: "Fixture Universe".into(),
        primary_basis: "explicit_number".into(),
        chronology_citations: vec![proof],
        before_titles: vec![],
        after_titles: vec![],
    };
    let mut b = book();
    b.hierarchy[0] = SeriesValue {
        name: Some("Night Cycle".into()),
        index: Some(2.0),
    };
    let mut web = MockWeb::new(vec![p.clone()]);
    web.findings
        .insert(p.source.id.clone(), vec![finding.clone()]);
    let (run, _) = collect(&b, &AppData::default(), &mut web).unwrap();
    assert_eq!(run.field_mask, [true, true, false, false]);
    assert_eq!(run.hierarchy[0].index, Some(3.0));
    let mut uncertain = finding;
    uncertain.candidate.edition = "collection".into();
    let mut web = MockWeb::new(vec![p.clone()]);
    web.findings.insert(p.source.id.clone(), vec![uncertain]);
    let (run, _) = collect(&b, &AppData::default(), &mut web).unwrap();
    assert_eq!(run.field_mask, [false, true, false, false]);
    assert!(run.notes.iter().any(|n| n.contains("Collection")));
}
#[test]
fn cached_pages_and_queries_drive_the_actual_optional_model_path_without_internet() {
    use serde_json::{json, Value};
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };
    let (p, f) = prose();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    let worker = thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap().to_string();
            let mut len = 0;
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse::<usize>().unwrap();
                }
            }
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes).unwrap();
            let body = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
            recorded.lock().unwrap().push((path.clone(), body));
            let output = match path.as_str() {
                "/api/tags" => {
                    json!({"models":[{"name":"fixture:latest","digest":"a".repeat(64),"size":2000000,"details":{"format":"gguf"},"capabilities":["completion"]}]})
                }
                "/api/show" => {
                    json!({"model_info":{"general.parameter_count":123},"capabilities":["completion"]})
                }
                _ => {
                    json!({"done":true,"done_reason":"stop","message":{"content":serde_json::to_string(&Findings{findings:vec![f.clone()],notes:String::new()}).unwrap()}})
                }
            };
            let body = output.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::open(temp.path(), temp.path().join("unused")).unwrap();
    app.scan = Some(Scan {
        library: app.data.settings.library.clone(),
        books: vec![book()],
        columns: vec![],
    });
    app.data.ai_settings = curator_core::ai::AiSettings {
        enabled: true,
        endpoint,
        model: "fixture:latest".into(),
        ..Default::default()
    };
    for query in [
        "\"First Published\" Fixture Author chronological order subseries",
        "Fixture Author  universe complete chronology bibliography",
    ] {
        let result = ResearchRun {
            query: query.into(),
            fetched_at: metadata::now(),
            cached: false,
            status: "complete".into(),
            notes: vec![],
            leads: vec![Lead {
                url: p.source.location.clone(),
                title: "Fixture".into(),
                engine: "DuckDuckGo fixture".into(),
            }],
        };
        app.store
            .cache_value(
                &format!("research:{}", sources::hash(query.as_bytes())),
                &result,
            )
            .unwrap();
    }
    app.store
        .cache_value(
            &format!(
                "research:page:{}",
                sources::hash(p.source.location.as_bytes())
            ),
            &p,
        )
        .unwrap();
    app.research_book(1, false).unwrap();
    assert_eq!(app.data.chronology_runs[0].status, "ready");
    app.research_book(1, false).unwrap();
    assert!(app.data.chronology_runs[0].cached);
    worker.join().unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|(path, _)| path == "/api/chat").count(),
        1
    );
    let body = &calls
        .iter()
        .find(|(path, _)| path == "/api/chat")
        .unwrap()
        .1;
    assert!(body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("Night Cycle"));
    assert!(!body.to_string().contains("fictional-book"));
    assert!(app.data.references.is_empty());
}
#[test]
#[ignore = "Explicit bounded public source fetch; no library access"]
fn public_timeline_does_not_turn_fictional_years_into_book_positions() {
    let p = fetch_page("https://larryniven.net/?q=lensmans-known-space-chronology").unwrap();
    let mut b = book();
    b.title = "Ringworld".into();
    b.authors = vec!["Larry Niven".into()];
    assert!(!p.source.segments.is_empty());
    assert!(deterministic(&b, &p.source).unwrap().is_empty());
    println!(
        "Fetched {} source segments; no invented global book rank from fictional-year rows",
        p.source.segments.len()
    );
}
