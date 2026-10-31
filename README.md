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
### Version 0.5.0
- Adds **Research chronology and subseries** directly in Repair: automatic DuckDuckGo discovery, bounded source fetching, excerpt selection and cited findings.
- Distinguishes story chronology from publication/recommended order; partial timelines, fictional years, disputed positions and collections do not become invented global numbers.
- Uses the installed local Ollama model when enabled; no documents, URLs or excerpts need to be supplied for each book.
- Requires an independent, verified **full-library backup, including ebooks**, before every apply or reversal. Insufficient space, unreadable/changing files and failed verification block writes.
- Adds **History → Revert this batch**, preserving guarded API restoration and interrupted recovery. Older metadata-only backups are labelled honestly.
### Version 0.4.0
- Opens directly into **Repair**: scan, reuse verified sources, preview fixes together, and apply one backed-up batch.
- Adds source-level verification of ordered ODT/HTML book entries and structured CSV/JSON mappings; no per-book rule approval is required for supported matches.
- Uses one overarching sequence including novellas and short stories, with independent nested subseries numbers. Unowned works retain their places.
- Fills missing **Comments/synopsis, publisher and recognized identifiers** from reviewed compatible Calibre provider results. Populated values are preserved.
- Adds cached DuckDuckGo author/series website discovery, with honest partial/blocked results and no manual-challenge dependency.
- Moves individual rules, model configuration and detailed diagnostics under **Advanced**. Existing workspaces, pins and history remain supported.
### Version 0.3.1
- Searches **all enabled Calibre identification providers** by default, including newly enabled plugins. There is no three-provider selection limit.
- Migrates older Google-only settings to all-provider mode unless you explicitly choose a custom subset in the updated application.
- Continues through provider failures and saves individual results as they finish, with provider-by-provider progress.
- Adds **Search web with DuckDuckGo** for explicit browser-based source discovery. Google Books remains a Calibre metadata provider; no Google web search fallback is used.
### Version 0.3.0
- Adds the **Synopses** workspace and per-provider `fetch-ebook-metadata` lookups using existing Calibre plugin configuration.
- Adds explicit work/edition confirmation, provider-description reuse, cited Ollama synopsis drafts, text review and blank-Comments-only apply.
- Extends backups, OPF verification, history, guarded undo and interrupted recovery to Comments while preserving series metadata.
- Adds read-only CLI commands for provider discovery, lookup, source confirmation and drafting. There is no CLI command that applies synopsis changes.

This release does not certify arbitrary provider/model output, repair ebook files, automatically identify mixed-content books, overwrite populated Comments or download models. Validate a disposable library before your first write. Network/model tests depend on provider availability and installed local models; deterministic safety tests run without those services.

### Launch or install
Build using `pnpm package`, then run `fish run-curator.fish` or `target/release/calibre-curator`. The Debian package is `target/release/bundle/deb/calibre-curator_0.5.0_amd64.deb` on x86-64. It includes a desktop launcher, the API bridge and the optional read-only `calibre-curator-audit` utility. Installing the package is separate from building it. Close older Curator windows before upgrading to avoid simultaneous versions saving different application-state schemas.

To install the built package rather than run from the checkout, use `sudo nala install --no-install-recommends ./target/release/bundle/deb/calibre-curator_0.5.0_amd64.deb`, then open the application menu. `amd64` means 64-bit Intel/AMD architecture, not Arch Linux. Calibre itself must be installed separately and provide `calibre-debug` and `fetch-ebook-metadata`.

For an application-menu entry without installing the Debian package, run `python3 scripts/install_launcher.py`. It creates a per-user **Calibre Curator** entry pointing to this repository's release build and icon. Rerun it if you move the repository.

Runtime requirements: Linux with GTK 3, WebKitGTK 4.1, and an installed Calibre exposing `calibre-debug`. The default executable is `/usr/bin/calibre-debug`; choose its absolute path in Settings if different. Close Calibre and calibre-server before writing. Read-only scans work while Calibre is open.

The normal workspace is `$XDG_DATA_HOME/org.nexus.calibre-curator` (usually `~/.local/share/org.nexus.calibre-curator`). `CURATOR_WORKSPACE` overrides it, and `CURATOR_LIBRARY` overrides the initial library selection for testing. Keep the entire workspace outside a Calibre library. It contains private cached research, UUID bindings, rules, protections, SQLite state and operation backups; do not commit it.

## Normal workflow
1. Open **Repair** and click **Scan library**. The app also performs a read-only scan at startup. Configure the library and existing series-type custom columns once through **Library settings**.
2. Your own documents are optional. In **Sources**, you can add reference files or public HTTPS pages, preview mappings and verify them once. Alternatively, choose a book in **Repair** and click **Research chronology and subseries**; the app finds and fetches relevant sources itself.
3. Return to **Repair**. Supported series/subseries corrections are selected together; protected fields stay unchanged. Use the optional book filter to inspect a smaller group. The series selection remains library-wide and the final confirmation reports its size.
4. Click **Find missing metadata** when needed. Up to 20 filtered books are looked up per request, using all enabled Calibre identification providers by default. Every provider gets an independent outcome. Consistent compatible results are prepared for preview; differing results require choosing a provider. Failed/mismatched results stay held. No ebooks or complete library inventory are uploaded.
5. Inspect the proposed synopsis, publisher and missing identifier values beside the series corrections. Uncheck unwanted fields. The final **Apply selected fixes** confirmation also confirms the displayed work/edition matches; there is no separate per-book confirmation-and-draft-transfer sequence.
6. Close Calibre, confirm the named library, and apply. A complete verified library copy must finish before metadata changes start. The entire selection receives one checkpoint journal and verified History entry. **History → Revert this batch** restores its edited fields if no intervening edits invalidate the checkpoint. Interrupted batches use guarded recovery; the batch is journaled, not a promise of a single atomic database transaction.

Automatic fills currently cover blank Comments, blank publisher and absent/blank recognized identifier keys only. Existing identifiers are retained, invalid identifiers are filtered, and a conflicting edition is not forced through. Dates, covers, tags, languages, ratings, titles and authors are not fill targets. Long descriptions remain held rather than silently truncated; the optional advanced Ollama tools can prepare a shorter reviewed synopsis.

### Automated chronology research
Filter/select a book in **Repair**, then click **Research chronology and subseries** and confirm the bounded web request. The app uses title, authors and existing series labels as search hints, runs at most two DuckDuckGo queries, and fetches at most six distinct HTTPS pages, including relevant chronology/bibliography links found on them. Previously downloaded pages and compatible research runs are cached privately. Applicable reviewed rules are reused first; **Refresh research** explicitly requests new evidence.

For cited prose/table interpretation, enable your installed local model once in **Advanced → AI assistant**. This uses the existing local/private endpoint restrictions and never downloads a model or invokes a hosted fallback. With AI disabled or unavailable, explicit source-declared complete chronological HTML lists can still be parsed deterministically. This fallback deliberately does not treat an arbitrary numbered reading list as full universe chronology.

Automatic findings retain exact source citations, URL/hash/fetch time, query provenance, model digest and extraction version. Inspect these alongside normal fixes and include the supported result in the final batch Apply; no separate source approval or manual draft transfer is required. Unsupported fields remain unchanged. Findings with conflicting numbers or scope remain held; protected numbering is never unlocked.

Main-series numbers require an explicitly supported chronological position or a complete scoped chronological list. Publication years, fictional years, event rows and recommended reading order are not numeric book positions. A narrower subseries list does not replace an established broader universe. An old unverified main-series label can be recognized as a subseries when the same evidence explicitly supplies that nested membership. Subseries retain their independently cited numbers. Unowned works do not compress positions. Relative claims can yield an interpolated position only between two uniquely matching, current reviewed anchors in the same overarching series; uncertain, missing or stale anchors leave it unresolved. Collections/omnibuses are not forced into one global whole-book chronological rank.

The local model can interpret evidence, not invent missing facts. Shortened quotations may be expanded to the exact already-cited paragraph (maximum 1,200 bytes), after validating the original quotation; the findings label backend-added context. No uncited paragraphs are searched to repair a missing name. Citation validation and agreement between pages are not independent factual certification; the final batch review remains necessary. Same-site copies are not counted as independent corroboration.

Research can return **ready** (possibly partial fields), **conflict**, **unresolved** or **blocked**. A blocked search preserves usable leads/evidence and stops without a CAPTCHA loop or Google substitution. Website structure and source availability limit coverage: the public Known Space smoke check successfully fetched sources but, with AI disabled, did not invent a global rank from timeline years. Some books genuinely remain unresolved. Successful run caches last a day, unresolved/blocked results five minutes, and fetched pages seven days; refresh bypasses reuse.

### Full-library backups and Revert
Every library-changing operation, including Revert/recovery, must first acquire Calibre's writer lock and create an independent full snapshot at the operation's `backup/library` directory, outside the library. This includes ebooks, covers, all OPFs, empty directories and auxiliary files. The database uses SQLite's consistent backup API, including committed WAL content; transient WAL/SHM/journal files are not copied as a substitute for a consistent database. Files are copied, not hard-linked, and snapshot access is owner-only.

Free space is checked for the complete copy, recovery database/OPF compatibility copies and safety headroom. Every copied file has a verified SHA-256 manifest; inventory or database changes during copying invalidate the backup. Symlinks, special/unreadable files, insufficient space and copy/hash/integrity failures refuse metadata writes rather than producing a partial certified backup. Large libraries can require substantial time and disk space. Backups are retained, not automatically deleted.

**History → Revert this batch** shows the backup location and coverage. It verifies the original full snapshot before restoring the batch's original edited fields through Calibre's API, then verifies metadata/OPFs again. It never blindly overwrites the live library or ebook contents. Intervening metadata edits or an uncheckpointed write can block Revert; the snapshot remains available for separately reviewed disaster recovery. New reversals receive their own complete pre-change snapshot. Older journals remain usable through their existing guarded metadata-only recovery path and are explicitly labelled as not being complete library copies.

### Verified source ordering
For **every** overarching series, main-series positions follow the verified document's listed works, including novellas and short stories. Numbering does not restart at subseries boundaries and does not close gaps for unowned books. This is a user-verified ordering policy, not an inference that publication years establish story chronology. Generic “Phase” headings and format labels never become subseries.

The ordered prose extractor supports entries such as `A Work (2020) – Branch #2`, `An Interlude (2020) – Novella`, and leading-number entries such as `3. Another Work (2021)`. Trailing explicit branch numbers populate Series2; CSV/JSON can explicitly supply Series2–4. Optional aliases normalize source labels to your preferred names. Review excluded paragraphs: unsupported free-form prose is not guessed, and a failed/partial extraction does not certify complete coverage. No titles, authors or franchise-specific mappings are hard-coded.

Verification is stored against the cached source hash and exact extracted segments. Importing changed content creates an unverified source; it does not transfer approval from the old version. Revoking verification disables its document-generated corrections without rewriting existing library metadata. Duplicate source entries, ambiguous owned editions and conflicting rules stay unresolved. Matching requires title plus author context; a title alone is insufficient. Fields not supported by that source remain unchanged, including an existing nested value on an unnumbered novella.

### Additional source discovery
**Sources → Find additional author or series websites** sends the entered query to DuckDuckGo's Instant Answer and HTML endpoints. Results retain query/time/engine provenance, are cached privately, and are always **unverified leads**. Structured topic answers are not full web search. If HTML search is challenged or unavailable, usable structured leads remain visible and the run reports partial/blocked status; there is no CAPTCHA loop, mandatory browser verification, Google fallback or paid-service dependency. Local verified-source repair continues normally.

Choose a public HTTPS result explicitly to fetch it as a reference, then review and verify its supported mappings. Fetches retain the public-address/DNS pinning, size, timeout and no-redirect safeguards. Sources are rendered as text; fetched content never executes commands or approves itself. Successful query retrieval is not a claim of complete author or series coverage. Cached successful searches last one day; partial/blocked runs last five minutes. AI-cache clearing preserves discovery caches.

## Advanced workflow
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

## Metadata lookup and book synopses
The **Synopses** tab is separate from series curation. It can query your installed Calibre metadata providers, retain their descriptions as drafts, or use local Ollama to write a short source-grounded synopsis. Only blank Comments can be filled. Lookup never changes a book's title, author, identifiers, publisher, cover or series metadata.

### Choose sources and look up a book
1. Scan the library and open **Synopses**. Catalogue/export records are excluded by default. Records without an ebook file can still be researched, but absence/corruption/mixed contents may prevent reliable identification.
2. Open **Metadata provider settings**, verify the absolute `fetch-ebook-metadata` path, and click **Discover Calibre providers**.
3. Leave **Search ALL enabled identification providers** checked to search every usable plugin. To choose a subset, uncheck it, Ctrl-click any number of providers and save; there is no three-source cap. Choose a 5–30 second per-provider timeout. Cover-only and disabled providers are excluded; enable providers/configure credentials in Calibre itself. Curator does not read/copy API keys.
4. Find/select a book with blank Comments and click **Search all metadata providers** (or **Look up selected book** in custom mode). Confirm sharing that selected book's public title, authors and recognized identifiers with the chosen online providers. No ebook files or full library inventory are uploaded.
5. Review each provider's returned title, authors, identifiers, description, version and fetch time separately. Identifier search is preferred, but the returned title/author must also be consistent. Different works, conflicting identifiers and different editions stay blocked.
6. Click **Confirm matching work and edition** only after checking the actual work, edition and collection/omnibus scope. This confirms identity, not the truth of every description sentence. Identical descriptions returned by different providers are not independent corroboration.

Calibre's CLI returns its top result, not a complete candidate list. A successful lookup can still be the wrong book. Where the record itself is misattributed, correct/establish its identity before confirming evidence; do not force a mismatch through the workflow. Curator does not silently repair corrupt EPUBs or infer which work a mixed-content file represents.

All-provider mode rediscovers the enabled identification plugins for each lookup, then runs each separately; it does not merge everything through one opaque Calibre CLI result. It continues after failures and after successful matches, saves completed results individually, and reports progress through the full provider list. A full search can take several minutes: providers run sequentially with their own timeout and a small startup allowance. Responses are cached privately for seven days by default (configurable 0–30); **Refresh lookup** bypasses reuse. Each subprocess has bounded output and a wall-clock limit. On timeout only its own process group is stopped, not an existing Calibre GUI or content-server job. Raw plugin diagnostics are discarded to avoid retaining accidentally logged credentials. Providers can still require accounts, fail, impose quotas or return no description.

**DuckDuckGo web discovery:** select a book and choose **Search web with DuckDuckGo** to open its title/authors/ISBN query in your default browser. This is separate from Calibre providers such as Google Books, Goodreads and StoryGraph. It does not automatically fetch result pages, import snippets or approve metadata. Open and verify original publisher/author/bibliographic pages; search results are leads, not evidence. DuckDuckGo is the web-search engine, with no Google web-search substitution.

### Reliable source material
Prefer an official publisher or author description tied to the correct work/edition, matching jacket/back-cover copy, or a description from a bibliographic service with consistent identifiers. Calibre lookup providers are acquisition channels, not automatic certificates of reliability. Public links are derived from recognized returned identifiers when possible; missing URLs are reported honestly.

Readable owned ebook text/front matter can help establish identity, but this workflow does not upload or summarize entire ebooks automatically. A title alone, generic fan reading order or model memory is insufficient. A short story with little published description may remain unresolved rather than receive an invented plot.

### Create, approve and apply
1. After identity confirmation, inspect the exact description. Descriptions shorter than 80 UTF-8 bytes are held as insufficient input.
2. Choose **Use source description as draft** to retain a suitable short blurb without AI, or **Draft synopsis with Ollama** to condense it. Ollama uses the existing optional AI configuration and local-model restrictions. Direct reuse and model output are bounded to 4,000 UTF-8 bytes and 220 words; longer blurbs need shortening.
3. Inspect the unapproved draft, model/source provenance and literal description citations. The model has a separate synopsis schema and cannot choose a library record, approve text or invoke the writer. “Insufficient evidence” creates no approvable text.
4. Edit the plain synopsis text if needed, check **I reviewed this synopsis against the source and approve it**, and click **Save synopsis review**. Changes are still only in the private workspace.
5. Select the approved draft and click **Apply selected approved synopses**. Confirm the named library and close Calibre normally first. The application rechecks current identity and exact blank before-Comments under Calibre's real lock, escapes plain text into simple HTML, and writes only Comments.
6. Wait for a verified **comments** operation in History. Verification covers all books' hierarchy and Comments, target OPF descriptions, unrelated metadata and integrity. Existing nonblank Comments are never overwritten, including nonempty stored HTML. If necessary, use the matching History entry's guarded undo before further edits.

Synopsis source confirmation and text approval are separate. Revoking a source confirmation also removes approval from its drafts. Model responses are cached by exact evidence, model digest, endpoint and prompt version; **Regenerate synopsis** explicitly bypasses the AI cache. Clearing AI response caches does not delete source evidence, metadata lookup caches, reviewed synopsis drafts or operation backups.

### Read-only CLI lookup and drafting
From the built checkout, replace the library and book ID below with your own selected record:
```fish
set workspace "$HOME/.local/share/org.nexus.calibre-curator"
set library "$HOME/Calibre Library"
target/release/curator providers "$workspace" "$library"
target/release/curator lookup "$workspace" "$library" 42
```
The lookup uses the saved all-provider mode by default. An optional trailing provider name explicitly switches to that single-provider custom selection; it is not needed to search all sources. Output includes result IDs and identity status. After personally reviewing the result:
```fish
set lookup_id "paste-reviewed-compatible-result-id-here"
target/release/curator confirm-source "$workspace" "$library" "$lookup_id"
target/release/curator synopsis "$workspace" "$library" "$lookup_id" source
```
Use `ollama` instead of `source` for model drafting; this explicit CLI choice enables the stored optional AI connection. These commands update only the application workspace and never apply library changes. Approve/apply through the desktop review workflow. The Debian package installs the same helper as `calibre-curator-audit`.

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

Import research documents in **Sources** and verify their order and author context to reuse their supported mappings. The advanced import path can still create unapproved numbered-prose drafts. Nested numbers must be explicit; they are never guessed. Current protected user metadata remains intact, including previously pinned universe numbering.

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
- `operation.json`: complete before/after targets, identity, evidence, protected-field snapshot, phase and verification, plus complete related reviewed research runs with source URLs, hashes, citations, model provenance and anchors.
- `backup/metadata.db`: consistent pre-write SQLite backup.
- `backup/library/`: verified independent full-library snapshot for operations created by 0.5.0 and later.
- `backup/manifest.json`: full snapshot file hashes, inventory and verification status.
- `backup/original-opf/`: original target OPFs, where present.
- `guard.json`: durable semantic checkpoint, target modification timestamps and OPF hashes.
- `bridge.log`: installed Calibre output, including noisy plugins.

Verification covers every book's hierarchy and identity, unrelated tables, category payloads, protected fields, target OPFs and SQLite integrity. Narrow expected Calibre bookkeeping changes are excluded: target modification timestamps, the metadata-dirtied queue, relevant category/comment/identifier sequences and trash-expiry housekeeping. Series-only operations preserve Comments. Synopsis-only operations preserve hierarchy. Combined repairs additionally verify every book's publisher and identifier map when those fields are selected, and every book's Comments when synopses are selected. Populated non-series values are protected under the writer lock. Unrelated custom metadata and ebook files are never write targets.

Undo and interrupted recovery reapply the original target fields **through Calibre**, after comparing the current library with the exact saved checkpoint and checking unrelated metadata against the original backup. Even an unrelated intervening edit can conservatively block reversal. No automatic database-backup overwrite is offered. A crash between a database commit/OPF write and its checkpoint also blocks automatic recovery: retain the workspace and inspect the journal/backups manually. This refusal is intentional, not permission to restore blindly.

Unfinished writes block new batches. Pre-write failures with no backup/checkpoint may be dismissed only after acquiring the workspace writer lock. Successful reversals mark their parents undone/recovered. Reversing a newly protected membership requires explicitly unlocking the affected fields first. The normal close button is disabled during backend work; OS termination/power loss is still possible, hence the journal.

## Build and validation
Development dependencies: Rust/Cargo (tested 1.99), Node 20.19+ and pnpm, Linux GTK3/WebKitGTK 4.1 development packages, a C toolchain and `dpkg-deb`.

- `pnpm install --frozen-lockfile`
- `pnpm build` — TypeScript checking and production frontend.
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test -p curator-core`
- `python3 tests/test_backups.py` — temporary-library full-copy, WAL consistency, corruption, concurrent mutation, symlink, insufficient-space and copy-failure checks.
- `/usr/bin/python3 tests/ui_webkit.py` — exercises the production frontend bundle in WebKit with fictional IPC fixtures. Tests simple navigation, source-level verification, preview invalidation, provider-result preparation, selection persistence and one-confirmation combined apply. Requires Python GI and a WebKit2 introspection namespace (4.1 or 4.0) plus a graphical display; does not access Calibre or use mouse/OCR automation. Run `pnpm build` first.
- `python3 tests/window_smoke.py` — actual native window; requires X11, xdotool, ImageMagick and tesseract. Creates a disposable library and verifies rule editing, approval, field review, the real production-lock refusal, history and an unchanged database checksum. Screenshots stay in its printed temporary directory. OCR avoids relying on an unavailable desktop accessibility bus.
- `python3 tests/window_smoke.py --ai-workflow` — explicit native AI test, requiring local Ollama and installed `qwen3.5:4b`. Uses fictional source text only, enables AI in a disposable workspace, checks excerpt selection and inference, and verifies draft-only or insufficient-evidence behavior with an unchanged database checksum. Leave mouse/keyboard idle during native automation. The ordinary native test also checks model discovery, but does not request inference.
- `pnpm package` — locked release builds, resource copy and `.deb`.

The package script directly builds Tauri's custom protocol and stages standard Debian files. It does not use the Tauri CLI's unconditional manifest watcher, which can fail even in production builds when the machine's inotify watch quota is exhausted. No system limits are changed.

The Calibre integration test creates only temporary libraries. It tests actual API apply/undo, stale review refusal, protection, a genuine bridge process exit followed by recovery, lock contention and refusal after intervening comments edits. Linux Calibre singleton names are global per user: a test-only namespace isolates the disposable fixture without changing the production bridge's locking policy. The test validates its exact temporary target before isolation. Calibre uses optimized Python; safety checks never rely on Python `assert` side effects.

Combined-repair tests exercise series, subseries, Comments, publisher and identifier changes in one journal, complete guarded undo, interrupted recovery, populated non-target preservation and stale publisher refusal. Document tests cover global numbering across unowned books, explicit independent subseries indices, all four structured slots, partial-field preservation, duplicate/ambiguous matches, conflicting rules, source content changes and verification revocation. DuckDuckGo fixtures cover structured/HTML parsing, redirect-link decoding, challenge detection, unsafe links, deduplication and cache isolation.

Automated research regressions cover prequels, independent nested numbers, publication/recommended/partial list rejection, fictional dates, scope mismatch, conflicts, missing/fabricated citations, same-cited-paragraph completion, relative directions/stale anchors, cached model protocol and research-to-Apply with full backup/refusal/Revert on a disposable Calibre library. The WebKit test also covers research selection and the full-backup/Revert UI.

Opt-in checks: `cargo test -p curator-core --test chronology public_ -- --ignored --nocapture` performs bounded public source research without library access. `real_local_research_extraction` in that test file uses the configured default installed local model with fictional cached evidence only and checks both main and nested numbers; it does not download models or contact an external source.

The opt-in `cargo test -p curator-core --test research public_author_discovery -- --ignored --nocapture` queries the public name Larry Niven without library data. `selected_local_document_read_only` in the documents tests reads only an explicitly selected `CURATOR_TEST_DOCUMENT`; it neither imports private contents into the repository nor accesses a Calibre library.

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

Synopsis regressions include bounded fake providers and actual disposable Calibre Comments fill/undo/interruption/stale-edit tests. The opt-in `cargo test -p curator-core --test metadata real_local_synopsis -- --ignored --nocapture` uses a fictional provider description. `real_metadata_provider` in that test file explicitly queries one public Google book result without accessing a library.

## Troubleshooting
- **No compatible columns:** create three series-type custom columns in Calibre, then rediscover their labels in Settings.
- **Calibre lock refusal:** close the GUI and content server normally. Do not kill jobs or bypass the lock.
- **Stale values / conflicting references:** rescan and inspect existing rules. Do not manually modify an operation journal to force it through.
- **Undo/recovery refused:** retain all backups and journals; a newer edit or uncheckpointed write requires manual review, not a blind database restore.
- **Ollama unavailable / model missing:** check the origin and `ollama list`, then refresh installed models. No automatic model download occurs.
- **AI citation/schema error:** use a clearer, smaller excerpt or another compatible local model. Nothing is approved or imported from the rejected response.
- **Provider missing/disabled/covers-only:** rediscover providers and choose an enabled identify provider; configure accounts and enablement in Calibre rather than entering credentials into Curator.
- **Wrong book or different edition returned:** leave it unconfirmed. Verify the record's identity/identifiers or choose another source; a successful fetch is not a valid match by itself.
- **No description or insufficient evidence:** try another provider or obtain an authoritative description. Do not ask the model to invent a plot from the title.
- **Synopsis apply refuses Comments:** existing stored Comments are protected, including nonempty HTML. If another program edited the record after drafting, rescan and review again rather than changing the journal.
- **Desktop launcher fails after moving the checkout:** rebuild and rerun `python3 scripts/install_launcher.py`, or install the Debian package.
