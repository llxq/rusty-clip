# RustyClip Clipboard History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build cross-platform clipboard history for macOS and Windows that captures text, images, and file paths, and persists deduplicated history records in SQLite.

**Architecture:** The Rust backend owns clipboard polling, payload normalization, hashing, SQLite writes, and Tauri commands. The React frontend becomes a minimal history viewer that calls Rust commands for listing and deletion. SQLite is accessed through the Tauri SQL plugin, while copied images are stored as PNG files in the app data directory and referenced from the database.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, `tauri-plugin-sql`, clipboard crate, `sha2`, `chrono`, `image`

---

## File Structure

### New files

- `src-tauri/src/clipboard_history.rs`
  Responsibility: clipboard polling, payload normalization, hashing, image persistence, database upsert, and Tauri commands.
- `src-tauri/permissions/sql.json`
  Responsibility: SQL plugin permission grant for the desktop window if required by plugin setup.
- `src/types/clipboard.ts`
  Responsibility: shared frontend TypeScript types for clipboard history items.

### Modified files

- `src-tauri/Cargo.toml`
  Responsibility: add SQL, hashing, image, and clipboard dependencies.
- `src-tauri/src/lib.rs`
  Responsibility: initialize SQL plugin, start watcher, and register history commands.
- `src-tauri/capabilities/default.json`
  Responsibility: allow SQL plugin capability if plugin requires explicit permission.
- `src/App.tsx`
  Responsibility: replace static hero page with history list UI.
- `src/App.css`
  Responsibility: style the history list, empty state, image preview, and action buttons.

### Verification targets

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `npm run build`

## Task 1: Add Rust dependencies and app wiring

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Add Rust dependencies**

Update `src-tauri/Cargo.toml` to include the SQL plugin, hashing, timestamps, image encoding, and clipboard access:

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-sql = { version = "2", features = ["sqlite"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }
image = { version = "0.25", default-features = false, features = ["png"] }
arboard = "3"
```

- [ ] **Step 2: Run Cargo metadata check to verify dependency resolution**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: dependency graph resolves, compile will still fail because new module and command symbols do not exist yet.

- [ ] **Step 3: Wire SQL plugin and placeholder module into app startup**

Update `src-tauri/src/lib.rs` so the builder loads the SQL plugin and prepares to register clipboard history commands:

```rust
mod clipboard_history;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .setup(|app| {
            clipboard_history::setup(app.handle().clone())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            clipboard_history::list_clipboard_history,
            clipboard_history::delete_clipboard_history,
            clipboard_history::clear_clipboard_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Add plugin permission if needed by desktop capability**

Update `src-tauri/capabilities/default.json` to allow SQL plugin access:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default",
    "sql:default"
  ]
}
```

- [ ] **Step 5: Run Cargo check to verify the app now only fails on missing module implementation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: compile reaches `clipboard_history` module references and fails only because that module does not yet define the required functions.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "chore: wire sql plugin for clipboard history"
```

## Task 2: Implement clipboard history backend and SQLite persistence

**Files:**

- Create: `src-tauri/src/clipboard_history.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write the backend module with data types and setup skeleton**

Create `src-tauri/src/clipboard_history.rs` with the core types and public entry points:

```rust
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use arboard::Clipboard;
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tauri_plugin_sql::{Migration, MigrationKind};

const DB_URL: &str = "sqlite:rusty-clip.db";
const POLL_INTERVAL_MS: u64 = 800;

#[derive(Clone, Debug)]
enum EClipboardPayload {
    Text(String),
    Image { bytes: Vec<u8> },
    FileList(Vec<String>),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IClipboardHistoryItem {
    id: i64,
    content_type: String,
    text_content: Option<String>,
    image_path: Option<String>,
    file_paths: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Default)]
struct IWatcherState {
    last_hash: Option<String>,
}

pub fn setup(app: AppHandle) -> Result<(), String> {
    run_migrations(&app)?;
    start_clipboard_watcher(app);
    Ok(())
}

#[tauri::command]
pub async fn list_clipboard_history(app: AppHandle) -> Result<Vec<IClipboardHistoryItem>, String> {
    load_history(&app).await
}

#[tauri::command]
pub async fn delete_clipboard_history(app: AppHandle, id: i64) -> Result<(), String> {
    delete_history_item(&app, id).await
}

#[tauri::command]
pub async fn clear_clipboard_history(app: AppHandle) -> Result<(), String> {
    clear_history_items(&app).await
}
```

- [ ] **Step 2: Add SQLite migration and query helpers**

In the same file, add migration setup and list query logic:

```rust
fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "create clipboard_history table",
        sql: r#"
            CREATE TABLE IF NOT EXISTS clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL,
                text_content TEXT,
                image_path TEXT,
                file_paths_json TEXT,
                content_hash TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_history_updated_at
            ON clipboard_history(updated_at DESC);
        "#,
        kind: MigrationKind::Up,
    }]
}

fn run_migrations(app: &AppHandle) -> Result<(), String> {
    let db = tauri_plugin_sql::Builder::default()
        .add_migrations(DB_URL, migrations())
        .build();
    app.plugin(db).map_err(|err| err.to_string())
}
```

Add async query logic that returns rows ordered by `updated_at DESC` and deserializes `file_paths_json` into `Vec<String>`.

- [ ] **Step 3: Implement hashing and payload normalization helpers**

Add helpers for deterministic deduplication:

```rust
fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalize_text(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn hash_file_list(paths: &[String]) -> String {
    let json = serde_json::to_vec(paths).unwrap_or_default();
    hash_bytes(&json)
}
```

- [ ] **Step 4: Implement image persistence and app data path helpers**

Add helper functions to write images into an app-owned directory:

```rust
fn ensure_image_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join("clipboard-images");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn persist_image(app: &AppHandle, hash: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = ensure_image_dir(app)?;
    let path = dir.join(format!("{hash}.png"));
    fs::write(&path, bytes).map_err(|err| err.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
```

- [ ] **Step 5: Implement polling watcher with duplicate upsert behavior**

Add a watcher that polls every `800ms`, skips unchanged hashes, and upserts on duplicate:

```rust
fn start_clipboard_watcher(app: AppHandle) {
    let state = Arc::new(Mutex::new(IWatcherState::default()));
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(err) = poll_once(&app, &state).await {
                eprintln!("clipboard watcher error: {err}");
            }
            tauri::async_runtime::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    });
}
```

`poll_once` must:

- open clipboard
- try text first
- try image second
- try file-list third
- compute hash
- compare to `last_hash`
- run SQL:

```sql
INSERT INTO clipboard_history (
  content_type, text_content, image_path, file_paths_json, content_hash, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(content_hash) DO UPDATE SET
  updated_at = excluded.updated_at;
```

- [ ] **Step 6: Run Cargo check to verify backend compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: Rust backend compiles cleanly or reports only concrete API mismatches from the chosen clipboard or SQL crate, which must be fixed before moving on.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/clipboard_history.rs src-tauri/src/lib.rs
git commit -m "feat: add clipboard watcher and sqlite persistence"
```

## Task 3: Build the frontend history viewer

**Files:**

- Create: `src/types/clipboard.ts`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

- [ ] **Step 1: Add frontend clipboard item types**

Create `src/types/clipboard.ts`:

```ts
export type TClipboardContentType = "text" | "image" | "file_list";

export interface IClipboardHistoryItem {
  id: number;
  contentType: TClipboardContentType;
  textContent: string | null;
  imagePath: string | null;
  filePaths: string[];
  createdAt: string;
  updatedAt: string;
}
```

- [ ] **Step 2: Replace the static landing page with command-driven history UI**

Update `src/App.tsx` to load, delete, and clear history via Tauri commands:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import type { IClipboardHistoryItem } from "./types/clipboard";

const App = (): JSX.Element => {
  const [history, setHistory] = useState<IClipboardHistoryItem[]>([]);
  const [error, setError] = useState<string>("");

  const loadHistory = async (): Promise<void> => {
    try {
      const items = await invoke<IClipboardHistoryItem[]>("list_clipboard_history");
      setHistory(items);
      setError("");
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    void loadHistory();
  }, []);

  return (
    <main className="container">
      <section className="panel">
        <header className="panel-header">
          <div>
            <p className="eyebrow">Rust Powered Clipboard History</p>
            <h1>RustyClip</h1>
          </div>
          <button type="button" onClick={() => void invoke("clear_clipboard_history").then(loadHistory)}>
            Clear All
          </button>
        </header>
      </section>
    </main>
  );
};

export default App;
```

Then expand the JSX with:

- empty state when `history.length === 0`
- text item rendering
- image item `<img src={convertFileSrc(item.imagePath)} />`
- file path list rendering
- delete button per item

- [ ] **Step 3: Add history list styling**

Update `src/App.css` to replace hero layout styles with scrollable history styles:

```css
.panel {
  width: min(1040px, 100%);
  min-height: 80vh;
  padding: 32px;
  border-radius: 32px;
  background: rgba(9, 20, 27, 0.78);
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.history-item {
  padding: 18px;
  border: 1px solid rgba(125, 218, 230, 0.12);
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.04);
}
```

Add classes for:

- `.panel-header`
- `.history-meta`
- `.history-preview`
- `.history-image`
- `.history-file-list`
- `.empty-state`
- `.error-banner`

- [ ] **Step 4: Run frontend build**

Run: `npm run build`

Expected: Vite build passes with no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add src/types/clipboard.ts src/App.tsx src/App.css
git commit -m "feat: add clipboard history viewer"
```

## Task 4: End-to-end verification and cleanup

**Files:**

- Modify: `src-tauri/src/clipboard_history.rs`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

- [ ] **Step 1: Run full Rust verification**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: exit code `0`.

- [ ] **Step 2: Run full frontend verification**

Run: `npm run build`

Expected: exit code `0`.

- [ ] **Step 3: Manual validation checklist**

Run the desktop app and verify:

```text
1. Copy a text string and confirm one text row appears.
2. Copy the same text again and confirm no new row is added; the existing row moves to the top.
3. Copy an image and confirm one image row appears and the file is written under the app data directory.
4. Copy one or more files in Finder or Explorer and confirm a file-list row appears.
5. Delete one row and confirm it disappears.
6. Clear all rows and confirm the list resets to empty state.
```

- [ ] **Step 4: Fix any concrete validation defects only**

If a verification step fails, return to the exact failing file and make the smallest change required. Do not refactor unrelated code during this pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/clipboard_history.rs src/App.tsx src/App.css
git commit -m "fix: finalize clipboard history flow"
```

## Self-Review

### Spec coverage

- Cross-platform scope: covered by Rust-side polling watcher in Task 2.
- Text, image, and file-list capture: covered by Task 2 payload normalization and persistence.
- SQLite persistence: covered by Task 1 plugin wiring and Task 2 schema plus upsert.
- Duplicate up-top behavior: covered by `content_hash` uniqueness and `updated_at` refresh in Task 2.
- Frontend history list, delete, and clear: covered by Task 3.
- Validation plan: covered by Task 4.

### Placeholder scan

No `TODO`, `TBD`, or “similar to previous task” placeholders remain. Remaining flexibility is limited to concrete crate API adjustments during implementation.

### Type consistency

- Rust payload values map to frontend `contentType`.
- Backend commands are consistently named `list_clipboard_history`, `delete_clipboard_history`, and `clear_clipboard_history`.
- SQLite order is consistently `updated_at DESC`.
