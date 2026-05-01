mod clipboard_history;

#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(desktop)]
use std::sync::Mutex;
#[cfg(desktop)]
use std::time::Duration;
use tauri_plugin_autostart::MacosLauncher;

#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSPopUpMenuWindowLevel, NSWindow, NSWindowCollectionBehavior,
};
#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    ActivationPolicy, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalRect,
    PhysicalSize, Position, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
#[cfg(desktop)]
use tokio::time::sleep;

#[cfg(desktop)]
const MAIN_WINDOW_LABEL: &str = "main";
#[cfg(desktop)]
const SHOW_PANEL_MENU_ID: &str = "show_panel";
#[cfg(desktop)]
const QUIT_MENU_ID: &str = "quit";
#[cfg(desktop)]
const TRAY_ID: &str = "launcher_tray";
#[cfg(desktop)]
const LAUNCHER_SHOWN_EVENT: &str = "launcher-shown";
#[cfg(desktop)]
const LAUNCHER_WINDOW_WIDTH: f64 = 1180.0;
#[cfg(desktop)]
const LAUNCHER_WINDOW_HEIGHT: f64 = 760.0;

#[cfg(desktop)]
#[derive(Default)]
struct LauncherFocusState {
    previous_app_bundle_id: Mutex<Option<String>>,
}

#[cfg(desktop)]
#[derive(Clone, Copy)]
struct LauncherMonitorBounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

#[cfg(target_os = "macos")]
fn launcher_collection_behavior(
    current: NSWindowCollectionBehavior,
) -> NSWindowCollectionBehavior {
    let mut collection_behavior = current;

    collection_behavior.remove(
        NSWindowCollectionBehavior::Managed
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenPrimary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::FullScreenNone
            | NSWindowCollectionBehavior::FullScreenAllowsTiling
            | NSWindowCollectionBehavior::FullScreenDisallowsTiling,
    );
    collection_behavior.insert(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );

    collection_behavior
}

#[cfg(target_os = "macos")]
fn configure_macos_launcher_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window_ptr = window.ns_window().map_err(|error| error.to_string())? as usize;

    window
        .run_on_main_thread(move || unsafe {
            let ns_window = &*(ns_window_ptr as *mut NSWindow);
            let collection_behavior = launcher_collection_behavior(ns_window.collectionBehavior());
            ns_window.setCollectionBehavior(collection_behavior);
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn elevate_macos_launcher_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window_ptr = window.ns_window().map_err(|error| error.to_string())? as usize;

    window
        .run_on_main_thread(move || unsafe {
            let ns_window = &*(ns_window_ptr as *mut NSWindow);
            ns_window.setLevel(NSPopUpMenuWindowLevel);
            ns_window.orderFrontRegardless();
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn set_macos_launcher_alpha(window: &tauri::WebviewWindow, alpha: f64) -> Result<(), String> {
    let ns_window_ptr = window.ns_window().map_err(|error| error.to_string())? as usize;

    window
        .run_on_main_thread(move || unsafe {
            let ns_window = &*(ns_window_ptr as *mut NSWindow);
            ns_window.setAlphaValue(alpha);
        })
        .map_err(|error| error.to_string())
}

#[cfg(desktop)]
fn launcher_position_for_work_area(
    work_area: &PhysicalRect<i32, u32>,
    window_size: &PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let window_width = window_size.width as i32;
    let window_height = window_size.height as i32;
    let horizontal_space = work_area.size.width as i32 - window_width;
    let vertical_space = work_area.size.height as i32 - window_height;

    PhysicalPosition::new(
        work_area.position.x + (horizontal_space / 2),
        work_area.position.y + (vertical_space / 2),
    )
}

#[cfg(desktop)]
fn desired_launcher_window_logical_size() -> LogicalSize<f64> {
    LogicalSize::new(LAUNCHER_WINDOW_WIDTH, LAUNCHER_WINDOW_HEIGHT)
}

#[cfg(desktop)]
fn desired_launcher_window_physical_size(scale_factor: f64) -> PhysicalSize<u32> {
    PhysicalSize::new(
        (LAUNCHER_WINDOW_WIDTH * scale_factor).round() as u32,
        (LAUNCHER_WINDOW_HEIGHT * scale_factor).round() as u32,
    )
}

#[cfg(desktop)]
fn build_launcher_monitor_bounds(monitor: &Monitor) -> LauncherMonitorBounds {
    let position = monitor.position();
    let size = monitor.size();

    LauncherMonitorBounds {
        left: f64::from(position.x),
        top: f64::from(position.y),
        right: f64::from(position.x) + f64::from(size.width),
        bottom: f64::from(position.y) + f64::from(size.height),
    }
}

#[cfg(desktop)]
fn is_point_inside_monitor(point: (f64, f64), bounds: &LauncherMonitorBounds) -> bool {
    point.0 >= bounds.left
        && point.0 <= bounds.right
        && point.1 >= bounds.top
        && point.1 <= bounds.bottom
}

#[cfg(desktop)]
fn squared_distance_to_monitor(point: (f64, f64), bounds: &LauncherMonitorBounds) -> f64 {
    let dx = if point.0 < bounds.left {
        bounds.left - point.0
    } else if point.0 > bounds.right {
        point.0 - bounds.right
    } else {
        0.0
    };
    let dy = if point.1 < bounds.top {
        bounds.top - point.1
    } else if point.1 > bounds.bottom {
        point.1 - bounds.bottom
    } else {
        0.0
    };

    (dx * dx) + (dy * dy)
}

#[cfg(desktop)]
fn find_monitor_index_for_point(
    monitor_bounds: &[LauncherMonitorBounds],
    point: (f64, f64),
) -> Option<usize> {
    monitor_bounds
        .iter()
        .position(|bounds| is_point_inside_monitor(point, bounds))
        .or_else(|| {
            monitor_bounds
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    squared_distance_to_monitor(point, left)
                        .total_cmp(&squared_distance_to_monitor(point, right))
                })
                .map(|(index, _)| index)
        })
}

#[cfg(desktop)]
fn find_monitor_for_point(monitors: &[Monitor], point: (f64, f64)) -> Option<Monitor> {
    let monitor_bounds = monitors
        .iter()
        .map(build_launcher_monitor_bounds)
        .collect::<Vec<_>>();

    find_monitor_index_for_point(&monitor_bounds, point).and_then(|index| {
        monitors
            .get(index)
            .cloned()
    })
}

#[cfg(desktop)]
fn resolve_launcher_monitor(window: &WebviewWindow) -> Result<Option<Monitor>, String> {
    let monitors = window.available_monitors().map_err(|error| error.to_string())?;

    if let Ok(cursor) = window.cursor_position() {
        if let Some(monitor) = find_monitor_for_point(&monitors, (cursor.x, cursor.y)) {
            return Ok(Some(monitor));
        }
    }

    if let Some(monitor) = window.current_monitor().map_err(|error| error.to_string())? {
        return Ok(Some(monitor));
    }

    if let Some(monitor) = window.primary_monitor().map_err(|error| error.to_string())? {
        return Ok(Some(monitor));
    }

    Ok(monitors.into_iter().next())
}

#[cfg(desktop)]
fn apply_launcher_bounds(window: &WebviewWindow, monitor: &Monitor) -> Result<(), String> {
    let window_size = desired_launcher_window_physical_size(monitor.scale_factor());
    let position = launcher_position_for_work_area(monitor.work_area(), &window_size);

    window
        .set_size(desired_launcher_window_logical_size())
        .map_err(|error| error.to_string())?;
    window
        .set_position(Position::Physical(position))
        .map_err(|error| error.to_string())
}

#[cfg(desktop)]
fn get_or_create_launcher_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("RustyClip")
        .inner_size(LAUNCHER_WINDOW_WIDTH, LAUNCHER_WINDOW_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(true)
        .decorations(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(desktop)]
fn position_launcher_window(window: &WebviewWindow) -> Result<(), String> {
    let Some(monitor) = resolve_launcher_monitor(window)? else {
        return window.center().map_err(|error| error.to_string());
    };

    apply_launcher_bounds(window, &monitor)
}

#[cfg(desktop)]
fn launcher_shortcut() -> Shortcut {
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers::SUPER | Modifiers::SHIFT;
    #[cfg(not(target_os = "macos"))]
    let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;

    Shortcut::new(Some(modifiers), Code::KeyV)
}

#[cfg(desktop)]
fn show_launcher(app: &tauri::AppHandle) -> Result<(), String> {
    let window = get_or_create_launcher_window(app)?;
    let target_monitor = resolve_launcher_monitor(&window)?;
    remember_previous_frontmost_app(app)?;

    window
        .set_visible_on_all_workspaces(true)
        .map_err(|error| error.to_string())?;
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    window
        .set_background_color(Some(Color(0, 0, 0, 0)))
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    set_macos_launcher_alpha(&window, 0.0)?;

    if let Some(target_monitor) = target_monitor.as_ref() {
        apply_launcher_bounds(&window, target_monitor)?;
    } else {
        window.center().map_err(|error| error.to_string())?;
    }

    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    {
        configure_macos_launcher_window(&window)?;
        elevate_macos_launcher_window(&window)?;
    }

    position_launcher_window(&window)?;

    #[cfg(target_os = "macos")]
    {
        let should_reveal_now = match (
            target_monitor.as_ref(),
            window.current_monitor().map_err(|error| error.to_string())?,
        ) {
            (Some(target_monitor), Some(current_monitor)) => {
                (current_monitor.scale_factor() - target_monitor.scale_factor()).abs() < f64::EPSILON
            }
            _ => true,
        };

        if should_reveal_now {
            set_macos_launcher_alpha(&window, 1.0)?;
        }
    }

    app.emit(LAUNCHER_SHOWN_EVENT, ())
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(desktop)]
fn hide_launcher(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window not found".to_string())?;

    window.hide().map_err(|error| error.to_string())?;
    restore_previous_frontmost_app(app)
}

#[cfg(desktop)]
#[tauri::command]
fn hide_launcher_window(app: tauri::AppHandle) -> Result<(), String> {
    hide_launcher(&app)
}

#[cfg(desktop)]
#[tauri::command]
async fn paste_history_into_previous_app(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let bundle_id = take_previous_frontmost_app_bundle_id(&app)?;
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window not found".to_string())?;

    clipboard_history::write_history_item_to_clipboard(&app, id).await?;
    paste_into_previous_app(bundle_id.as_deref())?;
    window.hide().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(desktop)]
fn toggle_launcher(app: &tauri::AppHandle) -> Result<(), String> {
    match app.get_webview_window(MAIN_WINDOW_LABEL) {
        Some(window) => {
            if window.is_visible().map_err(|error| error.to_string())? {
                hide_launcher(app)
            } else {
                show_launcher(app)
            }
        }
        None => show_launcher(app),
    }
}

#[cfg(desktop)]
fn create_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show_panel_item = MenuItem::with_id(app, SHOW_PANEL_MENU_ID, "显示面板", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_MENU_ID, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_panel_item, &quit_item])?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW_PANEL_MENU_ID => {
                if let Err(error) = show_launcher(app) {
                    eprintln!("failed to show launcher from tray menu: {error}");
                }
            }
            QUIT_MENU_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(error) = show_launcher(tray.app_handle()) {
                    eprintln!("failed to show launcher from tray click: {error}");
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    let _ = tray_builder.build(app)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn frontmost_app_bundle_id() -> Result<Option<String>, String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get bundle identifier of first application process whose frontmost is true"#,
        ])
        .output()
        .map_err(|error| format!("failed to read frontmost app: {error}"))?;

    if !output.status.success() {
        return Err(format!("frontmost app query exited with status {}", output.status));
    }

    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if bundle_id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(bundle_id))
    }
}

#[cfg(not(target_os = "macos"))]
fn frontmost_app_bundle_id() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn activate_app_by_bundle_id(bundle_id: &str) -> Result<(), String> {
    let status = Command::new("osascript")
        .args([
            "-e",
            &format!(r#"tell application id "{}" to activate"#, bundle_id),
        ])
        .status()
        .map_err(|error| format!("failed to activate previous app: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("app activation exited with status {status}"))
    }
}

#[cfg(not(target_os = "macos"))]
fn activate_app_by_bundle_id(_bundle_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
fn remember_previous_frontmost_app(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(bundle_id) = frontmost_app_bundle_id()? else {
        return Ok(());
    };

    let state = app.state::<LauncherFocusState>();
    let mut previous_app_bundle_id = state
        .previous_app_bundle_id
        .lock()
        .map_err(|_| "launcher focus state lock is poisoned".to_string())?;

    *previous_app_bundle_id = Some(bundle_id);
    Ok(())
}

#[cfg(desktop)]
fn restore_previous_frontmost_app(app: &tauri::AppHandle) -> Result<(), String> {
    let bundle_id = take_previous_frontmost_app_bundle_id(app)?;
    if let Some(bundle_id) = bundle_id {
        activate_app_by_bundle_id(&bundle_id)?;
    }

    Ok(())
}

#[cfg(desktop)]
fn take_previous_frontmost_app_bundle_id(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    let state = app.state::<LauncherFocusState>();
    let mut previous_app_bundle_id = state
        .previous_app_bundle_id
        .lock()
        .map_err(|_| "launcher focus state lock is poisoned".to_string())?;

    Ok(previous_app_bundle_id.take())
}

#[cfg(target_os = "macos")]
fn paste_into_previous_app(bundle_id: Option<&str>) -> Result<(), String> {
    let mut command = Command::new("osascript");

    if let Some(bundle_id) = bundle_id {
        command.args(["-e", &format!(r#"tell application id "{}" to activate"#, bundle_id)]);
        command.args(["-e", "delay 0.18"]);
    }

    command.args([
        "-e",
        r#"tell application "System Events" to keystroke "v" using command down"#,
    ]);

    let status = command
        .status()
        .map_err(|error| format!("failed to paste into previous app: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("paste into previous app exited with status {status}"))
    }
}

#[cfg(target_os = "windows")]
fn paste_into_previous_app(_bundle_id: Option<&str>) -> Result<(), String> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')"#,
        ])
        .status()
        .map_err(|error| format!("failed to paste into previous app: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("paste into previous app exited with status {status}"))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn paste_into_previous_app(_bundle_id: Option<&str>) -> Result<(), String> {
    Err("automatic paste is not supported on this platform yet".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let sql_plugin = tauri_plugin_sql::Builder::new()
        .add_migrations(clipboard_history::DB_URL, clipboard_history::migrations())
        .build();

    tauri::Builder::default()
        .plugin(sql_plugin)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent, // macOS 启动方式
            Some(vec![]), // 启动参数
        ))
        .manage(LauncherFocusState::default())
        .on_window_event(|window, event| {
            if window.label() == MAIN_WINDOW_LABEL {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    tauri::WindowEvent::Focused(false) => {
                        let _ = window.hide();
                    }
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        if let Some(launcher_window) =
                            window.app_handle().get_webview_window(MAIN_WINDOW_LABEL)
                        {
                            if launcher_window.is_visible().unwrap_or(false) {
                                let _ = position_launcher_window(&launcher_window);
                                #[cfg(target_os = "macos")]
                                let _ = set_macos_launcher_alpha(&launcher_window, 1.0);
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .setup(|app| {
            #[cfg(desktop)]
            {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(ActivationPolicy::Accessory);

                let toggle_shortcut = launcher_shortcut();

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler({
                            let toggle_shortcut = toggle_shortcut.clone();

                            move |app, shortcut, event| {
                                if shortcut == &toggle_shortcut
                                    && event.state() == ShortcutState::Pressed
                                {
                                    if let Err(error) = toggle_launcher(app) {
                                        eprintln!(
                                            "failed to toggle launcher from global shortcut: {error}"
                                        );
                                    }
                                }
                            }
                        })
                        .build(),
                )?;

                if let Err(error) = create_tray(app.handle()) {
                    eprintln!("failed to create tray icon: {error}");
                }

                if let Err(error) = app.global_shortcut().register(toggle_shortcut) {
                    eprintln!("failed to register global shortcut: {error}");
                }
            }

            clipboard_history::setup(app.handle().clone())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            hide_launcher_window,
            paste_history_into_previous_app,
            clipboard_history::list_clipboard_history,
            clipboard_history::copy_clipboard_history,
            clipboard_history::paste_clipboard_history,
            clipboard_history::delete_clipboard_history,
            clipboard_history::clear_clipboard_history,
            clipboard_history::toggle_pin_clipboard_history,
            clipboard_history::toggle_favorite_clipboard_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::launcher_collection_behavior;
    use objc2_app_kit::NSWindowCollectionBehavior;

    #[test]
    fn launcher_collection_behavior_replaces_conflicting_fullscreen_flags() {
        let current = NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenPrimary
            | NSWindowCollectionBehavior::FullScreenAllowsTiling;

        let behavior = launcher_collection_behavior(current);

        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));
        assert!(!behavior.contains(NSWindowCollectionBehavior::FullScreenPrimary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::FullScreenAllowsTiling));
    }
}

#[cfg(all(test, desktop))]
mod positioning_tests {
    use super::{
        desired_launcher_window_physical_size, find_monitor_index_for_point,
        launcher_position_for_work_area,
        LauncherMonitorBounds,
    };
    use tauri::{PhysicalPosition, PhysicalRect, PhysicalSize};

    #[test]
    fn launcher_position_respects_scale_factor() {
        let work_area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(3024, 1890),
        };
        let window_size = PhysicalSize::new(2360, 1520);

        let position = launcher_position_for_work_area(&work_area, &window_size);

        assert_eq!(position.x, 332);
        assert_eq!(position.y, 185);
    }

    #[test]
    fn launcher_window_size_uses_target_monitor_scale_factor() {
        let window_size = desired_launcher_window_physical_size(1.5);

        assert_eq!(window_size.width, 1770);
        assert_eq!(window_size.height, 1140);
    }

    #[test]
    fn launcher_monitor_selection_prefers_cursor_monitor_across_mixed_dpi() {
        let monitor_bounds = vec![
            LauncherMonitorBounds {
                left: 0.0,
                top: 0.0,
                right: 3024.0,
                bottom: 1964.0,
            },
            LauncherMonitorBounds {
                left: 3024.0,
                top: 0.0,
                right: 4944.0,
                bottom: 1080.0,
            },
        ];

        let external = find_monitor_index_for_point(&monitor_bounds, (3500.0, 300.0)).unwrap();
        let retina = find_monitor_index_for_point(&monitor_bounds, (1200.0, 300.0)).unwrap();

        assert_eq!(external, 1);
        assert_eq!(retina, 0);
    }
}
