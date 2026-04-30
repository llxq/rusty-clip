use std::{
    borrow::Cow,
    fs,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use arboard::{Clipboard, ImageData};
use chrono::{SecondsFormat, Utc};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
#[cfg(target_os = "macos")]
use objc2_app_kit::NSPasteboard;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Row, Sqlite};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_sql::{DbInstances, DbPool, Migration, MigrationKind};
use tokio::sync::{Mutex, OwnedMutexGuard};
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;

pub const DB_URL: &str = "sqlite:rusty-clip.db";
pub const HISTORY_UPDATED_EVENT: &str = "clipboard-history-updated";

const CONTENT_TYPE_FILE_LIST: &str = "file_list";
const CONTENT_TYPE_IMAGE: &str = "image";
const CONTENT_TYPE_TEXT: &str = "text";
const IMAGE_DIR_NAME: &str = "clipboard-images";
const POLL_INTERVAL_MS: u64 = 150;

#[derive(Debug)]
enum EClipboardPayload {
    Text {
        content_hash: String,
        text_content: String,
    },
    Image {
        content_hash: String,
        png_bytes: Vec<u8>,
    },
    FileList {
        content_hash: String,
        file_paths: Vec<String>,
    },
}

#[derive(Debug)]
enum EClipboardWritePayload {
    Text {
        text_content: String,
    },
    Image {
        rgba_bytes: Vec<u8>,
        width: usize,
        height: usize,
    },
    FileList {
        file_paths: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IClipboardHistoryItem {
    id: i64,
    content_type: String,
    text_content: Option<String>,
    image_path: Option<String>,
    file_paths: Vec<String>,
    is_pinned: bool,
    is_favorite: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Default)]
struct IWatcherState {
    last_content_hash: Option<String>,
    last_raw_signature: Option<String>,
}

#[derive(Clone, Default)]
struct IClipboardMutationLock(Arc<Mutex<()>>);

#[derive(Default)]
struct IClipboardContents {
    file_paths: Option<Vec<String>>,
    image_data: Option<ImageData<'static>>,
    text: Option<String>,
}

impl IWatcherState {
    fn should_skip_upsert(&self, content_hash: &str, raw_signature: Option<&str>) -> bool {
        if self.last_content_hash.as_deref() != Some(content_hash) {
            return false;
        }

        match raw_signature {
            Some(raw_signature) => self.last_raw_signature.as_deref() == Some(raw_signature),
            None => true,
        }
    }

    fn record_processed(&mut self, content_hash: &str, raw_signature: Option<&str>) {
        self.last_content_hash = Some(content_hash.to_string());
        self.last_raw_signature = raw_signature.map(str::to_string);
    }

    fn record_observed_signature(&mut self, raw_signature: Option<&str>) {
        self.last_content_hash = None;
        self.last_raw_signature = raw_signature.map(str::to_string);
    }
}

pub fn migrations() -> Vec<Migration> {
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
    }, Migration {
        version: 2,
        description: "add pinned and favorite flags to clipboard_history",
        sql: r#"
            ALTER TABLE clipboard_history ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE clipboard_history ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0;
            CREATE INDEX IF NOT EXISTS idx_clipboard_history_pinned_updated_at
            ON clipboard_history(is_pinned DESC, updated_at DESC);
        "#,
        kind: MigrationKind::Up,
    }]
}

pub fn setup(app: AppHandle) -> Result<(), String> {
    if app.try_state::<IClipboardMutationLock>().is_none() {
        app.manage(IClipboardMutationLock::default());
    }
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

#[tauri::command]
pub async fn copy_clipboard_history(app: AppHandle, id: i64) -> Result<(), String> {
    copy_history_item_to_clipboard(&app, id).await
}

#[tauri::command]
pub async fn paste_clipboard_history(app: AppHandle, id: i64) -> Result<(), String> {
    paste_history_item(&app, id).await
}

#[tauri::command]
pub async fn toggle_pin_clipboard_history(app: AppHandle, id: i64) -> Result<(), String> {
    toggle_history_item_flag(&app, id, "is_pinned").await
}

#[tauri::command]
pub async fn toggle_favorite_clipboard_history(app: AppHandle, id: i64) -> Result<(), String> {
    toggle_history_item_flag(&app, id, "is_favorite").await
}

fn start_clipboard_watcher(app: AppHandle) {
    let _watcher = thread::spawn(move || {
        let mut state = IWatcherState::default();

        loop {
            if let Err(err) = poll_once(&app, &mut state) {
                eprintln!("clipboard watcher error: {err}");
            }

            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}

fn poll_once(app: &AppHandle, state: &mut IWatcherState) -> Result<(), String> {
    let raw_signature = read_raw_clipboard_signature();
    let Some(payload) = read_clipboard_payload()? else {
        state.record_observed_signature(raw_signature.as_deref());
        return Ok(());
    };

    let content_hash = payload.content_hash().to_string();
    if is_launcher_visible(app) {
        state.record_processed(&content_hash, raw_signature.as_deref());
        return Ok(());
    }

    if state.should_skip_upsert(&content_hash, raw_signature.as_deref()) {
        return Ok(());
    }

    tauri::async_runtime::block_on(upsert_history_item(app, payload))?;
    state.record_processed(&content_hash, raw_signature.as_deref());

    Ok(())
}

fn is_launcher_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

fn read_raw_clipboard_signature() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        return Some(NSPasteboard::generalPasteboard().changeCount().to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let sequence_number = unsafe { GetClipboardSequenceNumber() };
        return (sequence_number != 0).then(|| sequence_number.to_string());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn read_clipboard_payload() -> Result<Option<EClipboardPayload>, String> {
    let mut clipboard = Clipboard::new().map_err(|err| err.to_string())?;
    let file_paths = clipboard.get().file_list().ok().map(normalize_file_paths);
    let image_data = clipboard.get_image().ok();
    let text = clipboard.get_text().ok().map(|value| normalize_text(&value));

    build_clipboard_payload(IClipboardContents {
        file_paths,
        image_data,
        text,
    })
}

fn build_clipboard_payload(contents: IClipboardContents) -> Result<Option<EClipboardPayload>, String> {
    if let Some(file_paths) = contents.file_paths {
        if !file_paths.is_empty() {
            let file_paths_json = serde_json::to_vec(&file_paths).map_err(|err| err.to_string())?;
            return Ok(Some(EClipboardPayload::FileList {
                content_hash: hash_payload(CONTENT_TYPE_FILE_LIST, &file_paths_json),
                file_paths,
            }));
        }
    }

    if let Some(image_data) = contents.image_data {
        let content_hash = hash_payload(CONTENT_TYPE_IMAGE, image_data.bytes.as_ref());
        let png_bytes = encode_png(image_data)?;

        return Ok(Some(EClipboardPayload::Image {
            content_hash,
            png_bytes,
        }));
    }

    if let Some(text_content) = contents.text {
        return Ok(Some(EClipboardPayload::Text {
            content_hash: hash_payload(CONTENT_TYPE_TEXT, text_content.as_bytes()),
            text_content,
        }));
    }

    Ok(None)
}

fn normalize_text(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn normalize_file_paths(paths: Vec<PathBuf>) -> Vec<String> {
    let mut normalized_paths = paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    normalized_paths.sort();
    normalized_paths.dedup();
    normalized_paths
}

fn build_clipboard_write_payload(
    item: &IClipboardHistoryItem,
) -> Result<EClipboardWritePayload, String> {
    match item.content_type.as_str() {
        CONTENT_TYPE_TEXT => Ok(EClipboardWritePayload::Text {
            text_content: item.text_content.clone().unwrap_or_default(),
        }),
        CONTENT_TYPE_FILE_LIST => Ok(EClipboardWritePayload::FileList {
            file_paths: item.file_paths.clone(),
        }),
        CONTENT_TYPE_IMAGE => {
            let image_path = item
                .image_path
                .as_ref()
                .ok_or_else(|| "image path is missing".to_string())?;
            let png_bytes = fs::read(image_path).map_err(|err| err.to_string())?;
            let rgba_image = image::load_from_memory(&png_bytes)
                .map_err(|err| err.to_string())?
                .into_rgba8();
            let (width, height) = rgba_image.dimensions();

            Ok(EClipboardWritePayload::Image {
                rgba_bytes: rgba_image.into_raw(),
                width: width as usize,
                height: height as usize,
            })
        }
        other => Err(format!("unsupported clipboard content type: {other}")),
    }
}

fn hash_payload(content_type: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn encode_png(image_data: ImageData<'static>) -> Result<Vec<u8>, String> {
    let width = u32::try_from(image_data.width).map_err(|err| err.to_string())?;
    let height = u32::try_from(image_data.height).map_err(|err| err.to_string())?;
    let mut png_bytes = Vec::new();

    PngEncoder::new(&mut png_bytes)
        .write_image(
            image_data.bytes.as_ref(),
            width,
            height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|err| err.to_string())?;

    Ok(png_bytes)
}

fn ensure_image_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join(IMAGE_DIR_NAME);

    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

    Ok(dir)
}

fn persist_image(app: &AppHandle, content_hash: &str, png_bytes: &[u8]) -> Result<String, String> {
    let path = ensure_image_dir(app)?.join(format!("{content_hash}.png"));
    fs::write(&path, png_bytes).map_err(|err| err.to_string())?;
    Ok(path_to_string(&path))
}

fn remove_image_file(image_path: &str) -> Result<(), String> {
    match fs::remove_file(image_path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn emit_history_updated(app: &AppHandle) -> Result<(), String> {
    app.emit(HISTORY_UPDATED_EVENT, ())
        .map_err(|err| err.to_string())
}

async fn get_sqlite_pool(app: &AppHandle) -> Result<Pool<Sqlite>, String> {
    let Some(instances) = app.try_state::<DbInstances>() else {
        return Err("sql plugin state is not available".to_string());
    };

    let instances = instances.0.read().await;
    let Some(pool) = instances.get(DB_URL) else {
        return Err(format!("sqlite pool is not preloaded for {DB_URL}"));
    };

    let DbPool::Sqlite(pool) = pool;
    Ok(pool.clone())
}

async fn acquire_mutation_lock(
    app: &AppHandle,
) -> Result<OwnedMutexGuard<()>, String> {
    let Some(lock) = app.try_state::<IClipboardMutationLock>() else {
        return Err("clipboard mutation lock is not available".to_string());
    };

    Ok(lock.0.clone().lock_owned().await)
}

async fn upsert_history_item(app: &AppHandle, payload: EClipboardPayload) -> Result<(), String> {
    let _mutation_lock = acquire_mutation_lock(app).await?;
    let pool = get_sqlite_pool(app).await?;
    let now = now_timestamp();

    let (content_type, text_content, image_path, file_paths_json, content_hash) = match payload {
        EClipboardPayload::Text {
            content_hash,
            text_content,
        } => (
            CONTENT_TYPE_TEXT.to_string(),
            Some(text_content),
            None,
            None,
            content_hash,
        ),
        EClipboardPayload::Image {
            content_hash,
            png_bytes,
        } => (
            CONTENT_TYPE_IMAGE.to_string(),
            None,
            Some(persist_image(app, &content_hash, &png_bytes)?),
            None,
            content_hash,
        ),
        EClipboardPayload::FileList {
            content_hash,
            file_paths,
        } => (
            CONTENT_TYPE_FILE_LIST.to_string(),
            None,
            None,
            Some(serde_json::to_string(&file_paths).map_err(|err| err.to_string())?),
            content_hash,
        ),
    };

    sqlx::query(
        r#"
        INSERT INTO clipboard_history (
            content_type,
            text_content,
            image_path,
            file_paths_json,
            content_hash,
            created_at,
            updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(content_hash) DO UPDATE SET
            updated_at = excluded.updated_at
        "#,
    )
    .bind(content_type)
    .bind(text_content)
    .bind(image_path)
    .bind(file_paths_json)
    .bind(content_hash)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;

    emit_history_updated(app)?;

    Ok(())
}

async fn load_history(app: &AppHandle) -> Result<Vec<IClipboardHistoryItem>, String> {
    let pool = get_sqlite_pool(app).await?;
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            content_type,
            text_content,
            image_path,
            file_paths_json,
            is_pinned,
            is_favorite,
            created_at,
            updated_at
        FROM clipboard_history
        ORDER BY is_pinned DESC, updated_at DESC, id DESC
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| err.to_string())?;

    rows.into_iter()
        .map(|row| {
            let file_paths_json = row
                .try_get::<Option<String>, _>("file_paths_json")
                .map_err(|err| err.to_string())?;
            let file_paths = parse_file_paths(file_paths_json)?;

            Ok(IClipboardHistoryItem {
                id: row.try_get("id").map_err(|err| err.to_string())?,
                content_type: row
                    .try_get("content_type")
                    .map_err(|err| err.to_string())?,
                text_content: row
                    .try_get("text_content")
                    .map_err(|err| err.to_string())?,
                image_path: row.try_get("image_path").map_err(|err| err.to_string())?,
                file_paths,
                is_pinned: read_bool_column(&row, "is_pinned")?,
                is_favorite: read_bool_column(&row, "is_favorite")?,
                created_at: row.try_get("created_at").map_err(|err| err.to_string())?,
                updated_at: row.try_get("updated_at").map_err(|err| err.to_string())?,
            })
        })
        .collect()
}

async fn load_history_item(app: &AppHandle, id: i64) -> Result<IClipboardHistoryItem, String> {
    let pool = get_sqlite_pool(app).await?;
    let row = sqlx::query(
        r#"
        SELECT
            id,
            content_type,
            text_content,
            image_path,
            file_paths_json,
            is_pinned,
            is_favorite,
            created_at,
            updated_at
        FROM clipboard_history
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|err| err.to_string())?
    .ok_or_else(|| format!("clipboard history item {id} not found"))?;

    let file_paths_json = row
        .try_get::<Option<String>, _>("file_paths_json")
        .map_err(|err| err.to_string())?;
    let file_paths = parse_file_paths(file_paths_json)?;

    Ok(IClipboardHistoryItem {
        id: row.try_get("id").map_err(|err| err.to_string())?,
        content_type: row
            .try_get("content_type")
            .map_err(|err| err.to_string())?,
        text_content: row
            .try_get("text_content")
            .map_err(|err| err.to_string())?,
        image_path: row.try_get("image_path").map_err(|err| err.to_string())?,
        file_paths,
        is_pinned: read_bool_column(&row, "is_pinned")?,
        is_favorite: read_bool_column(&row, "is_favorite")?,
        created_at: row.try_get("created_at").map_err(|err| err.to_string())?,
        updated_at: row.try_get("updated_at").map_err(|err| err.to_string())?,
    })
}

async fn copy_history_item_to_clipboard(app: &AppHandle, id: i64) -> Result<(), String> {
    let item = load_history_item(app, id).await?;
    let payload = build_clipboard_write_payload(&item)?;
    let mut clipboard = Clipboard::new().map_err(|err| err.to_string())?;

    match payload {
        EClipboardWritePayload::Text { text_content } => {
            clipboard.set_text(text_content).map_err(|err| err.to_string())?
        }
        EClipboardWritePayload::Image {
            rgba_bytes,
            width,
            height,
        } => clipboard
            .set_image(ImageData {
                width,
                height,
                bytes: Cow::Owned(rgba_bytes),
            })
            .map_err(|err| err.to_string())?,
        EClipboardWritePayload::FileList { file_paths } => {
            let paths = file_paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
            clipboard
                .set()
                .file_list(&paths)
                .map_err(|err| err.to_string())?
        }
    }

    Ok(())
}

pub async fn write_history_item_to_clipboard(app: &AppHandle, id: i64) -> Result<(), String> {
    copy_history_item_to_clipboard(app, id).await
}

async fn paste_history_item(app: &AppHandle, id: i64) -> Result<(), String> {
    copy_history_item_to_clipboard(app, id).await?;
    simulate_paste_shortcut()
}

#[cfg(target_os = "macos")]
fn simulate_paste_shortcut() -> Result<(), String> {
    let status = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to keystroke "v" using command down"#,
        ])
        .status()
        .map_err(|err| format!("failed to execute paste shortcut: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("paste shortcut exited with status {status}"))
    }
}

#[cfg(target_os = "windows")]
fn simulate_paste_shortcut() -> Result<(), String> {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')"#,
        ])
        .status()
        .map_err(|err| format!("failed to execute paste shortcut: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("paste shortcut exited with status {status}"))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn simulate_paste_shortcut() -> Result<(), String> {
    Err("automatic paste is not supported on this platform yet".to_string())
}

async fn delete_history_item(app: &AppHandle, id: i64) -> Result<(), String> {
    let _mutation_lock = acquire_mutation_lock(app).await?;
    let pool = get_sqlite_pool(app).await?;
    let image_path = sqlx::query("SELECT image_path FROM clipboard_history WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|err| err.to_string())?
        .and_then(|row| row.try_get::<Option<String>, _>("image_path").ok())
        .flatten();

    sqlx::query("DELETE FROM clipboard_history WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if let Some(image_path) = image_path {
        if let Err(err) = remove_image_file(&image_path) {
            eprintln!("failed to remove clipboard image {image_path}: {err}");
        }
    }

    emit_history_updated(app)?;

    Ok(())
}

async fn clear_history_items(app: &AppHandle) -> Result<(), String> {
    let _mutation_lock = acquire_mutation_lock(app).await?;
    let pool = get_sqlite_pool(app).await?;
    let rows = sqlx::query(
        "SELECT image_path FROM clipboard_history WHERE is_favorite = 0 AND image_path IS NOT NULL",
    )
        .fetch_all(&pool)
        .await
        .map_err(|err| err.to_string())?;
    let image_paths = rows
        .into_iter()
        .filter_map(|row| row.try_get::<Option<String>, _>("image_path").ok().flatten())
        .collect::<Vec<_>>();

    sqlx::query("DELETE FROM clipboard_history WHERE is_favorite = 0")
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    for image_path in image_paths {
        if let Err(err) = remove_image_file(&image_path) {
            eprintln!("failed to remove clipboard image {image_path}: {err}");
        }
    }

    emit_history_updated(app)?;

    Ok(())
}

fn parse_file_paths(file_paths_json: Option<String>) -> Result<Vec<String>, String> {
    match file_paths_json {
        Some(file_paths_json) => {
            serde_json::from_str::<Vec<String>>(&file_paths_json).map_err(|err| err.to_string())
        }
        None => Ok(Vec::new()),
    }
}

fn read_bool_column(row: &sqlx::sqlite::SqliteRow, column_name: &str) -> Result<bool, String> {
    let value = row
        .try_get::<i64, _>(column_name)
        .map_err(|err| err.to_string())?;
    Ok(value != 0)
}

async fn toggle_history_item_flag(
    app: &AppHandle,
    id: i64,
    column_name: &str,
) -> Result<(), String> {
    let _mutation_lock = acquire_mutation_lock(app).await?;
    let pool = get_sqlite_pool(app).await?;
    let sql = format!(
        "UPDATE clipboard_history SET {column_name} = CASE {column_name} WHEN 0 THEN 1 ELSE 0 END WHERE id = ?"
    );

    let result = sqlx::query(&sql)
        .bind(id)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if result.rows_affected() == 0 {
        return Err(format!("clipboard history item {id} not found"));
    }

    emit_history_updated(app)?;

    Ok(())
}

impl EClipboardPayload {
    fn content_hash(&self) -> &str {
        match self {
            Self::Text { content_hash, .. } => content_hash,
            Self::Image { content_hash, .. } => content_hash,
            Self::FileList { content_hash, .. } => content_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        build_clipboard_payload, build_clipboard_write_payload, normalize_file_paths,
        IClipboardHistoryItem, IClipboardContents,
        IClipboardMutationLock, IWatcherState, POLL_INTERVAL_MS,
    };

    #[test]
    fn poll_interval_is_tuned_for_responsive_launcher_refresh() {
        assert_eq!(POLL_INTERVAL_MS, 150);
    }

    #[test]
    fn watcher_skips_only_when_hash_and_raw_signature_match() {
        let mut state = IWatcherState::default();
        state.record_processed("hash-a", Some("sig-1"));

        assert!(state.should_skip_upsert("hash-a", Some("sig-1")));
        assert!(!state.should_skip_upsert("hash-a", Some("sig-2")));
    }

    #[test]
    fn watcher_processes_same_hash_when_signature_changes_after_unsupported_content() {
        let mut state = IWatcherState::default();
        state.record_processed("hash-a", Some("sig-1"));

        assert!(!state.should_skip_upsert("hash-a", Some("sig-empty")));
    }

    #[test]
    fn watcher_falls_back_to_hash_only_when_raw_signature_is_unavailable() {
        let mut state = IWatcherState::default();
        state.record_processed("hash-a", None);

        assert!(state.should_skip_upsert("hash-a", None));
        assert!(!state.should_skip_upsert("hash-b", None));
    }

    #[test]
    fn clipboard_payload_prefers_file_list_over_text() {
        let payload = build_clipboard_payload(IClipboardContents {
            file_paths: Some(vec!["/tmp/demo.txt".to_string()]),
            image_data: None,
            text: Some("/tmp/demo.txt".to_string()),
        })
        .expect("payload should build");

        match payload {
            Some(super::EClipboardPayload::FileList { file_paths, .. }) => {
                assert_eq!(file_paths, vec!["/tmp/demo.txt".to_string()]);
            }
            _ => panic!("expected file list payload"),
        }
    }

    #[test]
    fn clipboard_payload_prefers_image_over_text() {
        let payload = build_clipboard_payload(IClipboardContents {
            file_paths: None,
            image_data: Some(arboard::ImageData {
                width: 1,
                height: 1,
                bytes: std::borrow::Cow::Owned(vec![0, 0, 0, 255]),
            }),
            text: Some("plain text".to_string()),
        })
        .expect("payload should build");

        match payload {
            Some(super::EClipboardPayload::Image { .. }) => {}
            _ => panic!("expected image payload"),
        }
    }

    #[test]
    fn clipboard_write_payload_uses_text_content() {
        let item = IClipboardHistoryItem {
            id: 1,
            content_type: "text".to_string(),
            text_content: Some("hello".to_string()),
            image_path: None,
            file_paths: Vec::new(),
            is_pinned: false,
            is_favorite: false,
            created_at: "2026-04-30T00:00:00.000Z".to_string(),
            updated_at: "2026-04-30T00:00:00.000Z".to_string(),
        };

        let payload = build_clipboard_write_payload(&item).expect("payload should build");

        match payload {
            super::EClipboardWritePayload::Text { text_content } => {
                assert_eq!(text_content, "hello".to_string());
            }
            _ => panic!("expected text payload"),
        }
    }

    #[test]
    fn clipboard_write_payload_uses_file_paths() {
        let item = IClipboardHistoryItem {
            id: 1,
            content_type: "file_list".to_string(),
            text_content: None,
            image_path: None,
            file_paths: vec!["/tmp/a.txt".to_string(), "/tmp/b.txt".to_string()],
            is_pinned: false,
            is_favorite: false,
            created_at: "2026-04-30T00:00:00.000Z".to_string(),
            updated_at: "2026-04-30T00:00:00.000Z".to_string(),
        };

        let payload = build_clipboard_write_payload(&item).expect("payload should build");

        match payload {
            super::EClipboardWritePayload::FileList { file_paths } => {
                assert_eq!(
                    file_paths,
                    vec!["/tmp/a.txt".to_string(), "/tmp/b.txt".to_string()]
                );
            }
            _ => panic!("expected file list payload"),
        }
    }

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

    #[test]
    fn clipboard_mutation_lock_blocks_concurrent_access() {
        let lock = IClipboardMutationLock::default();
        let first_guard = tauri::async_runtime::block_on(lock.0.clone().lock_owned());

        assert!(lock.0.try_lock().is_err());

        drop(first_guard);

        assert!(lock.0.try_lock().is_ok());
    }
}
