# Calibre Curator
A local Rust/Tauri 2 desktop application that edits Calibre metadata, not generated reading lists. Series is the whole-book chronological universe sequence; Series2–4 are genuine nested series with their independent numbering. No AI service, API key, subscription or per-batch credits.

## Run
Build using `pnpm package`, then run `fish run-curator.fish` or `target/release/calibre-curator`. The Debian package is `target/release/bundle/deb/calibre-curator_0.1.0_amd64.deb` on x86-64. It includes a desktop launcher, the API bridge and the optional read-only `calibre-curator-audit` utility. Installing the package is separate from building it.

For an application-menu entry without installing the Debian package, run `python3 scripts/install_launcher.py`. It creates a per-user **Calibre Curator** entry pointing to this repository's release build and icon. Rerun it if you move the repository.

Runtime requirements: Linux with GTK 3, WebKitGTK 4.1, and an installed Calibre exposing `calibre-debug`. The default executable is `/usr/bin/calibre-debug`; choose its absolute path in Settings if different. Close Calibre and calibre-server before writing. Read-only scans work while Calibre is open.

The normal workspace is `$XDG_DATA_HOME/org.nexus.calibre-curator` (usually `~/.local/share/org.nexus.calibre-curator`). `CURATOR_WORKSPACE` overrides it, and `CURATOR_LIBRARY` overrides the initial library selection for testing. Keep the entire workspace outside a Calibre library. It contains private cached research, UUID bindings, rules, protections, SQLite state and operation backups; do not commit it.

## Workflow
1. In **Settings**, choose the library and discover/map three distinct existing **series-type** custom columns. Their labels do not include `#`. No columns are automatically created.
2. **Library** audits every record. Search/filter and inspect a book's rule or protections. `correct` means agreement with an approved rule, not merely that a series field is populated. `protected` means pinned user numbering, not independent certification.
3. **References & sources** imports JSON/CSV, ODT, saved HTML, or an explicitly requested public HTTPS page. ODT headings and table rows retain provenance and a SHA-256 hash. Imports are always drafts, even if a JSON file contains `approved:true`.
4. Review work identity, edition scope, evidence and all four target fields. Approve a reference once; future batches reuse it locally. Blank target series fields mean **clear**, not “leave unchanged.” Existing fields are shown separately in the book and review screens.
5. Select eligible books, open **Review**, and select individual changed fields. Explicitly confirm applying to the named library. The bridge obtains Calibre's real writer lock, backs up the database and original target OPFs, checks stale values, writes through Calibre's API, and verifies the result.
6. **History** shows journals, verification, backup locations and guarded undo/recovery. **Diagnostics** separately reports missing comments, publishers, identifiers and formats, without editing them.

Current Greater Foundation Universe and Greater Eternal Champion Saga records receive four-field pins on first scan, including newly completed memberships. Saved explicit unlocks persist; rescanning does not silently re-lock them. Unlocking requires a separate confirmation. Pins can also be set for any individual book/field. Unlocked fields can be reviewed without writing a different pinned field.

### Existing reviewed batches and finalized research
Use **Import verified batch history** on the directory containing `curation-backup-*` subdirectories. Only audits reporting verified status, integrity OK and unchanged unrelated metadata qualify. Manifests must refer to the selected library, and UUID/title identities must still match. The latest successful target per UUID wins; old rankings are not replayed.

Import the finalized Moorcock ODT as a source using universe `Greater Eternal Champion Saga`. Its numbered paragraphs can generate draft main-series references, but nested series are **not guessed** from prose. Current protected user metadata remains intact. Other research documents are not automatically authoritative.

### Identity and chronology
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

Reference export preserves reusable rule data. Reimport resets approval and assigns new local rule/source identities, remapping internal before relations. Aliases can be copied separately into the alias editor. Source caches render as text; no imported HTML, macros or archive entries execute. Files and decompressed ODT XML are bounded to 16 MiB. HTTPS requests pin checked public DNS addresses and refuse redirects; explicitly request a redirect destination instead.

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
- `pnpm package` — locked release builds, resource copy and `.deb`.

The package script directly builds Tauri's custom protocol and stages standard Debian files. It does not use the Tauri CLI's unconditional manifest watcher, which can fail even in production builds when the machine's inotify watch quota is exhausted. No system limits are changed.

The Calibre integration test creates only temporary libraries. It tests actual API apply/undo, stale review refusal, protection, a genuine bridge process exit followed by recovery, lock contention and refusal after intervening comments edits. Linux Calibre singleton names are global per user: a test-only namespace isolates the disposable fixture without changing the production bridge's locking policy. The test validates its exact temporary target before isolation. Calibre uses optimized Python; safety checks never rely on Python `assert` side effects.

The read-only helper accepts `curator audit WORKSPACE LIBRARY [VERIFIED_HISTORY_DIRECTORY]` and `curator import WORKSPACE LIBRARY SOURCE [UNIVERSE]`. Both can update application state but never write Calibre metadata. The package installs this executable as `calibre-curator-audit`.
