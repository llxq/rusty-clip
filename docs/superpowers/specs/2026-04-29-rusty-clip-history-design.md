# RustyClip Clipboard History Design

## Overview

This document defines the first implementation of RustyClip clipboard history for the current Tauri 2 desktop app.

The target scope is:

- Cross-platform desktop support for macOS and Windows
- Clipboard history capture for text, images, and file paths
- Persistent storage in SQLite
- Duplicate handling by moving existing content to the top instead of inserting a new row
- Minimal frontend history viewer backed by Tauri commands

## Goals

- Persist clipboard history locally in SQLite
- Capture clipboard changes while the app is running
- Normalize text, image, and file-list clipboard payloads into a single history table
- Keep history ordering stable by sorting on most recently updated entries
- Expose read and delete capabilities to the frontend

## Non-Goals

- Linux support in this iteration
- Rich search, tags, pinning, favorites, or grouping
- Cloud sync
- Clipboard write-back and paste automation
- OCR, image metadata extraction, or file previews beyond basic path display

## Architecture

The implementation is split into three layers:

1. Clipboard watcher in Rust
2. SQLite persistence via Tauri SQL plugin
3. Frontend history viewer using Tauri commands

### Clipboard Watcher

The watcher runs in Rust background code after app startup.

The watcher uses a cross-platform polling loop instead of platform-specific OS clipboard event APIs.

Reasoning:

- It works on both macOS and Windows with one main flow
- It avoids separate event implementations per platform
- It is enough for the first version of clipboard history

The watcher loop behavior:

- Poll clipboard content on a fixed interval
- Detect whether the current clipboard payload differs from the last processed payload
- Normalize clipboard data into an internal record shape
- Compute a stable content hash
- Upsert the record into SQLite

Initial polling interval:

- `800ms`

This value is a balance between responsiveness and resource usage. It can be tuned later without schema changes.

## Clipboard Payload Support

### Text

Capture plain text clipboard content as UTF-8 string data.

Stored fields:

- `content_type = text`
- `text_content`
- `content_hash`

### Images

Capture clipboard image payloads from the native clipboard.

Storage flow:

- Read image bytes from clipboard
- Encode and save to a file under the app data directory
- Store the saved file path in SQLite
- Compute the content hash from image bytes before file write

Stored fields:

- `content_type = image`
- `image_path`
- `content_hash`

### File Paths

Capture copied file lists from the clipboard.

Storage flow:

- Read file list entries from clipboard
- Normalize them into a stable ordered array of absolute path strings
- Serialize the array as JSON for SQLite storage
- Compute the content hash from the normalized path list

Stored fields:

- `content_type = file_list`
- `file_paths_json`
- `content_hash`

## Persistence

SQLite is used as the source of truth for history.

The implementation will use the Tauri SQL plugin with SQLite backend.

Database location:

- App-local data directory
- Database filename: `rusty-clip.db`

### Schema

Table: `clipboard_history`

Columns:

- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `content_type TEXT NOT NULL`
- `text_content TEXT`
- `image_path TEXT`
- `file_paths_json TEXT`
- `content_hash TEXT NOT NULL UNIQUE`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

Indexes:

- Unique index on `content_hash`
- Sort index on `updated_at DESC`

### Duplicate Handling

Duplicate behavior follows the approved product rule:

- If clipboard content is new, insert a new row
- If clipboard content already exists, do not insert a new row
- Update the existing row `updated_at` to the current timestamp
- Keep `created_at` unchanged

This makes an existing record move to the top when queried by `updated_at DESC`.

For images and file lists, duplicate detection uses normalized content hashing:

- Text: hash of normalized text bytes
- Image: hash of raw image bytes
- File list: hash of normalized JSON payload

## Data Model

Rust-side enum:

- `text`
- `image`
- `file_list`

Frontend item shape:

- `id`
- `contentType`
- `textContent`
- `imagePath`
- `filePaths`
- `createdAt`
- `updatedAt`

## Tauri Commands

The frontend will not talk to SQLite directly in this iteration. Rust commands will be the application boundary.

Commands:

- `list_clipboard_history`
- `delete_clipboard_history`
- `clear_clipboard_history`

### list_clipboard_history

Returns history rows ordered by `updated_at DESC`.

Optional future pagination is out of scope for this first version.

### delete_clipboard_history

Deletes a single history row by `id`.

If the row is an image item and `image_path` exists, remove the image file as part of deletion when possible.

### clear_clipboard_history

Deletes all rows.

If stored image files exist, attempt to remove them during cleanup.

## Frontend

The current landing page will be replaced by a minimal usable history list.

Initial behavior:

- Load history on page mount
- Render text items with truncated preview
- Render image items using the saved local image path
- Render file-list items as path rows
- Provide delete and clear actions

The frontend is not responsible for clipboard polling or direct SQL access.

## Error Handling

### Clipboard Read Failures

If a clipboard read fails during polling:

- Skip the current tick
- Keep the watcher alive
- Do not clear previous in-memory state

### File Write Failures

If image persistence fails:

- Skip insertion for that clipboard event
- Log the error
- Continue watcher loop

### Database Failures

If an insert or update fails:

- Log the error
- Keep the watcher alive
- Do not crash the app

Frontend command failures should surface a readable error string.

## Security and Privacy

This feature stores clipboard history locally on disk, including text snippets, images, and file paths.

This iteration does not include:

- Sensitive-data filtering
- Incognito mode
- Per-app exclusion
- Encryption at rest

These are explicit future improvements, not implicit guarantees.

## Dependencies

Expected Rust dependencies:

- `tauri-plugin-sql`
- Clipboard access crate for text, image, and file-list reads
- `serde`
- `serde_json`
- Hashing crate such as `sha2`
- Time crate for timestamps

The exact clipboard crate may vary depending on the best compatibility path for macOS and Windows, but it must support text, image, and file-list access from Rust.

## Open Implementation Decisions

The following implementation details are intentionally left flexible for coding:

- Exact clipboard crate selection after compatibility verification
- Exact timestamp format, as long as it sorts correctly and is consistent
- Exact image encoding format on disk, with PNG preferred

These are not product ambiguities because they do not affect external behavior in this version.

## Validation Plan

Minimum validation for this implementation:

- Build frontend with `npm run build`
- Build Rust app with `cargo check`
- Verify database file creation
- Verify text copy creates one row
- Verify repeated text copy refreshes one existing row instead of inserting duplicates
- Verify image copy creates file plus row
- Verify file copy creates row with serialized paths
- Verify list, delete, and clear commands work from UI

## Risks

- Cross-platform clipboard crates may expose different behavior for file-list payloads
- Polling may miss extremely short-lived clipboard transitions
- Image storage can grow quickly without retention limits

These are acceptable for the first version because the goal is a functional cross-platform history recorder, not a fully optimized production release.
