//! Explicit source-level approval, distinct from unverified research imports.
use crate::{app::App, matching, model::*, sources};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceReview {
    pub fingerprint: String,
    pub universe: String,
    pub authors: Vec<String>,
    pub aliases: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentEntry {
    pub reference: Reference,
    pub status: String,
    pub matches: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentPreview {
    pub entries: Vec<DocumentEntry>,
    pub skipped: Vec<usize>,
}
pub fn fingerprint(source: &Source) -> Result<String> {
    Ok(sources::hash(&serde_json::to_vec(&(
        &source.sha256,
        &source.segments,
    ))?))
}
pub fn authorized(data: &AppData, reference: &Reference) -> bool {
    let Some(expected) = &reference.document_verification else {
        return true;
    };
    data.source_reviews
        .get(&reference.source_id)
        .is_some_and(|review| {
            &review.fingerprint == expected
                && data
                    .sources
                    .iter()
                    .find(|s| s.id == reference.source_id)
                    .and_then(|s| fingerprint(s).ok())
                    .as_ref()
                    == Some(expected)
        })
}
pub fn preview(source: &Source, review: &SourceReview, scan: &Scan) -> Result<DocumentPreview> {
    if review.universe.trim().is_empty() || review.authors.iter().all(|a| a.trim().is_empty()) {
        bail!("Supply the overarching series and source author context");
    }
    if matches!(source.kind.as_str(), "json" | "csv") {
        let raw = &source
            .segments
            .first()
            .context("Structured source is empty")?
            .text;
        let (_, references) =
            sources::parse(raw.as_bytes(), &source.location, &source.kind, "", None)?;
        let mut entries = Vec::new();
        for (i, mut r) in references.into_iter().enumerate() {
            r.source_id = source.id.clone();
            r.book_uuids.clear();
            r.document_verification = Some(fingerprint(source)?);
            if r.authors.is_empty() {
                r.authors = review.authors.clone();
            }
            r.hierarchy[0] = SeriesValue {
                name: Some(review.universe.trim().into()),
                index: Some((i + 1) as f64),
            };
            let mut mask = [true, false, false, false];
            for (slot, value) in r.hierarchy.iter_mut().enumerate().skip(1) {
                if let Some(name) = &value.name {
                    if let Some(alias) = review.aliases.get(name) {
                        value.name = Some(alias.clone());
                    }
                    mask[slot] = value.index.is_some();
                }
            }
            r.field_mask = Some(mask);
            r.evidence = format!("Verified structured source row {}; {}", i + 1, r.evidence);
            r.origin = "verified source document".into();
            entries.push(entry(r, scan));
        }
        return Ok(finish(entries, Vec::new()));
    }
    // Deliberately require recognizable book-entry syntax, not arbitrary paragraphs.
    let dated = regex::Regex::new(r"^(.+?)\s+\((?:18|19|20)\d{2}\)\s*[–—-]\s*(.+)$")?;
    let numbered =
        regex::Regex::new(r"^\+?\d+(?:\.\d+)?\.?\s+(.+?)(?:\s+\((?:18|19|20)\d{2}\))?$")?;
    let branch = regex::Regex::new(r"^(.+?)\s+#(\d+(?:\.\d+)?)$")?;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for segment in &source.segments {
        if matches!(
            segment.kind.as_str(),
            "heading" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        ) {
            continue;
        }
        let (title, label) = if let Some(c) = dated.captures(segment.text.trim()) {
            (c[1].trim().to_string(), c[2].trim().to_string())
        } else if let Some(c) = numbered.captures(segment.text.trim()) {
            (c[1].trim().to_string(), String::new())
        } else {
            skipped.push(segment.ordinal);
            continue;
        };
        let mut r = Reference {
            title,
            authors: review
                .authors
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            source_id: source.id.clone(),
            evidence: format!(
                "Verified document order; segment {}: {}",
                segment.ordinal, segment.text
            ),
            origin: "verified source document".into(),
            field_mask: Some([true, false, false, false]),
            document_verification: Some(fingerprint(source)?),
            ..Default::default()
        };
        r.hierarchy[0] = SeriesValue {
            name: Some(review.universe.trim().into()),
            index: Some((entries.len() + 1) as f64),
        };
        if let Some(c) = branch.captures(&label) {
            let name = c[1].trim();
            let name = review
                .aliases
                .get(name)
                .map(String::as_str)
                .unwrap_or(name)
                .trim();
            if name.is_empty() {
                bail!("Subseries aliases cannot be blank");
            }
            r.hierarchy[1] = SeriesValue {
                name: Some(name.into()),
                index: Some(c[2].parse()?),
            };
            r.field_mask = Some([true, true, false, false]);
        }
        entries.push(entry(r, scan));
    }
    Ok(finish(entries, skipped))
}
fn entry(r: Reference, scan: &Scan) -> DocumentEntry {
    let matches = scan
        .books
        .iter()
        .filter(|b| {
            matching::normalize(&b.title) == matching::normalize(&r.title)
                && r.authors.iter().any(|a| {
                    b.authors
                        .iter()
                        .any(|actual| matching::normalize(a) == matching::normalize(actual))
                })
        })
        .count();
    DocumentEntry {
        reference: r,
        matches,
        status: match matches {
            0 => "not owned",
            1 => "matched",
            _ => "ambiguous editions",
        }
        .into(),
    }
}
fn finish(mut entries: Vec<DocumentEntry>, skipped: Vec<usize>) -> DocumentPreview {
    let mut seen = BTreeSet::new();
    let duplicates: BTreeSet<_> = entries
        .iter()
        .filter_map(|e| {
            let title = matching::normalize(&e.reference.title);
            (!seen.insert(title.clone())).then_some(title)
        })
        .collect();
    for e in &mut entries {
        if duplicates.contains(&matching::normalize(&e.reference.title)) {
            e.status = "duplicate source entry".into();
        }
    }
    DocumentPreview { entries, skipped }
}
impl App {
    pub fn preview_document(&self, id: &str, review: &SourceReview) -> Result<DocumentPreview> {
        let source = self
            .data
            .sources
            .iter()
            .find(|s| s.id == id)
            .context("Unknown source")?;
        preview(source, review, self.scan.as_ref().context("Scan first")?)
    }
    pub fn verify_document(&mut self, id: &str, mut review: SourceReview) -> Result<()> {
        let source = self
            .data
            .sources
            .iter()
            .find(|s| s.id == id)
            .context("Unknown source")?;
        let report = self.preview_document(id, &review)?;
        if report.entries.is_empty() {
            bail!("No supported ordered book entries; inspect source formatting");
        }
        review.fingerprint = fingerprint(source)?;
        let mut next = self.data.clone();
        next.source_reviews.insert(id.into(), review);
        // Replace generated/import drafts, not independently reviewed or AI rules.
        // Existing approved decisions must remain visible to conflict detection.
        next.references.retain(|r| {
            r.source_id != id
                || (r.document_verification.is_none() && (r.approved || r.ai_provenance.is_some()))
        });
        for mut entry in report.entries {
            entry.reference.approved = matches!(entry.status.as_str(), "matched" | "not owned");
            matching::validate_reference(&entry.reference)?;
            next.references.push(entry.reference);
        }
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    pub fn revoke_document(&mut self, id: &str) -> Result<()> {
        let mut next = self.data.clone();
        next.source_reviews.remove(id);
        for r in &mut next.references {
            if r.source_id == id && r.document_verification.is_some() {
                r.approved = false;
            }
        }
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
}
