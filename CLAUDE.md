# CLAUDE.md

This file provides repository-wide guidance to Claude Code when working on SnipVault.

## Documentation Gate (Required)

Every feature design and feature implementation must include a documentation impact assessment and update or add the relevant development documentation in the same task/change.

- Architecture, module boundaries, data flow, lifecycle, IPC, persistence, or permissions → update `docs/architecture.md`.
- User-visible behavior, interactions, shortcuts, configuration, or feature semantics → update `docs/feature-design.md`.
- Development commands, extension patterns, or verification workflow → update `docs/development.md`.
- New limitations/risks, or a fix for a documented issue → update or remove the corresponding entry in `docs/known-limitations.md`.
- Installation, platform support, storage paths, or user configuration → update `README.md` where applicable.
- Data-flow or module changes must also update the relevant Mermaid diagram.
- Do not describe unimplemented designs as current behavior.
- Before completion, verify source links and explicitly report either the documentation changes or `Documentation impact: none` with a reason.
- A feature task is not complete while required documentation remains unsynchronized.

The documentation index is `docs/README.md`. It is the detailed source of project design information; this file keeps only high-value working constraints.

## Build Commands

```bash
npm run dev                  # Frontend Vite server, port 1420
npm run build                # Frontend production build, outputs dist/
npm run typecheck            # TypeScript no-emit check
npm run test                 # Frontend tests in watch mode
npm run test:run             # Frontend tests once
npm run lint                 # ESLint for TypeScript and React
npm run format:check         # Prettier check for maintained tooling/config files
npm run docs:check           # Markdown relative-link and anchor check
npm run versions:check       # npm/Cargo/Tauri/Vite UI version and release-tag consistency check
npm run icons                # Regenerate Tauri icons from assets/app-icon.png
npm run icons:check          # Verify canonical icon source, generated icon formats and duplicate cleanup
npm run tauri:dev            # Full Tauri development mode (starts Vite automatically)
npm run tauri:build          # Production application/package build
npm run tauri:build:debug    # Debug application/package build
npm run tauri:info           # Tauri environment information
```

Equivalent npm argument-forwarding forms such as `npm run tauri dev` also work.

`tauri dev` is configured by `src-tauri/tauri.conf.json` to run `npm run dev` and load `http://localhost:1420`. Running `npm run build` first does not make Tauri dev reuse `dist/` or avoid its normal Rust build.

The repository has Rust unit tests (including one ignored, opt-in benchmark) plus Vitest/React Testing Library/user-event/axe frontend tests. Ordinary push/PR CI runs the documented frontend, Rust, documentation-link, and version-consistency gates. Coverage remains limited; see `docs/development.md` and `docs/known-limitations.md` for the exact checks and gaps rather than maintaining counts here.

## Architecture

### Stack

- **Tauri 2** — desktop runtime, IPC, window, tray, plugins
- **React 19 + TypeScript + Vite** — frontend UI
- **CodeMirror 6** (`@uiw/react-codemirror`) — code editor
- **rusqlite** bundled SQLite — local persistence
- **reqwest blocking client** — WebDAV HTTP

Detailed architecture: `docs/architecture.md`

Current feature behavior: `docs/feature-design.md`

Known defects and limitations: `docs/known-limitations.md`

### Backend (`src-tauri/src/`)

| File | Purpose |
|---|---|
| `main.rs` | Tauri entry point, plugins, single instance, window lifecycle, worker startup and command registration |
| `sync.rs` | Typed synchronization completion events and automatic-sync scheduling/backoff |
| `tray.rs` | Tray ownership, menu construction/refresh, handlers and window reveal |
| `lib.rs` | Declares the currently active modules |
| `commands.rs` | Registered `#[tauri::command]` IPC adapters, settings/credential/autostart transaction, controlled opens and startup markers |
| `credentials.rs` | Injectable OS credential-store boundary and test-only memory fake |
| `db.rs` | Current real `Snippet`/`SnippetSummary` protocol models, SQLite v4 sequential migration/backup/recovery, strict decoding, unchanged v2 FTS/query pagination, revision heads/tombstones/durable revision objects/bounded outbox/remote state, atomic CRUD/import, active v2 synchronization seams and sync history |
| `settings.rs` | Secret-free persisted `Settings`, redacted DTOs, legacy credential migration, damaged-file recovery and JSON persistence |
| `paths.rs` | Installation detection and data/export path resolution |
| `webdav.rs` | Stable synchronization facade, process lock, settings/credential assembly, production v2 engine invocation and success-marker persistence |
| `webdav/protocol.rs` | Explicit v1/v2 manifest, protocol-marker and immutable-revision DTOs; validation, bounds and safe URL/path construction |
| `webdav/transport.rs` | Injectable transport/clock/retry policy, blocking HTTP/authentication, strong ETag enforcement and conditional PUT |
| `webdav/engine_v2.rs` | Production v2 bootstrap, ancestry reconciliation, immutable publication, manifest CAS, marker activation, exact verification/acknowledgement and total deadline |
| `webdav/engine.rs` | Retained legacy v1 engine/test helper; not the production facade path |
| `webdav/store.rs`, `webdav/error.rs` | Injectable v1/v2 persistence adapters plus structured internal synchronization failures |

Data paths are controlled by `paths.rs`:

- Windows portable/default: `dirs::data_dir()/SnipVault`, normally Roaming `%APPDATA%\SnipVault`.
- Detected Windows installation: prefer `<exe_dir>\data` when writable; otherwise use the roaming fallback.
- macOS/Linux currently follow their platform `dirs::data_dir()/SnipVault` path.
- Files: `snippets.db`, `settings.json`.
- Export: prefer `Downloads/SnipVault`, fallback to `<data_dir>/exports`.

“Portable” is an installation-detection branch; it does not mean data is stored next to the executable.

### Frontend (`src/`)

| File | Purpose |
|---|---|
| `main.tsx` | Shared boot settings, root `SettingsProvider`, `ThemeContext`, splash removal and frontend-ready signals; supplies initial language to the provider |
| `context/LanguageContext.tsx`, `LanguageProvider.tsx`, `i18n/` | Runtime language contract/provider, i18next, HTML lang synchronization, and zh/en resources |
| `boot.ts`, `boot.css` | Vite-managed theme anti-flash and boot splash assets required by CSP |
| `App.tsx` | Root state and orchestration: selection/form/dirty state, backend search/filter pagination, lazy detail and reconciliation, dialogs, shortcuts, settings overlay, events, context menu |
| `types/index.ts` | Frontend full `Snippet`, bounded `SnippetSummary`, query/page, and `SnippetForm` contracts |
| `components/SnippetEditor.tsx` | CodeMirror rendering/highlighting, accessible tags, clipboard, line wrapping and custom Canvas minimap |
| `components/languageExtensions.ts` | Editor-only exhaustive parser-backed, StreamLanguage and plaintext extension factory |
| `components/ModalSurface.tsx` | Shared nested modal stack, topmost keyboard ownership, background isolation and focus restoration |
| `components/Settings.tsx` | In-window settings panel, WebDAV and sync history |
| `components/Titlebar.tsx` | Frameless titlebar and window controls |
| `components/Toolbar.tsx` | Search, filters and top-level actions |
| `components/Sidebar.tsx`, `SnippetList.tsx` | Snippet list and card actions |
| `components/Dialog.tsx` | Promise-based `alert()`, `confirm()`, `ask()` APIs on the shared modal surface |
| `hooks/useSnippets.ts` | Paginated summary IPC, lazy detail, distinct tags, stale-response guards, and separate first-page/load-more state |
| `hooks/useSettings.ts` | Authoritative Settings context/provider, injectable IPC API, synchronization status/history and system preferences |
| `utils/languages.ts` | Selectable language metadata, colors and `LanguageId`; intentionally contains no editor imports |

## Critical Constraints

### CodeMirror DOM, styling, and scrolling

CodeMirror manages its editor DOM, but the current app does **not** rely on a CodeMirror Shadow DOM isolation model. Do not assume outside CSS is categorically unable to reach `.cm-*` nodes.

The current outer structure is:

```text
.cm-editor-split
├── .cm-main-pane
│   └── .snippet-codemirror
└── .minimap-wrap
```

There is no active `.cm-editor-wrap` wrapper in `SnippetEditor.tsx`.

Use existing extension points:

- Syntax colors: `HighlightStyle.define()` + `syntaxHighlighting`.
- Cursor, selection, scroller, content and wrapping: `EditorView.theme()` in `buildMainExtensions()`.
- Language parser/stream/plaintext classification: `getLanguageExtensions()` in `components/languageExtensions.ts`; also update `utils/languages.ts` when selectable languages change. StreamLanguage modes provide syntax highlighting, not full Lezer parser semantics.
- Scrolling: `EditorView.scrollDOM` is the single main scroll source. Do not intercept wheel events.
- MiniMap: reuse the custom `MiniMap`, tokenizer and scroll synchronization. It is not implemented with `@replit/codemirror-minimap`.

Do not add `overflow: hidden` to an ancestor in a way that prevents the CodeMirror scroller from receiving a resolvable height. Keep the Canvas minimap/viewport decorative for assistive technology while CodeMirror remains keyboard-scrollable. Verify editor and minimap behavior in the real Tauri window after layout changes.

### Theme

The runtime effective theme is `dark` or `light`; the persisted preference can also be `system`. Startup resolution uses persisted settings, local cache, and system preference to reduce flashing.

`data-theme` is synchronized to both `document.documentElement` and `#root`. The toolbar toggle changes the effective theme temporarily; the Settings panel is the persistent theme preference entry point.

### Settings

Settings are rendered as an in-window modal layer, not a separate OS window. Settings and nested promise Dialogs must reuse `ModalSurface`; do not add competing document/window Tab or Escape traps. Preserve topmost ownership, deterministic initial focus, background isolation, focus restoration, and the existing Save/Discard/Cancel close guard.

`save_settings` accepts a non-secret `SettingsInput` plus explicit `SecretAction` (`Keep`, `Replace(value)`, or `Clear`) and preserves backend-owned `last_sync_at`. Reads return redacted `SettingsView`, never the persisted secret. The root `SettingsProvider` is the single authoritative React settings state; Settings UI drafts exclude backend-owned/status fields, and dirty drafts must survive external refreshes. `minimize_to_tray` is read on each close request, and the permanent auto-sync worker polls current relevant settings, tracking only `credential_revision` rather than secret material.

Adding a field still requires coordinated updates to:

- Rust persisted `Settings` + `Default` (non-secret fields only)
- Rust `SettingsView` / `SettingsInput` and TypeScript `SettingsView` / `SettingsDraft`
- Settings form/UI and runtime application logic
- Persistence/backward compatibility and recovery
- Documentation

Do not assume saving a new lifecycle field automatically reconfigures Rust behavior. Explicitly choose and implement whether it is read per event, polled by a worker, applied through an event, or startup-only.

WebDAV passwords/API keys/Bearer tokens are stored only through `credentials.rs` using the stable service/account identity. New JSON writes and all read DTOs omit the secret; the password field stays blank unless the user types a replacement. Legacy plaintext is sanitized only after credential-store success. Migration/recovery failures must preserve safe recovery behavior and block credential-backed persistent sync until Replace or Clear. Never log, return, snapshot, or schedule secret material; unit tests must use the in-memory/failing fake and never the developer credential store. Do not fall back to plaintext if keyring/platform support is unavailable.

### Snippet revisions and synchronization

Rust owns ordinary snippet timestamps and revision IDs: create/update/favorite return the authoritative `Snippet`; update must compare the caller's `base_revision_id` against the current head inside the same write transaction. A structured `stale_revision` response may refresh the selected authoritative base revision, but must never overwrite the dirty frontend draft.

SQLite v4 atomically maintains live snippet data, existing FTS triggers, current revision/tombstone head, durable immutable `revision_objects`, and bounded pending outbox for create/update/favorite/delete/import winners. Delete removes the live row/FTS and retains a tombstone. Pending writes are capped at 10,000 rows, 64 MiB total canonical payload, plus a per-payload limit; `outbox_full` must roll back the whole mutation. Outbox rows are acknowledged only by exact revision ID, never by snippet, time, sequence range or snapshot watermark, and acknowledgement must not delete `revision_objects`.

Production WebDAV uses a one-way v1→v2 activation. The active remote layout is `snipvault/protocol-v2.json`, v2 `snipvault/manifest.json`, and immutable `snipvault/objects/<revision_uuid>.json`. Existing v1 per-snippet payload files remain untouched but are ignored after activation; never implement downgrade/dual-write behavior or allow an old client to share an activated directory. Fresh, legacy-v1, interrupted-activation and ready-v2 bootstrap states are explicit. A marker without a manifest, marker with v1 manifest, vault-ID mismatch, or a locally committed v2 remote that disappears/reverts to v1 must hard-stop. A valid v2 manifest without marker is the sole interrupted-activation recovery state: require its strong ETag, verify any known local vault identity, conditionally publish the next generation, then create the marker.

Strong ETag and conditional PUT are mandatory server capabilities. Existing/legacy manifests use `If-Match`; fresh manifest and marker creation use `If-None-Match: *`. Missing/weak/malformed ETags or rejected conditional writes must not fall back to unconditional publication. Preserve immutable-object → conditional-manifest → conditional-marker → exact reread/strong-ETag verification → local exact-commit ordering, the maximum four CAS rounds, and the five-minute overall deadline.

Ancestry determines fast-forward direction. For divergent heads, the already-published remote original wins deterministically. A losing live local branch becomes an idempotent deterministic conflict copy/index; a losing local tombstone has no live payload to copy, but its immutable revision is still published and exactly acknowledged without becoming the manifest head. There is no semantic merge or complete conflict-resolution UI. Tombstones are normal immutable revisions and propagate deletion; tombstones and all other remote revision objects are retained indefinitely, with no current GC/compaction. Outbox acknowledgement is only by exact revision ID and must preserve later edits; remote plan application must not echo revisions to the local outbox. Network I/O must never occur while holding the SQLite mutex.

Immediate `SyncResult`/UI metadata uses `protocol_version` and `manifest_generation`; synchronization history uses `protocol_version` and `generation`. Keep these contracts distinct when changing Rust/TypeScript DTOs. Read `docs/known-limitations.md` before changing synchronization. `sync_upload` and `sync_download` both call the same v2 `sync_merge()` flow.

### SQLite changes

The database is schema v4. Initialization migrates only in sequence `v0→v1→v2→v3→v4`: v1 establishes business tables, v2 establishes strict FTS5 backfill/triggers, v3 adds stable DB device identity, revision/tombstone heads, bounded immutable outbox, remote state/conflict index and extended sync history without changing `snippets` or FTS, and v4 adds durable `revision_objects` for local/remote/conflict ancestry needed after outbox acknowledgement. Every existing on-disk v0/v1/v2/v3 source receives one verified `pre-v4` online backup before migration; any step failure restores and revalidates the original source version. New databases, repeated v4 opens and rejected future versions do not create migration backups.

V2 live rows receive deterministic `legacy-<sha256>` heads during v3 migration and are not bulk-enqueued. V4 backfills `revision_objects` from pending outbox rows, current live heads and tombstones, then future local/remote/conflict revisions write both head/object state while enqueueing only local pending work. Future schema work must add the next sequential migration, generalized backup/recovery semantics, strict decoding and temp-DB checks for every historical source, repeated initialization, future rejection, deterministic backfill, rollback/recovery, identity stability, import/export and WebDAV v1-bootstrap/v2-wire compatibility. Never verify by mutating a user database.

Main-list IPC uses bounded `SnippetSummary` cursor pages ordered by `updated_at + id`; full content is fetched only with `get_snippet(id)`. FTS user input must remain literal rather than raw MATCH syntax, preserve wildcard/CJK substring semantics and safe tokenizer fallback, and never be described as relevance-ranked unless ranking is implemented.

### Language persistence

`Settings.language` is persisted through `save_settings`; `LanguageProvider`, `LanguageContext` and i18next hold runtime state. The provider must also synchronize `document.documentElement.lang` to `zh-CN` or `en`. The persisted value is the next-start source of truth. Keep `zh.json` and `en.json` synchronized for new user-visible or accessible text.

### Tauri capabilities and controlled opens

Permissions for `main` are declared in `src-tauri/capabilities/default.json`. Generic Shell opening is not part of the current boundary: the repository URL is fixed in Rust, and data/export directories are opened only by a backend enum after `paths.rs` derives them. Do not return absolute local paths to the WebView merely to avoid adding Shell scope.

When adding a Tauri plugin/API:

1. Register/install the plugin where necessary.
2. Add only the minimum capability/scope.
3. Review CSP, secret exposure, clipboard, generic opening, network, file and window implications.
4. Prefer fixed Rust constants/enums and backend-derived paths over WebView-provided URLs/paths.
5. Update architecture and security documentation.

`withGlobalTauri` is false. The current CSP permits local scripts and exact IPC/resource schemes, has no `unsafe-eval`, and keeps `style-src 'unsafe-inline'` only for current CodeMirror/React runtime styles. Boot CSS/script live in Vite-managed `src/boot.css` and `src/boot.ts`; do not reintroduce inline executable content or broaden script sources. Do not restore the Shell plugin/dependencies, `shell:allow-open`, broad regex scopes, or unnecessary window/autostart permissions.

## Verification and Reporting

Match verification to the changed layer and report actual outcomes:

```bash
git diff --check
npm run build
# Rust-impacting work, as applicable:
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Run the real Tauri application for user-flow, editor, tray, system API, path, import/export, settings or WebDAV changes when it can be isolated from configured credentials/data. WebDAV automated tests must bind exact loopback, use a dedicated test directory and injected fake store/clock, and cover fresh/legacy/ready bootstrap, ambiguous hard-stops, local-only, remote-only, both-updated, tombstone/deletion, concurrent conflict/CAS, strong/weak/missing ETag, conditional-PUT rejection, partial publication, exact acknowledgement/later edits and deadline states. Never point automated tests at a configured real server or developer credential store. The current `tiny_http` suite covers retained v1 behavior, v2 transport wire semantics, and fresh v2 engine bootstrap/exact acknowledgement; the broader legacy-cutover, hard-stop, CAS-exhaustion, crash-recovery and concurrency matrix remains synthetic/unit coverage. Report that residual gap, real-service testing and production Tauri smoke separately; loopback tests are not substitutes for real-service validation.

Before completing any feature task:

- Review the Documentation Gate above.
- Update affected docs and Mermaid diagrams.
- Check links to source files.
- State exactly which checks ran and whether they passed, failed, or were skipped.
