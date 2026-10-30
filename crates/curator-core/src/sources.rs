use crate::{
    matching::{normalize, validate_reference},
    model::*,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    path::Path,
};

const MAX: u64 = 16 * 1024 * 1024;
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded(mut input: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut input).take(MAX + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX {
        bail!("Source exceeds the 16 MiB safety limit");
    }
    Ok(bytes)
}
fn text(node: roxmltree::Node<'_, '_>) -> String {
    if node.is_text() {
        return node.text().unwrap_or("").into();
    }
    if node.has_tag_name(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "s")) {
        let count = node
            .attribute(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "c"))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1)
            .min(100);
        return " ".repeat(count);
    }
    node.children().map(text).collect::<Vec<_>>().join("")
}
fn odt(bytes: &[u8]) -> Result<Vec<Segment>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let content = bounded(
        archive
            .by_name("content.xml")
            .context("Not an ODT: missing content.xml")?,
    )?;
    let document = roxmltree::Document::parse(std::str::from_utf8(&content)?)?;
    let mut segments = Vec::new();
    for node in document.descendants().filter(|n| n.is_element()) {
        let kind = match node.tag_name().name() {
            "h" => "heading",
            "p" if !node.ancestors().any(|n| {
                n.has_tag_name((
                    "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
                    "table-row",
                ))
            }) =>
            {
                "paragraph"
            }
            "table-row" => "row",
            _ => continue,
        };
        let value = if kind == "row" {
            node.children()
                .filter(|n| n.is_element())
                .map(text)
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            text(node)
        };
        if !value.trim().is_empty() {
            segments.push(Segment {
                ordinal: segments.len() + 1,
                kind: kind.into(),
                text: value.trim().into(),
            });
        }
    }
    Ok(segments)
}
fn html(bytes: &[u8]) -> Result<Vec<Segment>> {
    let document = scraper::Html::parse_document(std::str::from_utf8(bytes)?);
    let selector = scraper::Selector::parse("h1,h2,h3,h4,h5,h6,p,tr,li")
        .map_err(|_| anyhow::anyhow!("Invalid built-in selector"))?;
    Ok(document
        .select(&selector)
        .enumerate()
        .filter_map(|(i, element)| {
            let value = element.text().collect::<Vec<_>>().join(" ");
            (!value.trim().is_empty()).then(|| Segment {
                ordinal: i + 1,
                kind: element.value().name().into(),
                text: value.trim().into(),
            })
        })
        .collect())
}
fn parse(
    bytes: &[u8],
    location: &str,
    kind: &str,
    universe: &str,
    scan: Option<&Scan>,
) -> Result<(Source, Vec<Reference>)> {
    let id = new_id();
    let mut source = Source {
        id: id.clone(),
        name: Path::new(location)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        location: location.into(),
        sha256: hash(bytes),
        kind: kind.into(),
        segments: Vec::new(),
    };
    let mut references = Vec::new();
    match kind {
        "json" => {
            let value: Value = serde_json::from_slice(bytes)?;
            let rows = value
                .as_array()
                .or_else(|| value.get("references").and_then(Value::as_array))
                .context("JSON source must be an array of references or {references:[...]}")?;
            for row in rows {
                let mut reference: Reference = serde_json::from_value(row.clone())?;
                reference.approved = false;
                reference.source_id = id.clone();
                reference.origin = "imported research (not certified)".into();
                validate_reference(&reference)?;
                references.push(reference);
            }
            source.segments.push(Segment {
                ordinal: 1,
                kind: "json".into(),
                text: std::str::from_utf8(bytes)?.into(),
            });
        }
        "csv" => {
            let mut reader = csv::Reader::from_reader(bytes);
            for row in reader.deserialize::<BTreeMap<String, String>>() {
                let row = row?;
                let get = |key: &str| row.get(key).map(String::as_str).unwrap_or("").trim();
                let mut reference = Reference {
                    title: get("title").into(),
                    authors: get("authors")
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                    source_id: id.clone(),
                    evidence: get("evidence").into(),
                    origin: "CSV research (not certified)".into(),
                    ..Default::default()
                };
                for i in 0..4 {
                    let name_key = if i == 0 {
                        "universe".into()
                    } else {
                        format!("series{}", i + 1)
                    };
                    let index_key = if i == 0 {
                        "position".into()
                    } else {
                        format!("index{}", i + 1)
                    };
                    if !get(&name_key).is_empty() {
                        reference.hierarchy[i] = SeriesValue {
                            name: Some(get(&name_key).into()),
                            index: if get(&index_key).is_empty() {
                                None
                            } else {
                                Some(get(&index_key).parse()?)
                            },
                        };
                    }
                }
                if !get("isbn").is_empty() {
                    reference
                        .identifiers
                        .insert("isbn".into(), get("isbn").into());
                }
                if !get("uuid").is_empty() {
                    reference.book_uuids.push(get("uuid").into());
                }
                validate_reference(&reference)?;
                references.push(reference);
            }
            source.segments.push(Segment {
                ordinal: 1,
                kind: "csv".into(),
                text: std::str::from_utf8(bytes)?.into(),
            });
        }
        "odt" => source.segments = odt(bytes)?,
        "html" | "htm" => source.segments = html(bytes)?,
        _ => bail!("Supported sources are JSON, CSV, ODT and saved HTML"),
    }
    // Numbered prose is a draft extraction, never authoritative merely because it parses.
    if matches!(kind, "odt" | "html" | "htm") && !universe.trim().is_empty() {
        let pattern = regex::Regex::new(r"^\+?(\d+)(?:\.(\d+))?\.?\s+(.+?)(?:\s+\(\d{4}\))?$")?;
        for segment in &source.segments {
            if let Some(captures) = pattern.captures(&segment.text) {
                let title = captures[3].trim().to_string();
                let mut reference = Reference {
                    title: title.clone(),
                    source_id: id.clone(),
                    evidence: format!(
                        "Draft extraction at segment {}: {}",
                        segment.ordinal, segment.text
                    ),
                    origin: "numbered prose draft; verify work, scope and subseries".into(),
                    ..Default::default()
                };
                reference.hierarchy[0] = SeriesValue {
                    name: Some(universe.trim().into()),
                    index: Some(captures[1].parse()?),
                };
                if let Some(scan) = scan {
                    let matches: Vec<_> = scan
                        .books
                        .iter()
                        .filter(|book| normalize(&book.title) == normalize(&title))
                        .collect();
                    if matches.len() == 1 {
                        reference.authors = matches[0].authors.clone();
                        reference.book_uuids.push(matches[0].uuid.clone());
                        reference
                            .evidence
                            .push_str("; nested hierarchy was not inferred");
                    }
                }
                references.push(reference);
            }
        }
    }
    Ok((source, references))
}
pub fn import(
    path: &Path,
    universe: &str,
    scan: Option<&Scan>,
) -> Result<(Source, Vec<Reference>)> {
    let bytes = bounded(fs::File::open(path)?)?;
    let kind = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    parse(
        &bytes,
        &path.canonicalize()?.to_string_lossy(),
        &kind,
        universe,
        scan,
    )
}
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 240
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])))
        }
        IpAddr::V6(ip) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && !ip.is_multicast()
                && ip.to_ipv4_mapped().is_none()
        }
    }
}
pub fn fetch(url: &str, universe: &str, scan: Option<&Scan>) -> Result<(Source, Vec<Reference>)> {
    let url = reqwest::Url::parse(url)?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Use a public HTTPS URL without credentials on port 443");
    }
    let host = url.host_str().context("URL needs a hostname")?;
    let addresses: Vec<_> = (host, 443).to_socket_addrs()?.collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public(a.ip())) {
        bail!("Private, local, mapped and special network addresses are not allowed");
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .resolve_to_addrs(host, &addresses)
        .user_agent("Calibre-Curator/0.1 (explicit user source request)")
        .build()?;
    let response = client.get(url.clone()).send()?.error_for_status()?;
    if response.status().is_redirection() {
        bail!("Redirect blocked: explicitly request its public HTTPS destination");
    }
    let bytes = bounded(response)?;
    parse(&bytes, url.as_str(), "html", universe, scan)
}

/// Only successfully verified historical audits are eligible; newest per UUID wins.
pub fn reviewed_history(root: &Path, scan: &Scan) -> Result<Vec<(Source, Vec<Reference>)>> {
    let mut directories: Vec<_> = fs::read_dir(root)?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("curation-backup-")
        })
        .collect();
    directories.sort();
    let mut latest = BTreeMap::<String, (Source, Reference)>::new();
    for path in directories {
        if !path.join("audit.json").is_file() || !path.join("manifest.json").is_file() {
            continue;
        }
        let audit: Value =
            serde_json::from_slice(&bounded(fs::File::open(path.join("audit.json"))?)?)?;
        if audit["status"] != "verified"
            || audit["integrity"] != "ok"
            || audit["unrelated_metadata_unchanged"] != true
        {
            continue;
        }
        let bytes = bounded(fs::File::open(path.join("manifest.json"))?)?;
        let manifest: Value = serde_json::from_slice(&bytes)?;
        if Path::new(manifest["library"].as_str().unwrap_or(""))
            .canonicalize()
            .ok()
            != Some(Path::new(&scan.library).canonicalize()?)
        {
            continue;
        }
        let source = Source {
            id: new_id(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            location: path.join("manifest.json").to_string_lossy().into(),
            sha256: hash(&bytes),
            kind: "verified-history".into(),
            segments: vec![Segment {
                ordinal: 1,
                kind: "audit".into(),
                text: serde_json::to_string_pretty(&audit)?,
            }],
        };
        for entry in manifest["changes"]
            .as_array()
            .context("Invalid historical manifest")?
        {
            let uuid = entry["uuid"].as_str().context("Historical UUID missing")?;
            if !audit["updated_ids"]
                .as_array()
                .is_some_and(|ids| ids.contains(&entry["id"]))
            {
                continue;
            }
            let Some(book) = scan
                .books
                .iter()
                .find(|b| b.uuid == uuid && b.title == entry["title"])
            else {
                continue;
            };
            let reference = Reference {
                title: book.title.clone(),
                authors: book.authors.clone(),
                identifiers: book.identifiers.clone(),
                book_uuids: vec![book.uuid.clone()],
                hierarchy: hierarchy_from_json(&entry["after"])?,
                source_id: source.id.clone(),
                evidence: format!(
                    "{}; prior sources: {}",
                    entry["evidence"]
                        .as_str()
                        .unwrap_or("Historical reviewed mapping"),
                    entry["sources"]
                ),
                approved: true,
                origin: "previously applied and verified audit (latest per UUID)".into(),
                ..Default::default()
            };
            validate_reference(&reference)?;
            latest.insert(uuid.into(), (source.clone(), reference));
        }
    }
    let mut grouped: BTreeMap<String, (Source, Vec<Reference>)> = BTreeMap::new();
    for (_, (source, reference)) in latest {
        grouped
            .entry(source.id.clone())
            .or_insert_with(|| (source, Vec::new()))
            .1
            .push(reference);
    }
    Ok(grouped.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_internal_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "100.64.1.1",
            "::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!public(ip.parse().unwrap()));
        }
    }
    #[test]
    fn html_preserves_source_as_text() {
        let segments =
            html(b"<h2>World</h2><table><tr><td>1</td><td>A book</td></tr></table>").unwrap();
        assert_eq!(segments[0].text, "World");
        assert!(segments[1].text.contains("A book"));
    }
}
