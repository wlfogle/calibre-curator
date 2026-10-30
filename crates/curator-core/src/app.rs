use crate::{
    library, matching,
    model::*,
    sources,
    store::{atomic_json, Store},
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Serialize)]
pub struct Snapshot {
    pub workspace: String,
    pub data: AppData,
    pub proposals: Vec<Proposal>,
    pub history: Vec<Operation>,
    pub issues: BTreeMap<String, String>,
}
pub type ProgressCallback = Box<dyn Fn(&str) + Send + Sync>;
pub struct App {
    pub store: Store,
    pub data: AppData,
    pub scan: Option<Scan>,
    pub bridge: PathBuf,
    pub progress: Option<ProgressCallback>,
}
impl App {
    pub fn open(root: &Path, bridge: PathBuf) -> Result<Self> {
        let store = Store::open(root)?;
        let data = store.load()?;
        Ok(Self {
            store,
            data,
            scan: None,
            bridge,
            progress: None,
        })
    }
    pub fn refresh(&mut self) -> Result<()> {
        let scan = library::scan(&self.data.settings)?;
        self.data.settings.library = scan.library.clone();
        for book in &scan.books {
            if matching::model_universe(book) {
                self.data
                    .pins
                    .entry(matching::pin_key(&scan.library, &book.uuid))
                    .or_insert([true; 4]);
            }
        }
        self.store.save(&self.data)?;
        self.scan = Some(scan);
        Ok(())
    }
    pub fn history(&self) -> Result<Vec<Operation>> {
        let mut operations = Vec::new();
        for entry in fs::read_dir(self.store.root.join("operations"))? {
            let path = entry?.path().join("operation.json");
            if path.is_file() {
                operations.push(serde_json::from_slice::<Operation>(&fs::read(path)?)?);
            }
        }
        // A verified reversal is authoritative even if the app stopped before
        // updating its parent's label. Derive that label from durable child journals.
        let resolved: BTreeMap<String, String> = operations
            .iter()
            .filter(|op| {
                ["verified", "undone", "recovered"].contains(&op.phase.as_str())
                    && ["undo", "recover"].contains(&op.kind.as_str())
            })
            .filter_map(|op| {
                op.parent.as_ref().map(|id| {
                    (
                        id.clone(),
                        if op.kind == "undo" {
                            "undone"
                        } else {
                            "recovered"
                        }
                        .into(),
                    )
                })
            })
            .collect();
        for op in &mut operations {
            if let Some(phase) = resolved.get(&op.id) {
                op.phase = phase.clone();
            }
        }
        operations.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        Ok(operations)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let issues = matching::issues(&self.data.references);
        let proposals = self
            .scan
            .as_ref()
            .map(|scan| {
                scan.books
                    .iter()
                    .map(|book| matching::propose(book, &self.data, &scan.library, &issues))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Snapshot {
            workspace: self.store.root.to_string_lossy().into(),
            data: self.data.clone(),
            proposals,
            history: self.history()?,
            issues,
        })
    }
    pub fn settings(&mut self, settings: Settings) -> Result<()> {
        if self
            .store
            .root
            .starts_with(Path::new(&settings.library).canonicalize()?)
        {
            bail!("The application workspace must be outside the selected library");
        }
        library::scan(&settings)?; // Do not save an unusable mapping.
        self.data.settings = settings;
        self.refresh()
    }
    fn add_source(&mut self, source: Source, mut references: Vec<Reference>) -> Result<usize> {
        let map: BTreeMap<_, _> = references
            .iter()
            .map(|r| (r.id.clone(), new_id()))
            .collect();
        for reference in &mut references {
            reference.id = map[&reference.id].clone();
            reference.before = reference
                .before
                .iter()
                .map(|id| map.get(id).unwrap_or(id).clone())
                .collect();
        }
        let count = references.len();
        let mut next = self.data.clone();
        next.sources.push(source);
        next.references.extend(references);
        self.store.save(&next)?;
        self.data = next;
        Ok(count)
    }
    pub fn import(&mut self, location: &str, universe: &str, remote: bool) -> Result<usize> {
        let (source, references) = if remote {
            sources::fetch(location, universe, self.scan.as_ref())?
        } else {
            sources::import(Path::new(location), universe, self.scan.as_ref())?
        };
        self.add_source(source, references)
    }
    pub fn seed(&mut self, root: &Path) -> Result<usize> {
        let imported =
            sources::reviewed_history(root, self.scan.as_ref().context("Scan the library first")?)?;
        let mut next = self.data.clone();
        let mut count = 0;
        for (source, references) in imported {
            if next
                .sources
                .iter()
                .any(|s| s.sha256 == source.sha256 && s.kind == source.kind)
            {
                continue;
            }
            let uuids: BTreeSet<_> = references
                .iter()
                .flat_map(|r| r.book_uuids.iter().cloned())
                .collect();
            next.references.retain(|r| {
                !(r.origin.starts_with("previously applied")
                    && r.book_uuids.iter().any(|uuid| uuids.contains(uuid)))
            });
            count += references.len();
            next.sources.push(source);
            next.references.extend(references);
        }
        self.store.save(&next)?;
        self.data = next;
        Ok(count)
    }
    pub fn save_reference(&mut self, mut reference: Reference) -> Result<()> {
        if reference.id.is_empty() {
            reference.id = new_id();
        }
        if reference.hierarchy[0].name.is_none() {
            reference.hierarchy[0].index = Some(1.0);
        }
        if let Some(provenance) = &reference.ai_provenance {
            let source=self.data.sources.iter().find(|s|s.id==reference.source_id)
                .context("AI evidence must retain its original source; create a separate manual reference to replace provenance")?;
            crate::ai::validate_provenance(provenance, source)?;
        }
        matching::validate_reference(&reference)?;
        if reference.approved {
            reference.origin = "user-reviewed reference".into();
        }
        if reference.source_id.is_empty() {
            let text = serde_json::to_string_pretty(&reference)?;
            let source = Source {
                id: new_id(),
                name: format!("User decision: {}", reference.title),
                location: "local user decision".into(),
                sha256: sources::hash(text.as_bytes()),
                kind: "user-decision".into(),
                segments: vec![Segment {
                    ordinal: 1,
                    kind: "decision".into(),
                    text,
                }],
            };
            reference.source_id = source.id.clone();
            self.data.sources.push(source);
        }
        if !self
            .data
            .sources
            .iter()
            .any(|s| s.id == reference.source_id)
        {
            bail!("Unknown source");
        }
        self.data.references.retain(|r| r.id != reference.id);
        self.data.references.push(reference);
        self.store.save(&self.data)
    }
    pub fn delete_reference(&mut self, id: &str) -> Result<()> {
        self.data.references.retain(|r| r.id != id);
        self.store.save(&self.data)
    }
    pub fn pin(&mut self, uuid: &str, mask: [bool; 4]) -> Result<()> {
        let scan = self.scan.as_ref().context("Scan first")?;
        if !scan.books.iter().any(|b| b.uuid == uuid) {
            bail!("Unknown book");
        }
        self.data
            .pins
            .insert(matching::pin_key(&scan.library, uuid), mask);
        self.store.save(&self.data)
    }
    pub fn aliases(&mut self, aliases: Vec<UniverseAlias>) -> Result<()> {
        if aliases
            .iter()
            .any(|a| a.existing_series.trim().is_empty() || a.universe.trim().is_empty())
        {
            bail!("Aliases need both an existing series and a candidate universe");
        }
        self.data.aliases = aliases;
        self.store.save(&self.data)
    }
    pub fn sequence(&mut self, universe: &str) -> Result<usize> {
        let ordered = matching::chronology(&self.data.references, universe)?;
        let lookup: BTreeMap<_, _> = self
            .data
            .references
            .iter()
            .map(|r| (r.id.clone(), r.clone()))
            .collect();
        let anchors: Vec<_> = ordered
            .iter()
            .enumerate()
            .filter_map(|(i, ids)| {
                ids.iter()
                    .filter_map(|id| {
                        lookup[id]
                            .fixed
                            .then_some(lookup[id].hierarchy[0].index)
                            .flatten()
                    })
                    .next()
                    .map(|value| (i, value))
            })
            .collect();
        let mut assignments = BTreeMap::new();
        for (i, ids) in ordered.iter().enumerate() {
            if ids
                .iter()
                .any(|id| lookup[id].fixed && lookup[id].hierarchy[0].index.is_some())
            {
                continue;
            }
            let left = anchors.iter().rev().find(|(position, _)| *position < i);
            let right = anchors.iter().find(|(position, _)| *position > i);
            let number = match (left, right) {
                (Some((a, x)), Some((b, y))) => x + (y - x) * (i - a) as f64 / (b - a) as f64,
                (Some((a, x)), None) => x + (i - a) as f64,
                (None, Some((b, y))) => y - (b - i) as f64,
                (None, None) => (i + 1) as f64,
            };
            for id in ids {
                assignments.insert(id.clone(), number);
            }
        }
        let mut changed = 0;
        for r in &mut self.data.references {
            if let Some(number) = assignments.get(&r.id) {
                if r.hierarchy[0].index != Some(*number) {
                    r.hierarchy[0].index = Some(*number);
                    r.approved = false;
                    r.origin =
                        "user-requested constraint ordering; review calculated position".into();
                    changed += 1;
                }
            }
        }
        self.store.save(&self.data)?;
        Ok(changed)
    }
    fn protections(&self) -> Vec<Protection> {
        self.scan
            .as_ref()
            .map(|scan| {
                scan.books
                    .iter()
                    .filter_map(|book| {
                        self.data
                            .pins
                            .get(&matching::pin_key(&scan.library, &book.uuid))
                            .and_then(|mask| {
                                mask.iter().any(|v| *v).then(|| Protection {
                                    uuid: book.uuid.clone(),
                                    mask: *mask,
                                    values: book.hierarchy.clone(),
                                })
                            })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn run(&mut self, mut operation: Operation) -> Result<Operation> {
        let binary = Path::new(&self.data.settings.calibre_binary);
        if !binary.is_absolute()
            || binary.file_name().and_then(|s| s.to_str()) != Some("calibre-debug")
            || !binary.is_file()
        {
            bail!("Select the absolute installed calibre-debug executable");
        }
        let folder = self.store.root.join("operations").join(&operation.id);
        fs::create_dir(&folder)?;
        let journal = folder.join("operation.json");
        atomic_json(&journal, &operation)?;
        let mut log = fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(folder.join("bridge.log"))?;
        let spawn = Command::new(binary)
            .args(["-e"])
            .arg(&self.bridge)
            .arg("--")
            .arg(&journal)
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log.try_clone()?))
            .spawn();
        let mut child = match spawn {
            Ok(child) => child,
            Err(error) => {
                operation.phase = "rejected".into();
                operation.error = Some(format!("Could not start Calibre: {error}"));
                atomic_json(&journal, &operation)?;
                return Err(error.into());
            }
        };
        let mut reader = BufReader::new(child.stdout.take().context("Bridge stdout unavailable")?);
        let mut bytes = Vec::new();
        while reader.read_until(b'\n', &mut bytes)? != 0 {
            log.write_all(&bytes)?;
            if let Some(update) = String::from_utf8_lossy(&bytes)
                .trim()
                .strip_prefix("CURATOR_PROGRESS:")
            {
                if let Some(callback) = &self.progress {
                    callback(update);
                }
            }
            bytes.clear();
        }
        let status = child.wait()?;
        log.sync_all()?;
        operation = serde_json::from_slice(&fs::read(&journal)?)?;
        if !status.success() || operation.phase != "verified" {
            let error = operation.error.clone().unwrap_or_else(|| {
                format!(
                    "Bridge exited {}; inspect {}",
                    status,
                    folder.join("bridge.log").display()
                )
            });
            if operation.error.is_none() {
                operation.error = Some(error.clone());
                atomic_json(&journal, &operation)?;
            }
            bail!("{error}");
        }
        self.refresh()?;
        Ok(operation)
    }
    fn operation(&self, changes: Vec<Change>, kind: &str, parent: Option<String>) -> Operation {
        Operation {
            id: new_id(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            library: self.data.settings.library.clone(),
            columns: self.data.settings.columns.clone(),
            phase: "prepared".into(),
            kind: kind.into(),
            parent,
            backup: None,
            error: None,
            changes,
            protections: self.protections(),
            verification: None,
        }
    }
    pub fn apply(&mut self, requests: Vec<Change>) -> Result<Operation> {
        if self.history()?.iter().any(|op| {
            op.library == self.data.settings.library
                && !["verified", "rejected", "undone", "recovered"].contains(&op.phase.as_str())
        }) {
            bail!("Resolve the unfinished operation in History before applying another batch");
        }
        let scan = self
            .scan
            .as_ref()
            .context("Scan before reviewing changes")?;
        let issues = matching::issues(&self.data.references);
        let mut changes = Vec::new();
        let mut ids = BTreeSet::new();
        for request in requests {
            if !ids.insert(request.id) {
                bail!("Duplicate selection");
            }
            let book = scan
                .books
                .iter()
                .find(|b| b.id == request.id)
                .context("Selected book disappeared")?;
            if book.uuid != request.uuid || book.hierarchy != request.before {
                bail!("Review is stale: rescan");
            }
            let proposal = matching::propose(book, &self.data, &scan.library, &issues);
            if proposal.blocked {
                bail!("Selected book is blocked: {}", proposal.reason);
            }
            let target = proposal.after.context("No approved target")?;
            for (slot, approved_value) in target.iter().enumerate() {
                if request.after[slot] != book.hierarchy[slot] && proposal.pinned[slot] {
                    bail!("A selected field is protected");
                }
                if request.after[slot] != book.hierarchy[slot]
                    && request.after[slot] != *approved_value
                {
                    bail!("A selected field differs from its approved reference");
                }
            }
            if request.after == book.hierarchy {
                continue;
            }
            let reference = self
                .data
                .references
                .iter()
                .find(|r| Some(&r.id) == proposal.reference_id.as_ref())
                .context("Reference disappeared")?;
            let source = self
                .data
                .sources
                .iter()
                .find(|s| s.id == reference.source_id)
                .context("Source missing")?;
            changes.push(Change {
                id: book.id,
                uuid: book.uuid.clone(),
                title: book.title.clone(),
                before: book.hierarchy.clone(),
                after: request.after,
                evidence: format!(
                    "{}{}",
                    proposal.reason,
                    reference
                        .ai_provenance
                        .as_ref()
                        .map(|p| format!(
                            " [AI provenance: model {}; digest {}; prompt {}]",
                            p.model, p.digest, p.prompt_version
                        ))
                        .unwrap_or_default()
                ),
                sources: vec![format!(
                    "{}; sha256={}; reference={}",
                    source.location, source.sha256, reference.id
                )],
                authors: book.authors.clone(),
                identifiers: book.identifiers.clone(),
            });
        }
        if changes.is_empty() {
            bail!("No changed fields selected");
        }
        let operation = self.operation(changes, "apply", None);
        self.run(operation)
    }
    pub fn reverse(&mut self, id: &str) -> Result<Operation> {
        let id = uuid::Uuid::parse_str(id)?.to_string();
        let previous = self
            .history()?
            .into_iter()
            .find(|op| op.id == id)
            .context("Unknown operation")?;
        if ["rejected", "undone", "recovered"].contains(&previous.phase.as_str()) {
            bail!("This operation is already closed; inspect its successor journal");
        }
        if previous.library != self.data.settings.library
            || previous.columns != self.data.settings.columns
        {
            bail!("Select the original library and column mapping first");
        }
        self.refresh()?;
        let scan = self.scan.as_ref().context("Scan unavailable")?;
        let mut changes = Vec::new();
        for entry in &previous.changes {
            let book = scan
                .books
                .iter()
                .find(|b| b.id == entry.id && b.uuid == entry.uuid)
                .context("An original target was removed or replaced")?;
            changes.push(Change {
                id: book.id,
                uuid: book.uuid.clone(),
                title: book.title.clone(),
                before: book.hierarchy.clone(),
                after: entry.before.clone(),
                evidence: format!("Guarded reversal of {}", previous.id),
                sources: vec![id.clone()],
                authors: book.authors.clone(),
                identifiers: book.identifiers.clone(),
            });
        }
        let kind = if previous.phase == "verified" {
            "undo"
        } else {
            "recover"
        };
        let result = self.run(self.operation(changes, kind, Some(id.clone())))?;
        let mut previous = previous;
        previous.phase = if kind == "undo" {
            "undone"
        } else {
            "recovered"
        }
        .into();
        atomic_json(
            &self
                .store
                .root
                .join("operations")
                .join(id)
                .join("operation.json"),
            &previous,
        )?;
        Ok(result)
    }
    pub fn dismiss(&mut self, id: &str) -> Result<()> {
        let id = uuid::Uuid::parse_str(id)?.to_string();
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.store.root.join("writer.lock"))?;
        lock.try_lock()
            .context("A bridge is still running; do not dismiss its journal")?;
        let folder = self.store.root.join("operations").join(id);
        let mut operation: Operation =
            serde_json::from_slice(&fs::read(folder.join("operation.json"))?)?;
        if operation.backup.is_some()
            || folder.join("guard.json").exists()
            || !["prepared", "locked", "rejected"].contains(&operation.phase.as_str())
        {
            bail!("This operation may have written data: guarded recovery or manual review is required");
        }
        operation.phase = "rejected".into();
        operation.error = Some("Dismissed before any curated-field write or backup phase".into());
        atomic_json(&folder.join("operation.json"), &operation)
    }
    pub fn export(&self, path: &Path) -> Result<()> {
        if path.exists() {
            bail!("Choose a new export filename");
        }
        if path.ancestors().any(|p| p.join("metadata.db").is_file()) {
            bail!("Export outside the library");
        }
        atomic_json(
            path,
            &serde_json::json!({"references":self.data.references,"aliases":self.data.aliases}),
        )
    }
}
