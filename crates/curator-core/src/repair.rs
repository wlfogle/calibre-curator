use crate::{app::App, library, metadata, model::*, synopsis};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillRequest {
    pub lookup_id: String,
    pub book: Book,
    pub synopsis: bool,
    pub publisher: bool,
    pub identifiers: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepairRequest {
    pub series: Vec<Change>,
    pub fills: Vec<FillRequest>,
    #[serde(default)]
    pub research: Vec<String>,
}
pub fn fill_values(
    book: &Book,
    fetched: &metadata::FetchedMetadata,
    publisher: bool,
    identifiers: bool,
) -> BibliographicEdit {
    let before = BibliographicValues {
        publisher: book.publisher.clone(),
        identifiers: book.identifiers.clone(),
    };
    let mut after = before.clone();
    if publisher && after.publisher.as_ref().is_none_or(|v| v.trim().is_empty()) {
        if let Some(value) = &fetched.publisher {
            let value = value.trim();
            if !value.is_empty() && value.len() <= 500 {
                after.publisher = Some(value.into());
            }
        }
    }
    if identifiers {
        for (key, value) in metadata::safe_ids(&fetched.identifiers) {
            if after
                .identifiers
                .get(&key)
                .is_none_or(|v| v.trim().is_empty())
            {
                after.identifiers.insert(key, value);
            }
        }
    }
    BibliographicEdit { before, after }
}
impl App {
    pub fn prepare_missing(&mut self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() || ids.len() > 20 {
            bail!("Choose 1–20 books per online lookup batch");
        }
        let mut seen = BTreeSet::new();
        let mut notes = Vec::new();
        for id in ids {
            if !seen.insert(*id) {
                continue;
            }
            if let Err(e) = self.lookup_metadata(*id, false) {
                notes.push(format!("Book #{id}: {e}"));
            }
        }
        self.data.repair_notes = notes;
        self.store.save(&self.data)
    }
    pub fn apply_repairs(&mut self, mut request: RepairRequest) -> Result<()> {
        if request.series.is_empty() && request.fills.is_empty() && request.research.is_empty() {
            bail!("No repairs selected");
        }
        if request.fills.len() > 20 {
            bail!("Select at most 20 metadata fills per batch");
        }
        if self.history()?.iter().any(|o| {
            o.library == self.data.settings.library
                && !["verified", "rejected", "undone", "recovered"].contains(&o.phase.as_str())
        }) {
            bail!("Resolve unfinished operations in History first");
        }
        // Revalidate the entire metadata selection before any library write.
        self.refresh()?;
        let researched = self.accept_research(&request.research)?;
        let snapshot = self.snapshot()?;
        for p in snapshot
            .proposals
            .into_iter()
            .filter(|p| researched.contains(&p.book.id))
        {
            if request.series.iter().any(|c| c.id == p.book.id) {
                bail!("A book cannot have separate old and researched series selections");
            }
            let target = p.after.context("No supported research target")?;
            let after = std::array::from_fn(|i| {
                if p.pinned[i] {
                    p.book.hierarchy[i].clone()
                } else {
                    target[i].clone()
                }
            });
            if after == p.book.hierarchy {
                continue;
            }
            request.series.push(Change {
                id: p.book.id,
                uuid: p.book.uuid,
                title: p.book.title,
                before: p.book.hierarchy,
                after,
                evidence: String::new(),
                sources: Vec::new(),
                comments: None,
                bibliography: None,
                authors: p.book.authors,
                identifiers: p.book.identifiers,
            });
        }
        let series = if request.series.is_empty() {
            Vec::new()
        } else {
            self.series_changes(request.series)?
        };
        let mut seen = BTreeSet::new();
        let mut bibliography = Vec::new();
        let mut drafts = Vec::new();
        for fill in &request.fills {
            let result = self
                .data
                .metadata_results
                .iter()
                .find(|r| r.id == fill.lookup_id)
                .context("Lookup disappeared")?
                .clone();
            let current = self
                .scan
                .as_ref()
                .context("Scan first")?
                .books
                .iter()
                .find(|b| b.id == fill.book.id)
                .context("Book disappeared")?
                .clone();
            if !seen.insert(current.id)
                || current.uuid != fill.book.uuid
                || current.publisher != fill.book.publisher
                || current.identifiers != fill.book.identifiers
                || current.has_comments != fill.book.has_comments
                || result.book_id != current.id
                || result.book_uuid != current.uuid
                || result.library != self.data.settings.library
                || metadata::book_fingerprint(&current)? != result.book_fingerprint
            {
                bail!("Metadata preview is stale or duplicated; scan and review again");
            }
            self.confirm_metadata(&result.id, true)?;
            self.synopsis_evidence(&result.id)?;
            let fetched = result.metadata.as_ref().context("No provider metadata")?;
            if metadata::identity(&current, fetched).0 != "compatible" {
                bail!("Metadata identity or edition mismatch");
            }
            let edit = fill_values(&current, fetched, fill.publisher, fill.identifiers);
            if edit.before != edit.after {
                let source = self
                    .data
                    .sources
                    .iter()
                    .find(|s| Some(&s.id) == result.source_id.as_ref())
                    .context("Source evidence missing")?;
                bibliography.push(Change {
                    id: current.id,
                    uuid: current.uuid.clone(),
                    title: current.title.clone(),
                    before: current.hierarchy.clone(),
                    after: current.hierarchy.clone(),
                    evidence: format!(
                        "Batch-reviewed work/edition; provider {} {}",
                        result.provider.name, result.provider.version
                    ),
                    sources: vec![format!(
                        "{}; sha256={}; lookup={}",
                        source.location, source.sha256, result.id
                    )],
                    comments: None,
                    bibliography: Some(edit),
                    authors: current.authors.clone(),
                    identifiers: current.identifiers.clone(),
                });
            }
            if fill.synopsis {
                self.confirm_metadata(&result.id, true)?;
                self.generate_synopsis(&result.id, false, false)?;
                let draft = self
                    .data
                    .synopsis_drafts
                    .first()
                    .context("No synopsis generated")?
                    .clone();
                self.save_synopsis(&draft.id, &draft.text, true)?;
                drafts.push(draft.id);
            }
        }
        let mut combined: std::collections::BTreeMap<i64, Change> =
            series.into_iter().map(|c| (c.id, c)).collect();
        for change in bibliography {
            if let Some(existing) = combined.get_mut(&change.id) {
                existing.bibliography = change.bibliography;
                existing
                    .evidence
                    .push_str(&format!("; {}", change.evidence));
                existing.sources.extend(change.sources);
            } else {
                combined.insert(change.id, change);
            }
        }
        for id in &drafts {
            let draft = self
                .data
                .synopsis_drafts
                .iter()
                .find(|d| &d.id == id)
                .context("Synopsis disappeared")?;
            let (_, book, source) = self.synopsis_evidence(&draft.lookup_id)?;
            let before = library::comments(&self.data.settings, book.id)?;
            if before != draft.before_comments
                || before.as_ref().is_some_and(|v| !v.trim().is_empty())
            {
                bail!("Comments changed since preview");
            }
            let entry = combined.entry(book.id).or_insert_with(|| Change {
                id: book.id,
                uuid: book.uuid,
                title: book.title,
                before: book.hierarchy.clone(),
                after: book.hierarchy,
                authors: book.authors,
                identifiers: book.identifiers,
                evidence: String::new(),
                sources: Vec::new(),
                comments: None,
                bibliography: None,
            });
            entry.comments = Some(CommentEdit {
                before,
                after: Some(synopsis::as_html(&draft.text)),
            });
            entry
                .evidence
                .push_str("; Batch-reviewed provider synopsis");
            entry
                .sources
                .push(format!("{}; sha256={}", source.location, source.sha256));
        }
        if combined.is_empty() {
            bail!("Selected fields need no changes");
        }
        let mut operation = self.operation(combined.into_values().collect(), "apply", None);
        operation.metadata_field = "repair".into();
        let result = self.run(operation)?;
        for draft in &mut self.data.synopsis_drafts {
            if drafts.contains(&draft.id) {
                draft.state = "applied".into();
                draft.approved = false;
                draft.operation_id = Some(result.id.clone());
            }
        }
        self.store.save(&self.data)?;
        Ok(())
    }
}
