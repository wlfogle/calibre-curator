use crate::{ai::*, app::App, matching, model::*, sources};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::collections::BTreeSet;

impl App {
    pub fn configure_ai(&mut self, settings: AiSettings) -> Result<()> {
        if settings.model.trim().is_empty() || settings.model.len() > 200 {
            bail!("Choose a model name");
        }
        Ollama::new(&settings)?;
        let mut next = self.data.clone();
        next.ai_settings = settings;
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    pub fn discover_ai_models(&mut self) -> Result<()> {
        let models = Ollama::new(&self.data.ai_settings)?.models()?;
        let mut next = self.data.clone();
        next.ai_models = models;
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    fn ai_progress(&self, phase: &str) {
        if let Some(callback) = &self.progress {
            callback(&json!({"phase":phase,"completed":0,"total":1}).to_string());
        }
    }
    pub fn extract_ai(
        &mut self,
        source_id: &str,
        start: usize,
        end: usize,
        universe: &str,
        refresh: bool,
    ) -> Result<()> {
        let settings = self.data.ai_settings.clone();
        if !settings.enabled {
            bail!("Enable optional Ollama assistance before generating suggestions");
        }
        let universe = universe.trim();
        if universe.len() > 200 {
            bail!("Universe context is too long");
        }
        let source = self
            .data
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .context("Choose a cached source")?
            .clone();
        let segments = selected(&source, start, end)?;
        self.ai_progress("Ollama: checking local models");
        let client = Ollama::new(&settings)?;
        let models = client.models()?;
        let model = models
            .iter()
            .find(|m| m.name == settings.model)
            .context("Selected model is not installed; refresh the model list")?;
        if !model.usable {
            bail!("Selected model is blocked: {}", model.reason);
        }
        let key = cache_key(&source, &segments, universe, model, &settings.endpoint)?;
        let segments_hash = sources::hash(&serde_json::to_vec(&segments)?);
        let cached = if refresh {
            None
        } else {
            self.store.cached(&key)?
        };
        let mut run = if let Some(mut run) = cached {
            if run.cache_key != key
                || run.source_hash != source.sha256
                || run.segments_hash != segments_hash
                || run.digest != model.digest
                || run.prompt_version != VERSION
                || run.universe != universe
            {
                bail!("Cached response provenance is invalid; clear the AI cache");
            }
            validate(&run.extraction, &segments, universe)?;
            run.cached = true;
            run
        } else {
            self.ai_progress("Ollama: extracting cited draft suggestions");
            let response = client.generate(model, &segments, universe)?;
            if response["done"] != true
                || response["done_reason"]
                    .as_str()
                    .is_some_and(|r| r != "stop")
                || response["message"].get("tool_calls").is_some_and(|calls| {
                    !calls.is_null() && calls.as_array().is_none_or(|a| !a.is_empty())
                })
            {
                bail!("Ollama returned an incomplete response or attempted tool calls; nothing was imported");
            }
            let content = response["message"]["content"]
                .as_str()
                .context("No structured model response")?;
            let mut extraction: Extraction = serde_json::from_str(content).context(
                "Model output did not match the strict extraction schema; nothing was imported",
            )?;
            let identity_context_expansions =
                complete_identity_context(&mut extraction, &segments)?;
            validate(&extraction, &segments, universe)?;
            let run = AiRun {
                id: new_id(),
                cache_key: key.clone(),
                source_id: source.id.clone(),
                source_hash: source.sha256.clone(),
                segments_hash,
                segment_ordinals: segments.iter().map(|s| s.ordinal).collect(),
                universe: universe.into(),
                model: model.name.clone(),
                digest: model.digest.clone(),
                prompt_version: VERSION.into(),
                generated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                input_bytes: segments.iter().map(|s| s.text.len()).sum(),
                prompt_tokens: response["prompt_eval_count"].as_u64().unwrap_or(0),
                output_tokens: response["eval_count"].as_u64().unwrap_or(0),
                duration_ms: response["total_duration"].as_u64().unwrap_or(0) / 1_000_000,
                cached: false,
                identity_context_expansions,
                fingerprints: Vec::new(),
                extraction,
            };
            self.store.cache(&run)?;
            run
        };
        run.id = new_id();
        run.source_id = source.id;
        run.fingerprints = run
            .extraction
            .candidates
            .iter()
            .map(|c| fingerprint(&run.source_hash, c))
            .collect::<Result<_>>()?;
        let mut next = self.data.clone();
        next.ai_models = models;
        next.ai_runs.retain(|r| r.cache_key != key);
        next.ai_runs.insert(0, run);
        next.ai_runs.truncate(40);
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    fn ai_candidate(
        &self,
        run_id: &str,
        index: usize,
    ) -> Result<(AiRun, Source, Candidate, String)> {
        let run = self
            .data
            .ai_runs
            .iter()
            .find(|r| r.id == run_id)
            .context("AI run is no longer in the inbox")?
            .clone();
        let source = self
            .data
            .sources
            .iter()
            .find(|s| s.id == run.source_id)
            .context("Original source is missing")?
            .clone();
        if source.sha256 != run.source_hash {
            bail!("Source revision changed; extract/review again");
        }
        let segments: Vec<_> = source
            .segments
            .iter()
            .filter(|s| run.segment_ordinals.contains(&s.ordinal))
            .cloned()
            .collect();
        if sources::hash(&serde_json::to_vec(&segments)?) != run.segments_hash {
            bail!("Selected source text changed");
        }
        validate(&run.extraction, &segments, &run.universe)?;
        let candidate = run
            .extraction
            .candidates
            .get(index)
            .context("Unknown suggestion")?
            .clone();
        let key = fingerprint(&source.sha256, &candidate)?;
        Ok((run, source, candidate, key))
    }
    pub fn decide_ai(&mut self, run_id: &str, index: usize, status: &str) -> Result<()> {
        let (_, _, _, key) = self.ai_candidate(run_id, index)?;
        if !matches!(status, "pending" | "dismissed") {
            bail!("Unknown AI review decision");
        }
        if self
            .data
            .ai_decisions
            .get(&key)
            .and_then(|d| d.reference_id.as_ref())
            .is_some_and(|id| self.data.references.iter().any(|r| &r.id == id))
        {
            bail!(
                "A reference already exists for this suggestion; edit or delete it in References"
            );
        }
        let mut next = self.data.clone();
        next.ai_decisions.insert(
            key,
            AiDecision {
                status: status.into(),
                reference_id: None,
            },
        );
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    pub fn materialize_ai(
        &mut self,
        run_id: &str,
        index: usize,
        book_id: Option<i64>,
    ) -> Result<()> {
        let (run, source, candidate, key) = self.ai_candidate(run_id, index)?;
        if let Some(decision) = self.data.ai_decisions.get(&key) {
            if decision.status == "dismissed" {
                bail!("Restore the dismissed suggestion first");
            }
            if decision
                .reference_id
                .as_ref()
                .is_some_and(|id| self.data.references.iter().any(|r| &r.id == id))
            {
                bail!("This suggestion already has a draft; open the existing reference");
            }
        }
        let book = if let Some(id) = book_id {
            Some(
                self.scan
                    .as_ref()
                    .context("Scan the library before explicitly binding a record")?
                    .books
                    .iter()
                    .find(|b| b.id == id)
                    .context("Unknown binding record")?,
            )
        } else {
            None
        };
        let mut hierarchy = book.map(|b| b.hierarchy.clone()).unwrap_or_default();
        let mut citations: BTreeSet<(usize, String)> = candidate
            .citations
            .iter()
            .map(|c| (c.segment, c.quote.clone()))
            .collect();
        for field in &candidate.fields {
            let preserved_index =
                if hierarchy[field.slot].name.as_deref() == Some(field.name.as_str()) {
                    hierarchy[field.slot].index
                } else {
                    None
                };
            hierarchy[field.slot] = SeriesValue {
                name: Some(field.name.clone()),
                index: field.index.or(preserved_index),
            };
            citations.extend(field.citations.iter().map(|c| (c.segment, c.quote.clone())));
        }
        if hierarchy[0].name.is_none() {
            hierarchy[0].index = Some(1.0);
        }
        let citations: Vec<_> = citations
            .into_iter()
            .map(|(segment, quote)| Citation { segment, quote })
            .collect();
        let evidence = format!(
            "AI-assisted draft, NOT certified. Model {} ({}); {}. {}\n{}\n{}",
            run.model,
            run.digest,
            run.prompt_version,
            candidate.notes,
            citations
                .iter()
                .map(|c| format!("Segment {}: {}", c.segment, c.quote))
                .collect::<Vec<_>>()
                .join("\n"),
            if book.is_some() {
                "User explicitly bound this record; unmentioned current fields were preserved, not independently verified."
            } else {
                "No library record was automatically bound."
            }
        );
        let reference = Reference {
            title: book
                .map(|b| b.title.clone())
                .unwrap_or_else(|| candidate.title.clone()),
            authors: book
                .map(|b| b.authors.clone())
                .unwrap_or_else(|| candidate.authors.clone()),
            book_uuids: book.map(|b| vec![b.uuid.clone()]).unwrap_or_default(),
            identifiers: book.map(|b| b.identifiers.clone()).unwrap_or_default(),
            hierarchy,
            source_id: source.id.clone(),
            evidence,
            approved: false,
            origin: "AI draft; requires human review".into(),
            edition: candidate.edition.clone(),
            field_mask: Some(std::array::from_fn(|slot| {
                candidate.fields.iter().any(|f| f.slot == slot)
            })),
            ai_provenance: Some(AiProvenance {
                model: run.model,
                digest: run.digest,
                prompt_version: run.prompt_version,
                source_hash: source.sha256.clone(),
                citations,
                extracted_slots: candidate.fields.iter().map(|f| f.slot).collect(),
            }),
            ..Default::default()
        };
        matching::validate_reference(&reference)?;
        validate_provenance(
            reference
                .ai_provenance
                .as_ref()
                .context("Missing AI provenance")?,
            &source,
        )?;
        let mut next = self.data.clone();
        next.ai_decisions.insert(
            key,
            AiDecision {
                status: "draft".into(),
                reference_id: Some(reference.id.clone()),
            },
        );
        next.references.push(reference);
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
    pub fn clear_ai_cache(&mut self) -> Result<()> {
        self.store.clear_ai_cache()?;
        let mut next = self.data.clone();
        next.ai_runs.clear();
        self.store.save(&next)?;
        self.data = next;
        Ok(())
    }
}
