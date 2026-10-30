use curator_core::{app::App, model::Settings};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4
        || ![
            "audit",
            "import",
            "providers",
            "lookup",
            "confirm-source",
            "synopsis",
        ]
        .contains(&args[1].as_str())
    {
        anyhow::bail!("Usage: curator audit WORKSPACE LIBRARY [VERIFIED_HISTORY_DIRECTORY]\n       curator import WORKSPACE LIBRARY SOURCE [UNIVERSE]\n       curator providers WORKSPACE LIBRARY\n       curator lookup WORKSPACE LIBRARY BOOK_ID [PROVIDER]\n       curator confirm-source WORKSPACE LIBRARY LOOKUP_ID\n       curator synopsis WORKSPACE LIBRARY LOOKUP_ID source|ollama\nThese commands can fetch online metadata or generate drafts, but never apply library changes.");
    }
    let mut app = App::open(Path::new(&args[2]), PathBuf::new())?;
    app.settings(Settings {
        library: args[3].clone(),
        ..app.data.settings.clone()
    })?;
    if args[1] == "providers" {
        app.discover_metadata()?;
    } else if args[1] == "lookup" {
        if let Some(provider) = args.get(5) {
            let mut settings = app.data.metadata_settings.clone();
            settings.providers = vec![provider.clone()];
            app.configure_metadata(settings)?;
        }
        let book = args
            .get(4)
            .ok_or_else(|| anyhow::anyhow!("Missing book ID"))?
            .parse::<i64>()?;
        app.lookup_metadata(book, false)?;
    } else if args[1] == "confirm-source" {
        app.confirm_metadata(
            args.get(4)
                .ok_or_else(|| anyhow::anyhow!("Missing lookup ID"))?,
            true,
        )?;
    } else if args[1] == "synopsis" {
        let id = args
            .get(4)
            .ok_or_else(|| anyhow::anyhow!("Missing lookup ID"))?;
        let mode = args
            .get(5)
            .ok_or_else(|| anyhow::anyhow!("Choose source or ollama"))?;
        if !["source", "ollama"].contains(&mode.as_str()) {
            anyhow::bail!("Choose source or ollama");
        }
        if mode == "ollama" {
            let mut settings = app.data.ai_settings.clone();
            settings.enabled = true;
            app.configure_ai(settings)?;
        }
        app.generate_synopsis(id, mode == "ollama", false)?;
    } else if args[1] == "import" {
        let source = args
            .get(4)
            .ok_or_else(|| anyhow::anyhow!("Missing source path"))?;
        app.import(source, args.get(5).map(String::as_str).unwrap_or(""), false)?;
    } else if args[1] == "audit" {
        if let Some(history) = args.get(4) {
            app.seed(Path::new(history))?;
        }
    }
    let snapshot = app.snapshot()?;
    let mut counts = std::collections::BTreeMap::new();
    for proposal in &snapshot.proposals {
        *counts.entry(&proposal.status).or_insert(0) += 1;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "books":snapshot.proposals.len(),"references":snapshot.data.references.len(),
            "statuses":counts,"issues":snapshot.issues,"library_write_performed":false,
            "missing_comments":snapshot.proposals.iter().filter(|p|!p.book.has_comments).count(),
            "providers":if args[1]=="providers"{serde_json::to_value(&snapshot.data.metadata_providers)?}else{serde_json::Value::Null},
            "lookup_results":if args[1]=="lookup"{serde_json::to_value(snapshot.data.metadata_results.iter().filter(|r|r.library==snapshot.data.settings.library&&r.book_id==args[4].parse::<i64>().unwrap_or(0)).map(|r|
                serde_json::json!({"id":r.id,"provider":r.provider.name,"status":r.match_status,"note":r.note,"identity_confirmed":r.identity_confirmed,
                    "title":r.metadata.as_ref().map(|m|&m.title),"description_characters":r.metadata.as_ref().map(|m|m.description.len())})).collect::<Vec<_>>())?}else{serde_json::Value::Null},
            "synopsis_drafts":if args[1]=="synopsis"{serde_json::to_value(snapshot.data.synopsis_drafts.iter().filter(|d|d.library==snapshot.data.settings.library&&d.lookup_id==args[4]).collect::<Vec<_>>())?}else{serde_json::Value::Null}
        }))?
    );
    Ok(())
}
