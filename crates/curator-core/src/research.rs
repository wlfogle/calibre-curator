//! Best-effort DuckDuckGo discovery. Results are leads, never approved evidence.
use crate::{app::App, metadata, sources};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lead {
    pub url: String,
    pub title: String,
    pub engine: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchRun {
    pub query: String,
    pub fetched_at: u64,
    pub cached: bool,
    pub status: String,
    pub notes: Vec<String>,
    pub leads: Vec<Lead>,
}
pub fn safe_link(raw: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    if host == "localhost"
        || !host.contains('.')
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return None;
    }
    if url.port().is_some() {
        return None;
    }
    url.set_fragment(None);
    Some(url.into())
}
fn push(leads: &mut Vec<Lead>, raw: &str, title: &str, engine: &str) {
    if leads.len() >= 30 {
        return;
    }
    if let Some(url) = safe_link(raw) {
        leads.push(Lead {
            url,
            title: title.chars().take(500).collect(),
            engine: engine.into(),
        });
    }
}
pub fn instant(bytes: &[u8]) -> Result<Vec<Lead>> {
    let value: Value = serde_json::from_slice(bytes)?;
    let mut leads = Vec::new();
    for row in value["Results"].as_array().into_iter().flatten().take(30) {
        push(
            &mut leads,
            row["FirstURL"].as_str().unwrap_or(""),
            row["Text"].as_str().unwrap_or(""),
            "DuckDuckGo Instant Answer",
        );
    }
    push(
        &mut leads,
        value["AbstractURL"].as_str().unwrap_or(""),
        value["Heading"].as_str().unwrap_or(""),
        "DuckDuckGo Instant Answer",
    );
    fn topics(value: &Value, leads: &mut Vec<Lead>, depth: usize) {
        if depth > 3 || leads.len() >= 30 {
            return;
        }
        for row in value.as_array().into_iter().flatten().take(30) {
            push(
                leads,
                row["FirstURL"].as_str().unwrap_or(""),
                row["Text"].as_str().unwrap_or(""),
                "DuckDuckGo related topic",
            );
            topics(&row["Topics"], leads, depth + 1);
        }
    }
    topics(&value["RelatedTopics"], &mut leads, 0);
    Ok(leads)
}
pub fn html(status: u16, bytes: &[u8]) -> Result<(bool, Vec<Lead>)> {
    let text = std::str::from_utf8(bytes)?;
    if status == 202
        || [
            "anomaly-modal",
            "challenge-form",
            "Please verify you are a human",
        ]
        .iter()
        .any(|s| text.contains(s))
    {
        return Ok((true, Vec::new()));
    }
    let doc = scraper::Html::parse_document(text);
    let selector = scraper::Selector::parse("a.result__a").expect("constant selector");
    let mut leads = Vec::new();
    for node in doc.select(&selector).take(30) {
        let raw = node.value().attr("href").unwrap_or("");
        let Ok(url) = reqwest::Url::parse("https://duckduckgo.com/")?.join(raw) else {
            continue;
        };
        let target = if url.host_str() == Some("duckduckgo.com") && url.path() == "/l/" {
            url.query_pairs()
                .find(|(k, _)| k == "uddg")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        } else {
            url.into()
        };
        push(
            &mut leads,
            &target,
            &node.text().collect::<String>(),
            "DuckDuckGo HTML search",
        );
    }
    Ok((false, leads))
}
pub fn deduplicate(leads: &mut Vec<Lead>) {
    let key = |url: &str| {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    };
    let mut unique: Vec<Lead> = Vec::new();
    for lead in leads.drain(..) {
        if let Some(existing) = unique
            .iter_mut()
            .find(|other| key(&other.url) == key(&lead.url))
        {
            if existing.url.starts_with("http://") && lead.url.starts_with("https://") {
                *existing = lead;
            }
        } else {
            unique.push(lead);
        }
    }
    *leads = unique;
}
impl App {
    pub fn discover_sites(&mut self, query: &str, refresh: bool) -> Result<()> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 400 || query.chars().any(char::is_control) {
            bail!("Use a nonempty research query of at most 400 characters");
        }
        let key = format!("research:{}", sources::hash(query.as_bytes()));
        if !refresh {
            if let Some(mut old) = self.store.cached_value::<ResearchRun>(&key)? {
                let lifetime = if old.status == "complete" { 86400 } else { 300 };
                if metadata::now().saturating_sub(old.fetched_at) < lifetime {
                    old.cached = true;
                    self.save_research(old)?;
                    return Ok(());
                }
            }
        }
        let mut run = ResearchRun {
            query: query.into(),
            fetched_at: metadata::now(),
            cached: false,
            status: "partial".into(),
            notes: Vec::new(),
            leads: Vec::new(),
        };
        let mut api = reqwest::Url::parse("https://api.duckduckgo.com/")?;
        api.query_pairs_mut().extend_pairs([
            ("q", query),
            ("format", "json"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ]);
        match sources::public_bytes(api.as_str()).and_then(|(_, b)| instant(&b)) {
            Ok(leads) => run.leads.extend(leads),
            Err(e) => run.notes.push(format!("Instant Answers unavailable: {e}")),
        }
        let mut search = reqwest::Url::parse("https://html.duckduckgo.com/html/")?;
        search.query_pairs_mut().append_pair("q", query);
        match sources::public_bytes(search.as_str()).and_then(|(s, b)| html(s, &b)) {
            Ok((true, _)) => run.notes.push("Full search blocked by DuckDuckGo verification; no retry loop or manual challenge required. Instant Answers are not full web search.".into()),
            Ok((false, leads)) => {
                if leads.is_empty() { run.notes.push("No HTML results parsed; full search coverage is unknown.".into()); }
                else { run.status = "complete".into(); run.leads.extend(leads); }
            }
            Err(e) => run.notes.push(format!("Full search unavailable: {e}")),
        }
        deduplicate(&mut run.leads);
        if run.leads.is_empty() && run.status != "complete" {
            run.status = "blocked".into();
        }
        self.store.cache_value(&key, &run)?;
        self.save_research(run)
    }
    fn save_research(&mut self, run: ResearchRun) -> Result<()> {
        let mut next = self.data.clone();
        next.research_runs.retain(|r| r.query != run.query);
        next.research_runs.insert(0, run);
        next.research_runs.truncate(20);
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
}
