# Calibre Curator
A local Rust/Tauri 2 desktop application that edits Calibre metadata, not generated reading lists. Series is the whole-book chronological universe sequence; Series2–4 are genuine nested series with their independent numbering. Normal curation requires no AI service, API key or subscription. Optional local Ollama assistance can extract cited drafts; it never approves rules or writes Calibre metadata.

## Quick start on Pop!_OS 22.04
Install the native build prerequisites below if missing. You also need current Rust/Cargo, Node 20.19+ and pnpm; Ubuntu's older default Node package is not sufficient for this frontend.
```fish
sudo nala install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev python3
git clone https://github.com/wlfogle/calibre-curator.git
cd calibre-curator
pnpm package
python3 scripts/install_launcher.py
fish run-curator.fish
```
Subsequent launches can use **Calibre Curator** in the application menu. Rebuild after pulling source updates with `git pull --ff-only` and `pnpm package`. Preserve the separate application workspace; do not replace it with an empty directory when upgrading.

## Run
Build using `pnpm package`, then run `fish run-curator.fish` or `target/release/calibre-curator`. The Debian package is `target/release/bundle/deb/calibre-curator_0.2.0_amd64.deb` on x86-64. It includes a desktop launcher, the API bridge and the optional read-only `calibre-curator-audit` utility. Installing the package is separate from building it.

To install the built package rather than run from the checkout, use `sudo nala install ./target/release/bundle/deb/calibre-curator_0.2.0_amd64.deb`, then open the application menu. `amd64` means 64-bit Intel/AMD architecture, not Arch Linux. Calibre itself must be installed separately and provide `calibre-debug`.

For an application-menu entry without installing the Debian package, run `python3 scripts/install_launcher.py`. It creates a per-user **Calibre Curator** entry pointing to this repository's release build and icon. Rerun it if you move the repository.

Runtime requirements: Linux with GTK 3, WebKitGTK 4.1, and an installed Calibre exposing `calibre-debug`. The default executable is `/usr/bin/calibre-debug`; choose its absolute path in Settings if different. Close Calibre and calibre-server before writing. Read-only scans work while Calibre is open.

The normal workspace is `$XDG_DATA_HOME/org.nexus.calibre-curator` (usually `~/.local/share/org.nexus.calibre-curator`). `CURATOR_WORKSPACE` overrides it, and `CURATOR_LIBRARY` overrides the initial library selection for testing. Keep the entire workspace outside a Calibre library. It contains private cached research, UUID bindings, rules, protections, SQLite state and operation backups; do not commit it.

## Workflow
1. In **Settings**, choose the library and discover/map three distinct existing **series-type** custom columns. Their labels do not include `#`. No columns are automatically created.
2. **Library** audits every record. Search/filter and inspect a book's rule or protections. `correct` means agreement with an approved rule, not merely that a series field is populated. `protected` means pinned user numbering, not independent certification.
3. **References & sources** imports JSON/CSV, ODT, saved HTML, or an explicitly requested public HTTPS page. ODT headings and table rows retain provenance and a SHA-256 hash. Imports are always drafts, even if a JSON file contains `approved:true`.
4. Review work identity, edition scope, evidence and all four target fields. Approve a reference once; future batches reuse it locally. Blank selected target series fields mean **clear**, not “leave unchanged”; unchecked fields in a partial rule stay unchanged. Existing fields are shown separately in the book and review screens.
5. Select eligible books, open **Review**, and select individual changed fields. Explicitly confirm applying to the named library. The bridge obtains Calibre's real writer lock, backs up the database and original target OPFs, checks stale values, writes through Calibre's API, and verifies the result.
6. **History** shows journals, verification, backup locations and guarded undo/recovery. **Diagnostics** separately reports missing comments, publishers, identifiers and formats, without editing them.

**Universes** groups recognized owned records by approved target, candidate alias or existing primary series. It shows approved-match/unresolved counts, partial-source warnings, nested subseries, proposed memberships and gaps in parent fields. It never certifies unknown universe membership merely because a group exists. Applying operations streams live lock/backup/write/OPF/verification progress into the status bar while preserving durable journals.

Current Greater Foundation Universe and Greater Eternal Champion Saga records receive four-field pins on first scan, including newly completed memberships. Saved explicit unlocks persist; rescanning does not silently re-lock them. Unlocking requires a separate confirmation. Pins can also be set for any individual book/field. Unlocked fields can be reviewed without writing a different pinned field.

### First reviewed change
1. Choose the directory containing `metadata.db`, not an individual ebook folder. In Settings, click **Discover compatible columns**, enter their labels and click **Validate, save and scan**. Create any missing series-type columns in Calibre itself.
2. Find a book in Library and choose **View / edit → Create reviewed reference**. Starting values are copied from the record for convenience; this does not certify them.
3. Enter the overarching chronology and genuine subseries values, cite your evidence or explicit numbering decision, and check **I reviewed this evidence and approve this rule** only after reviewing the complete target.
4. Return to Library, select the eligible changed record, and choose **Review selected**. Check only the fields you intend to change. Unchecked fields are not written.
5. Close Calibre normally, choose **Apply checked changes**, verify the named library and confirm. Wait for the verified History entry before reopening Calibre.
6. Rescan to check agreement with the approved rule. If reversal is necessary, close Calibre and use **Guarded undo** promptly, before other edits invalidate the checkpoint.

A new checkout does not contain anyone's private pre-seeded library rules. Import your own references/history or create reviewed decisions. Saved rules and pins persist across launches; ordinary rescans do not need AI or repeat research.

### Status guide
- **correct:** selected fields agree with an approved reference. A partial-field rule does not certify its unselected fields.
- **missing / conflict:** supported changes are available for review; a populated current value is not inherently correct.
- **protected:** one or more fields are pinned. Unlocked supported fields may still be selectable.
- **unresolved:** identity, chronology or evidence is insufficient, or approved rules conflict. Add evidence or resolve the conflict instead of forcing a batch.

Use **Universes → Inspect these books** for an exact group filter; clear that filter to return to the whole library. **Select eligible on this page** selects only the currently displayed page, not every matching record.

## Optional Ollama assistance
### Connect and choose a model
Run Ollama separately and install/download models only if you choose to. Curator never pulls models automatically. `ollama list` shows registered models; `ollama serve` starts a server if no service is already running.

In **AI assistant**:
1. Leave the origin at `http://127.0.0.1:11434` for a local server.
2. Check **Enable optional Ollama suggestions**, choose a model name and save the settings.
3. Click **List installed models**. Start with an installed text-completion model such as `qwen3.5:4b`; compare a larger model only if you need it and have sufficient memory. Embedding-only, cloud and remote models are not extraction models.

Point Curator at the server URL, **not** its `models`, `blobs` or manifest directory. Ollama manages those files. A private LAN/VPN server requires the explicit LAN permission checkbox; selected excerpts then leave this computer. Public destinations, link-local metadata addresses, credential-bearing URLs, redirects and inherited proxies are blocked. No hosted-provider fallback exists.

### Extract, inspect and keep a draft
1. Import the source in **References & sources** first.
2. In AI assistant, select that cached source and an inclusive segment range. Inspect the exact preview. Start with a few paragraphs; requests allow at most 32 segments and 12,000 UTF-8 bytes of source text.
3. Optionally supply a user-chosen universe context. That context supplies a name, not proof of chronology.
4. Click **Extract cited drafts** and confirm sending only the previewed excerpts. Curator does not automatically send the library inventory, ebook contents or comments.
5. Review the inbox, warnings and **Inspect exact cited source text**. Literal quotations, names, numbers and field limits are checked; these checks do not prove the model's interpretation is correct. “Insufficient evidence” is a valid result, not a reason to invent an order.
6. Choose **Create unapproved reference**. An optional library binding requires your explicit record selection. Omitted fields stay unchanged even without binding; current values copied from a bound record are not independently verified.
7. Open the reference, review its selected-field mask, identity, evidence and numbers, then approve it manually if justified. Apply later through the ordinary field-level Review screen. AI cannot approve, unlock pins or call the writer.

The evidence inspector retains source hash, literal segments, model digest and prompt version. Warnings highlight same-title existing rules; compare them before creating contradictory references. The **Review priority queue** ranks draft availability, conflicts, parent-field gaps and edition warnings using ordinary code, with no inference calls or fabricated confidence percentages.

If a model quotes only part of an identity-bearing paragraph, Curator may expand that identity citation to the exact cached segment already cited by that candidate or one of its fields (up to 1,200 bytes). The inbox labels this as backend-added context. Original quotes are validated first; no uncited or unselected segments are searched, and absent names/fabricated quotes still fail validation. This does not infer or repair hierarchy numbers.

### Cache, cost and limits
Repeat requests reuse validated responses keyed by source/segment content, context, model digest, endpoint and prompt/schema version. The server catalogue is checked before reuse so updated model weights invalidate generation caches. Generation token counts and duration are shown; a cache hit reports the original generation statistics.

Dismissals and created-draft decisions are remembered by suggestion fingerprint. **Regenerate** bypasses the response cache but does not silently forget those decisions. The inbox retains 40 recent selections; cached responses remain in the private workspace until cleared. Clearing the AI cache removes cached results/inbox entries, not sources, rules, pins or remembered decisions.

Existing inbox results can be reviewed without Ollama. New extraction requests—including catalogue checks—need the configured server. Normal manual curation remains usable with AI disabled or offline. Each request has a configurable 15–180 second timeout, bounded response size and generation limits. Truncated, malformed, fabricated-citation and tool-call responses are refused, not partly imported. Choose a smaller excerpt range if output limits are reached.

Local inference uses hardware and electricity, but no paid agent/API credits. No model downloads, model-weight changes, training or fine-tuning are performed by Curator. Trust the operator of any configured private server.

## Existing reviewed batches and finalized research
Use **Import verified batch history** on the directory containing `curation-backup-*` subdirectories. Only audits reporting verified status, integrity OK and unchanged unrelated metadata qualify. Manifests must refer to the selected library, and UUID/title identities must still match. The latest successful target per UUID wins; old rankings are not replayed.

Import the finalized Moorcock ODT as a source using universe `Greater Eternal Champion Saga`. Its numbered paragraphs can generate draft main-series references, but nested series are **not guessed** from prose. Current protected user metadata remains intact. Other research documents are not automatically authoritative.

## Identity and chronology
Matching priority: reviewed UUID binding with identity checks; whitelisted work/edition identifiers (including checksum-validated ISBN-10/13); approved title/author or title aliases. Generic plugin fields such as revision numbers are not identity evidence. Same-title unrelated authors remain unmatched. Conflicting targets stay blocked. Known collection/omnibus titles cannot silently match a novel-scope reference. Title-based matching still requires the user's edition review; the application does not automatically inspect every ebook's full contents.

Universe aliases suggest candidate membership only; they never invent a chronological position. Add them in the alias editor using objects with `id`, `existing_series` and `universe`.

Chronology rules support fixed main-series anchors and explicit `before` relations. **Calculate draft positions** only accepts a uniquely constrained whole-work order, preserves fixed anchors, uses fractional positions between anchors, and makes newly calculated positions unapproved. Cycles, contradictory anchors and incomplete order are rejected instead of alphabetically tie-breaking. Duplicate editions with the same normalized title, first author and declared edition scope share a work position. Different editions/collections may need distinct references. Subseries numbers are never globally re-ranked.

Partial coverage is explicitly marked. Unknown universes and disputed timelines still need evidence or a user decision once; this is not an offline encyclopedia. Intrinsic flashbacks, interleaving novels and multi-era collections cannot be made perfectly linear by changing whole-book metadata.

## Source formats
CSV headers: `title,authors,universe,position,series2,index2,series3,index3,series4,index4,evidence,isbn,uuid`. Only `title` is structurally required for a draft. Separate authors with `|`. Named approved fields need finite numbers; empty custom fields need empty indices. Author, identifier or explicit UUID binding is needed for approval. See `examples/rules.csv`.

JSON is an array of reference objects or an object containing `references`. Important fields:
- `id`, `title`, `authors`, `identifiers`, `book_uuids`, `aliases`.
- `hierarchy`: four `{ "name": ..., "index": ... }` objects, in Series/Series2/Series3/Series4 order. Empty primary series uses index 1; empty custom series use null indices.
- `evidence`, `coverage` (`partial` or `complete`), `edition` (`any`, `novel`, `collection`, `omnibus`).
- `fixed` and `before` (reference IDs within the same universe).
- `field_mask`: optional four-boolean array selecting which hierarchy fields the rule controls. Omitted masks select all fields; false slots preserve each matched record's current value. AI drafts use explicit partial masks.

Reference export preserves reusable rule data. Reimport resets approval and assigns new local rule/source identities, remapping internal before relations. Aliases can be copied separately into the alias editor. Source caches render as text; no imported HTML, macros or archive entries execute. Files and decompressed ODT XML are bounded to 16 MiB. HTTPS requests pin checked public DNS addresses and refuse redirects; explicitly request a redirect destination instead.

AI-assisted exports retain provenance and partial-field masks. To approve them in a new workspace, import the original source and select it in the reference editor so its hash and quotations can be verified. Importing only an exported rules file is not a substitute for the original cited document.

## Safety and recovery
Each operation directory includes:
- `operation.json`: complete before/after targets, identity, evidence, protected-field snapshot, phase and verification.
- `backup/metadata.db`: consistent pre-write SQLite backup.
- `backup/original-opf/`: original target OPFs, where present.
- `guard.json`: durable semantic checkpoint, target modification timestamps and OPF hashes.
- `bridge.log`: installed Calibre output, including noisy plugins.

Verification covers every book's hierarchy and identity, unrelated tables, category payloads, protected fields, target OPFs and SQLite integrity. Narrow expected Calibre bookkeeping changes are excluded: target modification timestamps, the metadata-dirtied queue, category sequences and trash-expiry housekeeping. Comments, publishers, identifiers, unrelated custom metadata and ebook files are not curation targets.

Undo and interrupted recovery reapply the original target fields **through Calibre**, after comparing the current library with the exact saved checkpoint and checking unrelated metadata against the original backup. Even an unrelated intervening edit can conservatively block reversal. No automatic database-backup overwrite is offered. A crash between a database commit/OPF write and its checkpoint also blocks automatic recovery: retain the workspace and inspect the journal/backups manually. This refusal is intentional, not permission to restore blindly.

Unfinished writes block new batches. Pre-write failures with no backup/checkpoint may be dismissed only after acquiring the workspace writer lock. Successful reversals mark their parents undone/recovered. Reversing a newly protected membership requires explicitly unlocking the affected fields first. The normal close button is disabled during backend work; OS termination/power loss is still possible, hence the journal.

## Build and validation
Development dependencies: Rust/Cargo (tested 1.99), Node 20.19+ and pnpm, Linux GTK3/WebKitGTK 4.1 development packages, a C toolchain and `dpkg-deb`.

- `pnpm install --frozen-lockfile`
- `pnpm build` — TypeScript checking and production frontend.
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test -p curator-core`
- `python3 tests/window_smoke.py` — actual native window; requires X11, xdotool, ImageMagick and tesseract. Creates a disposable library and verifies rule editing, approval, field review, the real production-lock refusal, history and an unchanged database checksum. Screenshots stay in its printed temporary directory. OCR avoids relying on an unavailable desktop accessibility bus.
- `python3 tests/window_smoke.py --ai-workflow` — explicit native AI test, requiring local Ollama and installed `qwen3.5:4b`. Uses fictional source text only, enables AI in a disposable workspace, checks excerpt selection and inference, and verifies draft-only or insufficient-evidence behavior with an unchanged database checksum. Leave mouse/keyboard idle during native automation. The ordinary native test also checks model discovery, but does not request inference.
- `pnpm package` — locked release builds, resource copy and `.deb`.

The package script directly builds Tauri's custom protocol and stages standard Debian files. It does not use the Tauri CLI's unconditional manifest watcher, which can fail even in production builds when the machine's inotify watch quota is exhausted. No system limits are changed.

The Calibre integration test creates only temporary libraries. It tests actual API apply/undo, stale review refusal, protection, a genuine bridge process exit followed by recovery, lock contention and refusal after intervening comments edits. Linux Calibre singleton names are global per user: a test-only namespace isolates the disposable fixture without changing the production bridge's locking policy. The test validates its exact temporary target before isolation. Calibre uses optimized Python; safety checks never rely on Python `assert` side effects.

The read-only helper accepts `curator audit WORKSPACE LIBRARY [VERIFIED_HISTORY_DIRECTORY]` and `curator import WORKSPACE LIBRARY SOURCE [UNIVERSE]`. Both can update application state but never write Calibre metadata. The package installs this executable as `calibre-curator-audit`.

Keep custom workspaces outside the checkout as well as outside Calibre libraries; they contain private citations, bindings and operation journals.

Example from the built checkout:
```fish
set workspace "$HOME/.local/share/org.nexus.calibre-curator"
set library "$HOME/Calibre Library"
target/release/curator audit "$workspace" "$library"
```
Replace the library path with your own. For an installed Debian package, use `calibre-curator-audit` in place of `target/release/curator`.

The opt-in real-model regression uses fictional source text only: `cargo test -p curator-core --test ai real_local_ollama_extraction -- --ignored --nocapture`. Normal tests use mock servers and do not invoke your installed model.

## Troubleshooting
- **No compatible columns:** create three series-type custom columns in Calibre, then rediscover their labels in Settings.
- **Calibre lock refusal:** close the GUI and content server normally. Do not kill jobs or bypass the lock.
- **Stale values / conflicting references:** rescan and inspect existing rules. Do not manually modify an operation journal to force it through.
- **Undo/recovery refused:** retain all backups and journals; a newer edit or uncheckpointed write requires manual review, not a blind database restore.
- **Ollama unavailable / model missing:** check the origin and `ollama list`, then refresh installed models. No automatic model download occurs.
- **AI citation/schema error:** use a clearer, smaller excerpt or another compatible local model. Nothing is approved or imported from the rejected response.
- **Desktop launcher fails after moving the checkout:** rebuild and rerun `python3 scripts/install_launcher.py`, or install the Debian package.
