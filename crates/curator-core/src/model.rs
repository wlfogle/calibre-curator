use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SeriesValue {
    pub name: Option<String>,
    pub index: Option<f64>,
}
pub type Hierarchy = [SeriesValue; 4];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub library: String,
    pub columns: [String; 3],
    pub calibre_binary: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            library: "/media/loufogle/Data/Calibre Library".into(),
            columns: ["series2".into(), "series3".into(), "series4".into()],
            calibre_binary: "/usr/bin/calibre-debug".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub authors: Vec<String>,
    pub identifiers: BTreeMap<String, String>,
    pub hierarchy: Hierarchy,
    pub publisher: Option<String>,
    pub has_comments: bool,
    pub formats: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub id: i64,
    pub label: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub library: String,
    pub books: Vec<Book>,
    pub columns: Vec<Column>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub ordinal: usize,
    pub kind: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub location: String,
    pub sha256: String,
    pub kind: String,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Reference {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub identifiers: BTreeMap<String, String>,
    pub book_uuids: Vec<String>,
    pub aliases: Vec<String>,
    pub hierarchy: Hierarchy,
    pub source_id: String,
    pub evidence: String,
    pub approved: bool,
    pub origin: String,
    pub coverage: String,
    pub before: Vec<String>,
    pub fixed: bool,
    pub edition: String,
    pub ai_provenance: Option<crate::ai::AiProvenance>,
    pub field_mask: Option<[bool; 4]>,
}
impl Default for Reference {
    fn default() -> Self {
        Self {
            id: new_id(),
            title: String::new(),
            authors: Vec::new(),
            identifiers: BTreeMap::new(),
            book_uuids: Vec::new(),
            aliases: Vec::new(),
            hierarchy: Default::default(),
            source_id: String::new(),
            evidence: String::new(),
            approved: false,
            origin: "draft".into(),
            coverage: "partial".into(),
            before: Vec::new(),
            fixed: true,
            edition: "any".into(),
            ai_provenance: None,
            field_mask: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UniverseAlias {
    pub id: String,
    pub existing_series: String,
    pub universe: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppData {
    pub settings: Settings,
    pub references: Vec<Reference>,
    pub sources: Vec<Source>,
    pub pins: BTreeMap<String, [bool; 4]>,
    pub aliases: Vec<UniverseAlias>,
    pub ai_settings: crate::ai::AiSettings,
    pub ai_models: Vec<crate::ai::AiModel>,
    pub ai_runs: Vec<crate::ai::AiRun>,
    pub ai_decisions: BTreeMap<String, crate::ai::AiDecision>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub book: Book,
    pub status: String,
    pub reason: String,
    pub reference_id: Option<String>,
    pub after: Option<Hierarchy>,
    pub changed: Vec<usize>,
    pub blocked: bool,
    pub pinned: [bool; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub before: Hierarchy,
    pub after: Hierarchy,
    pub evidence: String,
    pub sources: Vec<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub identifiers: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Protection {
    pub uuid: String,
    pub mask: [bool; 4],
    pub values: Hierarchy,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub created: u64,
    pub library: String,
    pub columns: [String; 3],
    pub phase: String,
    pub kind: String,
    pub parent: Option<String>,
    pub backup: Option<String>,
    pub error: Option<String>,
    pub changes: Vec<Change>,
    pub protections: Vec<Protection>,
    pub verification: Option<serde_json::Value>,
}

pub fn hierarchy_from_json(value: &serde_json::Value) -> anyhow::Result<Hierarchy> {
    if value.is_array() {
        return Ok(serde_json::from_value(value.clone())?);
    }
    let mut result: Hierarchy = Default::default();
    for (i, item) in result.iter_mut().enumerate() {
        if let Some(raw) = value.get(format!("series{}", i + 1)) {
            *item = serde_json::from_value(raw.clone())?;
        }
    }
    Ok(result)
}
