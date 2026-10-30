use curator_core::{app::App, model::Settings};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || !["audit", "import"].contains(&args[1].as_str()) {
        anyhow::bail!("Usage: curator audit WORKSPACE LIBRARY [VERIFIED_HISTORY_DIRECTORY]\n       curator import WORKSPACE LIBRARY SOURCE [UNIVERSE]\nRead-only library commands; never apply metadata changes.");
    }
    let mut app = App::open(Path::new(&args[2]), PathBuf::new())?;
    app.settings(Settings {
        library: args[3].clone(),
        ..app.data.settings.clone()
    })?;
    if args[1] == "import" {
        let source = args
            .get(4)
            .ok_or_else(|| anyhow::anyhow!("Missing source path"))?;
        app.import(source, args.get(5).map(String::as_str).unwrap_or(""), false)?;
    } else if let Some(history) = args.get(4) {
        app.seed(Path::new(history))?;
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
            "missing_comments":snapshot.proposals.iter().filter(|p|!p.book.has_comments).count()
        }))?
    );
    Ok(())
}
