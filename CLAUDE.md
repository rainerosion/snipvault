# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
npm run dev          # Frontend dev server (vite, port 1420)
npm run build        # Frontend production build (outputs to dist/)
npm run tauri dev    # Full Tauri dev (frontend + Rust backend)
npm run tauri build  # Production build: outputs .exe, .msi, NSIS installer
```

For faster iteration on frontend-only changes: run `npm run build` then `npm run tauri dev` to avoid rebuilding Rust every time.

## Architecture

### Stack
- **Tauri 2** (Rust backend + WebView frontend)
- **React 19 + TypeScript** (frontend UI)
- **CodeMirror 6** (`@uiw/react-codemirror`) for code editing
- **rusqlite** (bundled SQLite) for local data persistence
- **reqwest** (blocking HTTP) for WebDAV sync

### Backend (`src-tauri/src/`)
| File | Purpose |
|------|---------|
| `main.rs` | Entry point; registers commands, sets up tray, handles window close → minimize-to-tray |
| `lib.rs` | Module root (re-exports all modules) |
| `db.rs` | SQLite CRUD; `with_db()` helper; snippet schema is flattened (tags as JSON string) |
| `settings.rs` | In-memory Settings struct + JSON file persistence; `init_settings()` called at startup |
| `paths.rs` | Portable vs installed mode detection via registry + exe path; all data dirs derived here |
| `webdav.rs` | Blocking HTTP WebDAV upload/download for snippet backup |
| `commands.rs` | All `#[tauri::command]` functions exposed to the frontend via IPC |

**Data storage locations** (controlled by `paths.rs`):
- Portable: `%LOCALAPPDATA%/SnipVault/`
- Installed (MSI/NSIS registry detected): `<exe_dir>/data/`
- Files: `snippets.db`, `settings.json`

### Frontend (`src/`)
| File | Purpose |
|------|---------|
| `App.tsx` | Root component; manages `settingsOpen` state to show settings as a modal overlay; handles all main-app state, keyboard shortcuts (Ctrl+N/S/E) |
| `main.tsx` | `ThemeProvider` + `LanguageProvider` contexts; `data-theme` attribute set on `#root` |
| `i18n/index.ts` | react-i18next config; exports `LANGUAGES` list |
| `context/LanguageContext.tsx` | React context for current UI language (`zh`/`en`) |
| `components/SnippetEditor.tsx` | CodeMirror 6 editor with per-theme `HighlightStyle` + `EditorView.theme` for selection/cursor/scroller |
| `components/Settings.tsx` | Settings form panel; language selector uses `settings.language` (not `i18n.language`) |
| `components/Titlebar.tsx` | Custom frameless titlebar with drag-to-move and min/max/close buttons |
| `components/Dialog.tsx` | Modal dialog with `alert()`, `confirm()`, `ask()` (save/discard/cancel) promise-based APIs |
| `hooks/useSnippets.ts` | Frontend state + Tauri IPC for snippet CRUD |
| `hooks/useSettings.ts` | Frontend state + Tauri IPC for settings and sync |
| `utils/languages.ts` | Language config: dot colors, display names, CodeMirror extension mapping |

## Critical Gotchas

### CodeMirror shadow DOM isolation
CodeMirror 6 uses shadow DOM — **CSS selectors outside `.cm-editor` cannot reach token elements inside the shadow root**. All editor styles (syntax highlight colors, selection background, cursor, scrollbar, gutters) are set via CodeMirror JS APIs:
- **Syntax highlight colors**: `HighlightStyle.define()` + `syntaxHighlighting` (GitHub dark/light token palettes)
- **Selection/cursor/scroller**: `EditorView.theme()` extension — this is the ONLY way to inject styles into shadow DOM from React
- **CSS in `index.css`**: Only handles `.cm-editor-wrap` outer container sizing, NOT any styles inside the editor

### Editor scrolling (CodeMirror 6 specific)
CodeMirror 6 handles scrolling internally via `EditorView` — external wheel event interception is NOT needed and can interfere. The `.cm-scroller` has `overflow-y: auto` set via `EditorView.theme()`. Do NOT add `overflow: hidden` to `.cm-editor-wrap` or any ancestor of the CodeMirror host element, as this blocks the internal scroll reference. The correct structure is:
```
.cm-editor-wrap (display:flex, flex:1, min-height:0, overflow:hidden)
  .cm-editor (host element — height:100%)
    shadow DOM
      .cm-scroller (overflow:auto, flex:1) ← scrolling happens here
```

### `data-theme` attribute placement
The `data-theme` attribute must be on `#root` (not `.app` or any inner div) so that it reaches both the main DOM tree and CodeMirror's shadow DOM. Both `ThemeProvider` in `main.tsx` and `App.tsx` sync it via `useEffect`.

### Settings modal
Settings are rendered as an in-window modal overlay (`settings-overlay` class) in `App.tsx`, not a separate OS window. This avoids Tauri 2 multi-window complexity.

### Theme context initialization
`ThemeContext` defaults to `"dark"` to avoid flash-of-wrong-theme. The actual persisted theme is loaded from settings via `useSettings`; the context is the runtime state used by all components.

### Language settings persistence
The `language` field in `Settings` is saved via `save_settings` in `commands.rs`. The backend uses `*s = new_settings.clone()` (full replacement) so any new fields are automatically persisted. `LanguageContext` provides runtime state; the persisted value in `settings.json` is the source of truth for the next startup.

## Capabilities
Permissions for the `main` window are declared in `src-tauri/capabilities/default.json`. If adding new Tauri APIs (e.g., shell, fs, notification), add the corresponding permission string there.
