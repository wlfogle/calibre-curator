use crate::{
    matching::{isbn, normalize},
    model::*,
    sources::hash,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LIMIT: usize = 2 * 1024 * 1024;
pub const PROVIDER_CODE:&str="import json; from calibre.customize.ui import all_metadata_plugins,is_disabled; print('CURATOR_PLUGINS:'+json.dumps([{'name':p.name,'identify':'identify' in p.capabilities,'enabled':not is_disabled(p),'version':'.'.join(map(str,p.version))} for p in all_metadata_plugins()]))";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataSettings {
    pub fetch_binary: String,
    pub search_all: bool,
    pub providers: Vec<String>,
    pub timeout_seconds: u64,
    pub cache_days: u64,
}
pub fn select_providers(
    settings: &MetadataSettings,
    providers: &[Provider],
) -> Result<Vec<Provider>> {
    let mut names = std::collections::BTreeSet::new();
    let chosen = if settings.search_all {
        providers
            .iter()
            .filter(|p| p.enabled && p.identify && names.insert(p.name.clone()))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        settings
            .providers
            .iter()
            .filter(|name| names.insert((*name).clone()))
            .map(|name| {
                providers
                    .iter()
                    .find(|p| &p.name == name && p.enabled && p.identify)
                    .cloned()
                    .with_context(|| {
                        format!(
                            "{name} is not an enabled identify provider; configure it in Calibre"
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?
    };
    if chosen.is_empty() {
        bail!(
            "No enabled identification providers selected; discover/configure providers in Calibre"
        );
    }
    Ok(chosen)
}
pub fn duckduckgo_url(book: &Book) -> Result<reqwest::Url> {
    if book.title.trim().is_empty() || book.title.len() > 1000 {
        bail!("Book title is missing or too long for web search");
    }
    let mut query = format!("\"{}\" {}", book.title, book.authors.join(" "));
    if let Some(isbn) = safe_ids(&book.identifiers).get("isbn") {
        query.push(' ');
        query.push_str(isbn);
    }
    let mut url = reqwest::Url::parse("https://duckduckgo.com/")?;
    url.query_pairs_mut().append_pair("q", &query);
    Ok(url)
}
impl Default for MetadataSettings {
    fn default() -> Self {
        Self {
            fetch_binary: "/usr/bin/fetch-ebook-metadata".into(),
            search_all: true,
            providers: Vec::new(),
            timeout_seconds: 15,
            cache_days: 7,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub version: String,
    pub identify: bool,
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchedMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub identifiers: BTreeMap<String, String>,
    pub publisher: Option<String>,
    pub description: String,
    pub url: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedMetadata {
    pub fetched_at: u64,
    pub metadata: FetchedMetadata,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LookupResult {
    pub id: String,
    pub library: String,
    pub book_id: i64,
    pub book_uuid: String,
    pub book_fingerprint: String,
    pub provider: Provider,
    pub query_title: String,
    pub query_authors: Vec<String>,
    pub query_identifiers: BTreeMap<String, String>,
    pub fetched_at: u64,
    pub cached: bool,
    pub metadata: Option<FetchedMetadata>,
    pub source_id: Option<String>,
    pub match_status: String,
    pub note: String,
    pub identity_confirmed: bool,
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn binary(path: &str, name: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.is_absolute()
        || !path.is_file()
        || path.file_name().and_then(|p| p.to_str()) != Some(name)
    {
        bail!("Choose the absolute installed {name} executable");
    }
    Ok(())
}
pub fn run_bounded(command: &mut Command, timeout: Duration) -> Result<Vec<u8>> {
    command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("Cannot start Calibre metadata command")?;
    let pid = child.id();
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = overflow.clone();
    let mut stdout = child.stdout.take().context("No metadata stdout")?;
    let out = thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let n = stdout.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            if bytes.len() + n > LIMIT {
                flag.store(true, Ordering::SeqCst);
            } else {
                bytes.extend_from_slice(&buffer[..n]);
            }
        }
        Ok(bytes)
    });
    let mut stderr = child.stderr.take().context("No metadata stderr")?;
    // Plugins can log credentials: drain diagnostics, never retain or display them.
    let err = thread::spawn(move || {
        let mut buffer = [0; 8192];
        while let Ok(n) = stderr.read(&mut buffer) {
            if n == 0 {
                break;
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut stopped = false;
    let status = loop {
        if overflow.load(Ordering::SeqCst) || Instant::now() >= deadline {
            // This group was created for this child, not an existing Calibre GUI/job.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            stopped = true;
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            while !(out.is_finished() && err.is_finished()) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if !(out.is_finished() && err.is_finished()) {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
                stopped = true;
            }
            break status;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let bytes = out
        .join()
        .map_err(|_| anyhow::anyhow!("Metadata reader failed"))??;
    let _ = err.join();
    if overflow.load(Ordering::SeqCst) {
        bail!("Metadata output exceeded 2 MiB");
    }
    if stopped {
        bail!("Metadata command timed out; only its own process group was stopped");
    }
    if !status.success() {
        bail!("Provider returned no result or failed (status {status}); check its Calibre configuration");
    }
    Ok(bytes)
}
pub fn discover(calibre_binary: &str) -> Result<Vec<Provider>> {
    binary(calibre_binary, "calibre-debug")?;
    let output = run_bounded(
        Command::new(calibre_binary).args(["-c", PROVIDER_CODE]),
        Duration::from_secs(15),
    )?;
    let text = String::from_utf8_lossy(&output);
    let line = text
        .lines()
        .find_map(|s| s.strip_prefix("CURATOR_PLUGINS:"))
        .context("Calibre did not return its provider list")?;
    let providers: Vec<Provider> = serde_json::from_str(line)?;
    if providers.len() > 200 {
        bail!("Unexpected provider count");
    }
    Ok(providers)
}
pub fn catalogue(book: &Book) -> bool {
    book.authors.iter().any(|a| normalize(a) == "calibre")
        || (!book.formats.is_empty()
            && book
                .formats
                .iter()
                .all(|f| matches!(f.as_str(), "CSV" | "XML")))
}
pub fn book_fingerprint(book: &Book) -> Result<String> {
    Ok(hash(&serde_json::to_vec(&(
        &book.uuid,
        &book.title,
        &book.authors,
        &book.identifiers,
    ))?))
}
pub fn safe_ids(ids: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    ids.iter()
        .filter_map(|(key, value)| {
            let key = key.to_ascii_lowercase();
            if key.starts_with("isbn") {
                return isbn(value).map(|v| ("isbn".into(), v));
            }
            let valid = match key.as_str() {
                "asin" | "amazon" | "mobi-asin" => {
                    value.len() == 10 && value.bytes().all(|b| b.is_ascii_alphanumeric())
                }
                "goodreads" | "isfdb" => {
                    !value.is_empty()
                        && value.len() < 24
                        && value.bytes().all(|b| b.is_ascii_digit())
                }
                "storygraph" => uuid::Uuid::parse_str(value).is_ok(),
                "google" | "openlibrary" | "openlibraryplus" => {
                    !value.is_empty()
                        && value.len() <= 128
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                }
                _ => false,
            };
            valid.then(|| {
                (
                    if key == "mobi-asin" {
                        "asin".into()
                    } else {
                        key
                    },
                    value.clone(),
                )
            })
        })
        .collect()
}
pub fn fetch(
    config: &MetadataSettings,
    provider: &Provider,
    book: &Book,
) -> Result<FetchedMetadata> {
    binary(&config.fetch_binary, "fetch-ebook-metadata")?;
    if !(5..=30).contains(&config.timeout_seconds) || config.cache_days > 30 {
        bail!("Provider timeout must be 5–30 seconds and cache retention 0–30 days");
    }
    if !provider.enabled || !provider.identify {
        bail!("Choose an enabled identification provider in Calibre");
    }
    if book.title.len() > 1000 || book.authors.iter().map(String::len).sum::<usize>() > 2000 {
        bail!("Book identity exceeds lookup limits");
    }
    let mut command = Command::new(&config.fetch_binary);
    command
        .arg(format!("--allowed-plugin={}", provider.name))
        .arg("--opf")
        .arg(format!("--timeout={}", config.timeout_seconds))
        .arg(format!("--title={}", book.title))
        .arg(format!("--authors={}", book.authors.join(" & ")));
    for (key, value) in safe_ids(&book.identifiers) {
        if key == "isbn" {
            command.arg(format!("--isbn={value}"));
        } else {
            command.arg(format!("--identifier={key}:{value}"));
        }
    }
    parse_opf(&run_bounded(
        &mut command,
        Duration::from_secs(config.timeout_seconds + 5),
    )?)
}
pub fn plain_description(html: &str) -> String {
    let pattern=regex::Regex::new(r"(?is)<script\b[^>]*>.*?</script>|<style\b[^>]*>.*?</style>|<noscript\b[^>]*>.*?</noscript>").expect("static HTML filter");
    let filtered = pattern.replace_all(html, " ");
    let document = scraper::Html::parse_fragment(&filtered);
    document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn parse_opf(bytes: &[u8]) -> Result<FetchedMetadata> {
    if bytes.len() > LIMIT {
        bail!("Metadata exceeds output limit");
    }
    let raw = std::str::from_utf8(bytes).context("Metadata is not UTF-8")?;
    let start = raw
        .find("<package")
        .context("Provider returned no OPF package")?;
    let end = raw.rfind("</package>").context("Incomplete metadata OPF")? + 10;
    if end <= start {
        bail!("Invalid metadata package");
    }
    let document =
        roxmltree::Document::parse(&raw[start..end]).context("Invalid provider OPF XML")?;
    let dc = "http://purl.org/dc/elements/1.1/";
    let values = |tag: &str| {
        document
            .descendants()
            .filter(|n| n.has_tag_name((dc, tag)))
            .map(|n| {
                n.descendants()
                    .filter(|c| c.is_text())
                    .filter_map(|c| c.text())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
    };
    let title = values("title")
        .into_iter()
        .next()
        .context("Metadata title missing")?;
    let authors = values("creator");
    let mut ids = BTreeMap::new();
    for node in document
        .descendants()
        .filter(|n| n.has_tag_name((dc, "identifier")))
    {
        if let Some(scheme) = node.attribute(("http://www.idpf.org/2007/opf", "scheme")) {
            ids.insert(
                scheme.to_ascii_lowercase(),
                node.text().unwrap_or("").to_string(),
            );
        }
    }
    let identifiers = safe_ids(&ids);
    let description = plain_description(&values("description").join("\n"));
    if normalize(&title).is_empty()
        || title.len() > 1000
        || authors.len() > 30
        || authors.iter().any(|s| s.len() > 300)
        || description.len() > 32_000
    {
        bail!("Provider metadata fields exceed limits");
    }
    let url = identifiers
        .get("google")
        .map(|id| format!("https://books.google.com/books?id={id}"))
        .or_else(|| {
            identifiers
                .get("goodreads")
                .map(|id| format!("https://www.goodreads.com/book/show/{id}"))
        })
        .or_else(|| {
            identifiers
                .get("asin")
                .or_else(|| identifiers.get("amazon"))
                .map(|id| format!("https://www.amazon.com/dp/{id}"))
        });
    Ok(FetchedMetadata {
        title,
        authors,
        identifiers,
        publisher: values("publisher").into_iter().next(),
        description,
        url,
    })
}
fn author_key(author: &str) -> String {
    if let Some((last, first)) = author.split_once(',') {
        normalize(&format!("{first} {last}"))
    } else {
        normalize(author)
    }
}
pub fn identity(book: &Book, metadata: &FetchedMetadata) -> (&'static str, String) {
    let a = normalize(&book.title);
    let b = normalize(&metadata.title);
    let ids = safe_ids(&book.identifiers);
    let strong = ids
        .iter()
        .any(|(key, value)| metadata.identifiers.get(key) == Some(value));
    let decorated_a = book.title.contains([':', '(']);
    let decorated_b = metadata.title.contains([':', '(']);
    let base = |s: &str| normalize(s.split([':', '(']).next().unwrap_or(s));
    let plain = if decorated_a { &b } else { &a };
    let title_ok = a == b
        || (strong
            && decorated_a != decorated_b
            && plain.len() >= 6
            && !["starwars", "startrek", "forgottenrealms", "warhammer40000"]
                .contains(&plain.as_str())
            && base(&book.title) == base(&metadata.title));
    let authors_ok = book.authors.iter().any(|a| {
        !matches!(
            normalize(a).as_str(),
            "unknown" | "unknownauthor" | "calibre"
        ) && metadata
            .authors
            .iter()
            .any(|b| author_key(a) == author_key(b))
    });
    if !title_ok || !authors_ok {
        return ("mismatch","Returned title/author does not reliably match this record; no synopsis can be generated.".into());
    }
    if ids.iter().any(|(key, value)| {
        metadata
            .identifiers
            .get(key)
            .is_some_and(|other| other != value)
    }) {
        return (
            "different_edition",
            "A returned identifier differs; verify the edition or repair stale identifiers before using this evidence.".into(),
        );
    }
    ("compatible",if strong {"Recognized identifier plus title/author consistency; verify edition and scope."}
        else {"Title/author match only; independently verify edition, bundles and authorship before confirming."}.into())
}
