# RustyClip Launcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert RustyClip into a tray-first clipboard launcher with a centered transparent search panel, immediate history refresh, stable file-list dedupe, and lighter icons.

**Architecture:** The Rust/Tauri side becomes responsible for tray lifecycle, global shortcut registration, launcher window visibility, and clipboard-history update events. The React frontend becomes a compact launcher surface with a focused search box and filtered live-updating results. Icon assets are revised from a lighter SVG master and regenerated for Tauri bundle targets.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, SQLite via `tauri-plugin-sql`, Tauri tray/global-shortcut APIs, existing clipboard watcher

---

## File Structure

### New files

- `src/constants/history.ts`
  Responsibility: frontend launcher constants such as event names and shortcut display strings.

### Modified files

- `src-tauri/src/clipboard_history.rs`
  Responsibility: stable file-list normalization and history-updated event emission.
- `src-tauri/src/lib.rs`
  Responsibility: tray setup, shortcut registration, launcher toggle wiring, and startup visibility behavior.
- `src-tauri/tauri.conf.json`
  Responsibility: launcher window configuration, transparent/frameless properties, and app behavior settings.
- `src/App.tsx`
  Responsibility: replace page-style history UI with compact launcher surface and live event refresh.
- `src/App.css`
  Responsibility: launcher visual system, compact search results layout, and transparent panel styling.
- `src/assets/rustyclip-logo.svg`
  Responsibility: lighter master brand/icon artwork used in the frontend.
- `public/rustyclip-logo.svg`
  Responsibility: public-facing copy of the lighter master artwork.
- `public/tauri.svg`
  Responsibility: launcher-consistent replacement asset.
- `public/vite.svg`
  Responsibility: launcher-consistent replacement asset.
- `src/assets/react.svg`
  Responsibility: launcher-consistent replacement asset.
- `src-tauri/icons/*`
  Responsibility: regenerated Tauri bundle icons without the heavy dark border feel.

### Verification targets

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run build`

## Task 1: Fix backend event and dedupe foundations

**Files:**

- Modify: `src-tauri/src/clipboard_history.rs`

- [ ] **Step 1: Add stable file-list normalization before hashing**

Update the file-list normalization flow so hashing and storage use a stable ordering:

```rust
fn normalize_file_paths(paths: Vec<PathBuf>) -> Vec<String> {
    let mut normalized_paths = paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    normalized_paths.sort();
    normalized_paths.dedup();
    normalized_paths
}
```

- [ ] **Step 2: Add a history-updated event constant and emit helper**

In `src-tauri/src/clipboard_history.rs`, add:

```rust
pub const HISTORY_UPDATED_EVENT: &str = "clipboard-history-updated";

fn emit_history_updated(app: &AppHandle) -> Result<(), String> {
    app.emit(HISTORY_UPDATED_EVENT, ()).map_err(|err| err.to_string())
}
```

- [ ] **Step 3: Emit the event after successful history mutations**

Update backend mutation paths so the event fires after:

- successful `upsert_history_item`
- successful `delete_history_item`
- successful `clear_history_items`

Use:

```rust
emit_history_updated(app)?;
```

- [ ] **Step 4: Add regression tests for stable file-list dedupe**

Extend the backend tests in `src-tauri/src/clipboard_history.rs` with a direct normalization test:

```rust
#[test]
fn normalize_file_paths_sorts_and_deduplicates() {
    let paths = vec![
        PathBuf::from("/tmp/b.txt"),
        PathBuf::from("/tmp/a.txt"),
        PathBuf::from("/tmp/a.txt"),
    ];

    let normalized = normalize_file_paths(paths);

    assert_eq!(
        normalized,
        vec!["/tmp/a.txt".to_string(), "/tmp/b.txt".to_string()]
    );
}
```

- [ ] **Step 5: Run Rust tests to verify green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml clipboard_history::tests`

Expected: PASS with the new dedupe/event-related tests included.

- [ ] **Step 6: Run Cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/clipboard_history.rs
git commit -m "fix: emit clipboard history updates"
```

## Task 2: Convert the app into a tray-first launcher

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Define launcher window behavior in Tauri config**

Update the existing main window configuration in `src-tauri/tauri.conf.json` to launcher-oriented properties:

```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "RustyClip",
        "width": 760,
        "height": 560,
        "visible": false,
        "transparent": true,
        "decorations": false,
        "resizable": false,
        "alwaysOnTop": true,
        "center": true,
        "skipTaskbar": true,
        "focus": false
      }
    ]
  }
}
```

- [ ] **Step 2: Add tray menu creation in Rust**

Update `src-tauri/src/lib.rs` to build a tray with `显示面板` and `退出`:

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
```

Create menu items and a tray icon in setup.

- [ ] **Step 3: Add launcher toggle helpers**

In `src-tauri/src/lib.rs`, add helper functions similar to:

```rust
fn show_launcher(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("main window not found")?;
    window.center().map_err(|err| err.to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

fn hide_launcher(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("main window not found")?;
    window.hide().map_err(|err| err.to_string())
}
```

- [ ] **Step 4: Register the global shortcut**

Wire startup shortcut registration in `src-tauri/src/lib.rs` for:

- macOS: `Command+Shift+V`
- Windows: `Ctrl+Shift+V`

Implementation should log failures but not crash the app.

- [ ] **Step 5: Hook tray and shortcut actions into launcher visibility**

Implement behavior:

- tray `显示面板` shows launcher
- tray `退出` exits app
- shortcut toggles show/hide
- tray icon click may optionally also show launcher if consistent with minimal UX

- [ ] **Step 6: Hide launcher on blur**

Hook the window event so loss of focus hides the launcher:

```rust
window.on_window_event(|event| {
    if let tauri::WindowEvent::Focused(false) = event {
        let _ = window.hide();
    }
});
```

Adapt to current Tauri 2 API shape as needed.

- [ ] **Step 7: Run Cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS with tray and shortcut wiring compiled.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat: add tray-first launcher shell"
```

## Task 3: Rebuild the frontend as a compact launcher

**Files:**

- Create: `src/constants/history.ts`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

- [ ] **Step 1: Add frontend constants for event and shortcut display**

Create `src/constants/history.ts`:

```ts
export const HISTORY_UPDATED_EVENT = "clipboard-history-updated";
export const SHORTCUT_LABEL = navigator.userAgent.includes("Mac")
  ? "Command + Shift + V"
  : "Ctrl + Shift + V";
```

- [ ] **Step 2: Replace the page layout with launcher structure**

Rewrite `src/App.tsx` around:

- focused search input
- derived filtered list
- compact empty state
- compact delete/clear actions

Keep existing backend command usage, but remove large branding header and count-card layout.

- [ ] **Step 3: Add live event subscription**

In `src/App.tsx`, subscribe to the backend history-updated event:

```tsx
import { listen } from "@tauri-apps/api/event";
```

Then in `useEffect`:

```tsx
useEffect(() => {
  let unlisten: null | (() => void) = null;

  void listen(HISTORY_UPDATED_EVENT, () => {
    void loadHistory("refreshing", "刷新失败");
  }).then((cleanup) => {
    unlisten = cleanup;
  });

  return () => {
    unlisten?.();
  };
}, []);
```

- [ ] **Step 4: Add search filtering**

In `src/App.tsx`, add local filtering:

```tsx
const [query, setQuery] = useState("");

const normalizedQuery = query.trim().toLowerCase();
const filteredHistory = normalizedQuery
  ? history.filter((item) => {
      const searchSpace = [
        item.textContent ?? "",
        item.imagePath ?? "",
        ...item.filePaths,
      ]
        .join("\n")
        .toLowerCase();

      return searchSpace.includes(normalizedQuery);
    })
  : history;
```

- [ ] **Step 5: Focus the search input on mount and launcher re-open**

Add a search input ref and focus it on initial load. Also add a lightweight focus fallback whenever the page becomes visible:

```tsx
const searchInputRef = useRef<HTMLInputElement | null>(null);

useEffect(() => {
  searchInputRef.current?.focus();
}, []);
```

Then add `visibilitychange` or window focus handling to refocus.

- [ ] **Step 6: Restyle the UI into a transparent command-palette launcher**

Update `src/App.css` to:

- remove the large panel hero look
- use a centered translucent launcher surface
- make search input the main anchor
- tighten list spacing and row height
- keep image thumbnails compact
- preserve readability on transparency

The resulting structure should feel closer to Alfred/Spotlight than to a dashboard.

- [ ] **Step 7: Run frontend build**

Run: `npm run build`

Expected: PASS with no TypeScript or Vite errors.

- [ ] **Step 8: Commit**

```bash
git add src/constants/history.ts src/App.tsx src/App.css
git commit -m "feat: add launcher-style clipboard search ui"
```

## Task 4: Lighten icon assets and regenerate Tauri icons

**Files:**

- Modify: `src/assets/rustyclip-logo.svg`
- Modify: `public/rustyclip-logo.svg`
- Modify: `public/tauri.svg`
- Modify: `public/vite.svg`
- Modify: `src/assets/react.svg`
- Modify: `src-tauri/icons/*`

- [ ] **Step 1: Redraw the master icon without the heavy dark border feel**

Update the SVG master artwork so:

- the edge reads lighter
- the background frame is less visually heavy
- the clipboard/history metaphor remains intact

Do this first in:

- `src/assets/rustyclip-logo.svg`
- `public/rustyclip-logo.svg`

- [ ] **Step 2: Sync replacement SVG assets used by the frontend**

Copy or adapt the revised artwork into:

- `public/tauri.svg`
- `public/vite.svg`
- `src/assets/react.svg`

These should all match the lighter icon language.

- [ ] **Step 3: Regenerate Tauri bundle icons from the revised master**

Run:

```bash
npx tauri icon public/rustyclip-logo.svg -o src-tauri/icons
```

Expected: icon assets regenerate successfully, including `png`, `ico`, and `icns`.

- [ ] **Step 4: Run frontend build**

Run: `npm run build`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/assets/rustyclip-logo.svg public/rustyclip-logo.svg public/tauri.svg public/vite.svg src/assets/react.svg src-tauri/icons
git commit -m "feat: refine launcher icon assets"
```

## Task 5: Final verification and behavior check

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/clipboard_history.rs`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

- [ ] **Step 1: Run full Rust verification**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 2: Run full Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 3: Run frontend verification**

Run: `npm run build`

Expected: PASS.

- [ ] **Step 4: Manual runtime checklist**

Launch the app and verify:

```text
1. App starts without showing a regular dashboard window.
2. Tray icon appears.
3. Command/Ctrl + Shift + V opens a centered transparent launcher.
4. The search input is focused automatically.
5. Esc hides the launcher.
6. Copying text updates the visible list immediately.
7. Copying an image updates the visible list immediately.
8. Copying files updates the visible list immediately.
9. Copying the same file set in a different order refreshes one existing row instead of adding a duplicate.
10. Icon no longer appears to have a heavy black border.
```

- [ ] **Step 5: Fix only concrete defects found in verification**

If manual or automated verification reveals issues, make the smallest targeted changes needed. Avoid unrelated refactors during this pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/clipboard_history.rs src/App.tsx src/App.css src-tauri/tauri.conf.json
git commit -m "fix: finalize tray launcher behavior"
```

## Self-Review

### Spec coverage

- Tray-first behavior: covered by Task 2.
- Global shortcut centered launcher: covered by Task 2.
- Transparent minimal UI with search and compact list: covered by Task 3.
- Immediate list refresh after copy: covered by Task 1 event emission and Task 3 event subscription.
- Stable file-list dedupe: covered by Task 1.
- Lighter icon without black border feel: covered by Task 4.
- Full validation including runtime behavior: covered by Task 5.

### Placeholder scan

No `TODO`, `TBD`, or vague “handle appropriately” placeholders remain. Where crate API details may vary, the commands and target behavior are still concrete.

### Type consistency

- Backend event name is centralized and reused consistently.
- Shortcut label and behavior are aligned with the spec.
- Clipboard history ordering remains `updated_at DESC`.
