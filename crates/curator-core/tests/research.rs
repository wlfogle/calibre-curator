use curator_core::{app::App, research::*, store::Store};
#[test]
fn parses_structured_links_html_redirects_and_blocks_without_inventing_results() {
    let mut leads=instant(br#"{"Results":[{"FirstURL":"https://author.example/","Text":"Author"},{"FirstURL":"http://author.example/","Text":"Duplicate"}],"AbstractURL":"https://encyclopedia.example/author","Heading":"Author","RelatedTopics":[{"Topics":[{"FirstURL":"https://reference.example/","Text":"Reference"}]}]}"#).unwrap();
    deduplicate(&mut leads);
    assert_eq!(leads.len(), 3);
    let (blocked, parsed) = html(202, b"<form class='challenge-form'>verify</form>").unwrap();
    assert!(blocked);
    assert!(parsed.is_empty());
    assert!(
        html(200, b"<div class='anomaly-modal'>challenge</div>")
            .unwrap()
            .0
    );
    let (blocked,parsed)=html(200,br#"<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fauthor.example%2F">Author</a><a class="result__a" href="javascript:bad">bad</a>"#).unwrap();
    assert!(!blocked);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].url, "https://author.example/");
    let mut variants = vec![
        Lead {
            url: "http://author.example/".into(),
            title: "HTTP".into(),
            engine: "fixture".into(),
        },
        Lead {
            url: "https://author.example/".into(),
            title: "HTTPS".into(),
            engine: "fixture".into(),
        },
    ];
    deduplicate(&mut variants);
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].title, "HTTPS");
    for raw in [
        "file:///etc/passwd",
        "https://user:pass@example.com/",
        "http://localhost/",
        "https://127.0.0.1/",
        "https://10.0.0.1/",
        "https://service.local/",
        "data:text/html,bad",
    ] {
        assert!(safe_link(raw).is_none(), "{raw}");
    }
}
#[test]
fn research_cache_survives_ai_clear_and_cached_block_does_not_hit_network() {
    let temp = tempfile::tempdir().unwrap();
    let run = ResearchRun {
        query: "Fixture".into(),
        fetched_at: curator_core::metadata::now(),
        cached: false,
        status: "blocked".into(),
        notes: vec!["Blocked fixture".into()],
        leads: vec![],
    };
    let key = format!("research:{}", curator_core::sources::hash(b"Fixture"));
    let store = Store::open(temp.path()).unwrap();
    store.cache_value(&key, &run).unwrap();
    store.clear_ai_cache().unwrap();
    assert!(store.cached_value::<ResearchRun>(&key).unwrap().is_some());
    drop(store);
    let mut app = App::open(temp.path(), temp.path().join("unused")).unwrap();
    app.discover_sites("Fixture", false).unwrap();
    assert!(app.data.research_runs[0].cached);
    assert_eq!(app.data.research_runs[0].status, "blocked");
}
#[test]
#[ignore = "Explicit public DuckDuckGo request; no library data"]
fn public_author_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::open(temp.path(), temp.path().join("unused")).unwrap();
    app.discover_sites("Larry Niven", true).unwrap();
    let run = &app.data.research_runs[0];
    println!(
        "Status: {}; leads: {}; notes: {:?}",
        run.status,
        run.leads.len(),
        run.notes
    );
    assert!(run.leads.iter().any(|l| l.url.contains("larryniven.net")));
}
