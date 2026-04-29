# RustyClip Launcher UI Design

## Overview

This document defines the next product step for RustyClip after clipboard capture and persistence are in place.

The product shape is changing from a visible desktop page into a tray-first clipboard launcher similar to Spotlight or Alfred.

The target scope is:

- Tray-first application behavior
- No regular always-visible main window
- Global shortcut to open a centered transparent launcher
- Minimal interface with a search box and history list
- Immediate list refresh after new clipboard records are stored
- Cleaner icon treatment without heavy black border feeling

## Goals

- Make RustyClip feel like a lightweight background productivity tool
- Open the history UI instantly with `Command/Ctrl + Shift + V`
- Present clipboard history in a minimal, search-first interface
- Refresh the launcher list immediately when new clipboard data is recorded
- Preserve existing clipboard persistence behavior for text, images, and file paths

## Non-Goals

- Full settings UI in this iteration
- Multi-window application shell
- Rich keyboard action system beyond basic navigation and close behavior
- Advanced fuzzy ranking comparable to full Alfred workflows
- Editable shortcut management in UI

## Product Model

RustyClip should behave as a tray-resident launcher, not as a normal windowed application.

### Startup Behavior

- App launches into tray mode
- No regular content window is shown by default
- Background clipboard watcher starts immediately
- Global shortcut is registered at startup

### Primary Surface

The primary UI is a launcher panel:

- Centered on screen
- Transparent window background
- Frameless
- Always on top while visible
- Hidden by default

### Window Lifecycle

The launcher is shown when:

- User presses `Command/Ctrl + Shift + V`
- User selects `显示面板` from tray menu

The launcher is hidden when:

- User presses `Esc`
- User presses the global shortcut again
- Launcher loses focus

The app exits only through tray menu `退出`.

## Interaction Design

### Visual Direction

The UI should be notably simpler than the current hero-card page.

Principles:

- Minimal chrome
- Tight spacing
- High information density without looking crowded
- Neutral, polished surface
- Search field as the visual anchor
- No oversized branding block in the launcher itself

This should feel closer to a desktop command palette than a landing page.

### Layout

The launcher contains two regions:

1. Search field
2. Results list

Structure:

- Top: single search input
- Bottom: scrollable clipboard history list

No extra marketing copy, feature chips, or large counters should remain in the launcher.

### Search Field

The search input is focused automatically whenever the launcher opens.

Behavior:

- Placeholder text explains that users can search clipboard history
- Search filters the currently loaded records in real time
- Search applies to:
  - text content
  - file paths
  - image path metadata when present

This iteration uses client-side filtering only.

### History List

Items are ordered by `updated_at DESC`.

Each item should be visually compact:

- Text item: concise text preview
- Image item: compact thumbnail with minimal metadata
- File-list item: primary emphasis on filename, with path shown secondarily

The selected or hovered row should be clear, but the overall look should stay restrained.

### Refresh Behavior

The launcher list must refresh quickly after new clipboard content is captured.

This should not rely on manual page refresh.

Target behavior:

- Backend emits an event after clipboard history is successfully inserted or updated
- Frontend listens for the event while mounted
- If launcher is visible, its list updates immediately
- If launcher is hidden, the latest state is loaded when it opens

## Tray Design

Tray is the default application anchor.

Tray menu items for this iteration:

- `显示面板`
- `退出`

Optional future tray items such as pause recording or settings are out of scope.

## Shortcut Design

Default shortcut:

- macOS: `Command + Shift + V`
- Windows: `Ctrl + Shift + V`

The implementation should share one logical shortcut mapping with platform-specific modifier behavior as needed by Tauri.

## Technical Architecture

### Tauri Window Model

Use a dedicated launcher-style window configuration.

Expected properties:

- hidden on startup
- transparent
- decorations disabled
- resizable disabled or tightly constrained
- always on top while visible
- centered when shown
- focusable

The existing visible page implementation should be repurposed into the launcher instead of keeping a separate dashboard window.

### Global Shortcut

Rust side should:

- register the global shortcut at startup
- toggle launcher visibility
- focus the search field path when shown

If the shortcut cannot be registered, the app should still run and log the error rather than crash.

### Event Refresh

Rust clipboard persistence flow should emit a Tauri event after successful upsert:

- event name should be specific to clipboard history updates

Frontend should subscribe and refresh local list state.

### Existing Persistence Fix

One known backend issue must be corrected in this implementation:

- file-list duplicate detection should normalize to a stable ordering before hashing

Without this, copying the same file set in a different order incorrectly creates multiple entries.

For this iteration, normalize file paths by stable sorting before hashing and storage.

## Icon Direction

The current icon treatment feels too heavy around the edge.

The next icon revision should:

- remove the apparent black border feeling
- keep the clipboard/history metaphor
- retain a clean, modern desktop-app look
- work well at small tray and app-icon sizes

The icon should feel lighter and more neutral, with cleaner edge handling.

## Frontend Scope

Frontend work in this iteration includes:

- replace hero-style history page with launcher-style surface
- add focused search box
- add compact results list
- subscribe to backend refresh events
- keep delete and clear capabilities available if they fit the compact layout

Delete and clear may move into lighter-weight controls, but they should remain accessible.

## Backend Scope

Backend work in this iteration includes:

- tray setup
- launcher window behavior
- global shortcut registration
- show/hide toggle logic
- history-updated event emission
- stable file-list normalization for dedupe

The existing clipboard watcher and SQLite schema should remain the persistence foundation.

## Validation Plan

Minimum validation for this iteration:

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run build`
- Launch app and confirm no normal dashboard window appears
- Confirm tray icon appears
- Confirm `Command/Ctrl + Shift + V` opens centered transparent launcher
- Confirm launcher hides on `Esc`
- Confirm copying new text updates visible list immediately
- Confirm copying same file set in different order updates existing row instead of inserting a new one
- Confirm icon no longer shows heavy black border feeling

## Risks

- Global shortcut and transparent window behavior may vary slightly across macOS and Windows
- Tray-first apps require careful hide/focus behavior to avoid flicker
- Transparent launcher visuals can become unreadable if contrast is too subtle

These are acceptable implementation risks for this iteration because they are core to the desired product shape and should be solved now rather than deferred.
