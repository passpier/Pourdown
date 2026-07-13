// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod convert;
#[cfg(test)]
mod fixture_gen;

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use tauri::menu::{AboutMetadata, AboutMetadataBuilder, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager, State};
use walkdir::WalkDir;
use regex::RegexBuilder;

// Dev-only diagnostic logging: compiles to a no-op in release builds so
// nothing but real errors reach stdout in a shipped binary. Behaves like
// `println!` during `cargo run` / `tauri dev` (debug_assertions is set).
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

// State management
struct AppState {
    recent_files: Mutex<VecDeque<String>>,
    pending_open_files: Mutex<VecDeque<String>>,
    language: Mutex<String>,
    source_mode: Mutex<bool>,
}

impl AppState {
    fn new(language: String, source_mode: bool) -> Self {
        AppState {
            recent_files: Mutex::new(VecDeque::new()),
            pending_open_files: Mutex::new(VecDeque::new()),
            language: Mutex::new(language),
            source_mode: Mutex::new(source_mode),
        }
    }
}

/**
 * Returns the app's config directory, creating it (and migrating the legacy
 * "MarkBear" directory into it) on first use. Platform-specific:
 * - macOS: ~/Library/Application Support/Pourdown
 * - Windows: C:\Users\{User}\AppData\Local\Pourdown
 * - Linux: ~/.config/Pourdown
 */
fn app_config_dir() -> Result<PathBuf, String> {
    let config_dir = if cfg!(target_os = "macos") {
        // macOS: ~/Library/Application Support
        let home =
            std::env::var("HOME").map_err(|_| "Failed to get HOME directory".to_string())?;
        PathBuf::from(home).join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        // Windows: %LOCALAPPDATA%
        let local_app_data = std::env::var("LOCALAPPDATA")
            .map_err(|_| "Failed to get LOCALAPPDATA directory".to_string())?;
        PathBuf::from(local_app_data)
    } else {
        // Linux: ~/.config
        let home =
            std::env::var("HOME").map_err(|_| "Failed to get HOME directory".to_string())?;
        PathBuf::from(home).join(".config")
    };

    let app_config_dir = config_dir.join("Pourdown");

    // One-time migration: if this is the first launch under the new name
    // and a config directory from the old "MarkBear" name still exists,
    // move it over so existing users keep their settings. Best-effort —
    // if it fails for any reason, just fall through and create fresh.
    if !app_config_dir.exists() {
        let legacy_config_dir = config_dir.join("MarkBear");
        if legacy_config_dir.exists() {
            let _ = fs::rename(&legacy_config_dir, &app_config_dir);
        }
    }

    // Create directory if it doesn't exist
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    Ok(app_config_dir)
}

/// Directory holding per-import staging media (`imports/{id}/assets/...`).
/// Created lazily; not tied to document save location.
fn imports_root_dir() -> Result<PathBuf, String> {
    let dir = app_config_dir()?.join("imports");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create imports directory: {}", e))?;
    Ok(dir)
}

// User settings - stored persistently in config directory
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserSettings {
    language: String,
}

impl UserSettings {
    /**
     * Get the path to the settings file in the app's config directory
     * Uses platform-specific config directories:
     * - macOS: ~/Library/Application Support/Pourdown
     * - Windows: C:\Users\{User}\AppData\Local\Pourdown
     * - Linux: ~/.config/Pourdown
     */
    fn config_path() -> Result<PathBuf, String> {
        Ok(app_config_dir()?.join("settings.json"))
    }

    /**
     * Load settings from file.
     * Returns Ok(Some(settings)) if the file exists and parses successfully,
     * Ok(None) if the file does not exist (first launch),
     * or Err if the file exists but cannot be read/parsed.
     */
    fn load() -> Result<Option<Self>, String> {
        let path = Self::config_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read settings file: {}", e))?;

            // Use a permissive intermediate value so unknown fields (e.g. old
            // `source_mode`) are silently ignored rather than causing a parse error.
            let raw: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse settings: {}", e))?;
            let settings: UserSettings = serde_json::from_value(raw)
                .map_err(|e| format!("Failed to deserialize settings: {}", e))?;

            debug_log!("📂 Settings loaded from: {}", path.display());
            Ok(Some(settings))
        } else {
            debug_log!("📂 Settings file not found (first launch)");
            Ok(None)
        }
    }

    /**
     * Save settings to file
     */
    fn save(&self) -> Result<(), String> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;
        
        debug_log!("💾 Settings saved to: {}", path.display());
        Ok(())
    }
}

fn get_label(lang: &str, key: &str) -> String {
    match lang {
        "zh" => match key {
            "file" => "檔案".to_string(),
            "file_new" => "新檔案".to_string(),
            "file_open" => "開啟...".to_string(),
            "file_save" => "儲存".to_string(),
            "file_save_as" => "另存新檔...".to_string(),
            "file_close_document" => "關閉文件".to_string(),
            "format" => "格式".to_string(),
            "format_text" => "文字".to_string(),
            "format_bold" => "粗體".to_string(),
            "format_italic" => "斜體".to_string(),
            "format_strike" => "刪除線".to_string(),
            "format_inline_code" => "行內程式碼".to_string(),
            "format_headings" => "標題".to_string(),
            "format_paragraph" => "本文".to_string(),
            "format_heading_1" => "標題 1".to_string(),
            "format_heading_2" => "標題 2".to_string(),
            "format_heading_3" => "標題 3".to_string(),
            "format_heading_4" => "標題 4".to_string(),
            "format_heading_5" => "標題 5".to_string(),
            "format_heading_6" => "標題 6".to_string(),
            "format_lists" => "清單".to_string(),
            "format_bullet_list" => "項目符號清單".to_string(),
            "format_ordered_list" => "編號清單".to_string(),
            "format_blocks" => "區塊".to_string(),
            "format_blockquote" => "引用".to_string(),
            "format_code_block" => "程式碼區塊".to_string(),
            "format_horizontal_rule" => "水平分割線".to_string(),
            "view" => "檢視".to_string(),
            "view_source_code" => "原始碼".to_string(),
            "view_theme" => "佈景主題".to_string(),
            "view_language" => "語言".to_string(),
            "edit" => "編輯".to_string(),
            "edit_undo" => "復原".to_string(),
            "edit_redo" => "重做".to_string(),
            "edit_cut" => "剪下".to_string(),
            "edit_copy" => "複製".to_string(),
            "edit_paste" => "貼上".to_string(),
            "edit_select_all" => "全選".to_string(),
            "edit_find" => "尋找".to_string(),
            "edit_find_in_files" => "在檔案中尋找".to_string(),
            "window" => "視窗".to_string(),
            "help" => "說明".to_string(),
            "lang_en" => "English".to_string(),
            "lang_zh" => "繁體中文".to_string(),
            "file_import" => "匯入".to_string(),
            "file_import_docx" => "從 Word (.docx)".to_string(),
            "file_import_xlsx" => "從試算表 (.xlsx)".to_string(),
            "file_import_pdf"  => "從 PDF".to_string(),
            "file_import_pptx" => "從 PowerPoint (.pptx)".to_string(),
            "file_export" => "匯出".to_string(),
            "file_export_html" => "匯出為 HTML...".to_string(),
            "file_export_pdf"  => "匯出為 PDF...".to_string(),
            "app_about" => "關於 Pourdown".to_string(),
            "app_preferences" => "偏好設定…".to_string(),
            "app_services" => "服務".to_string(),
            "app_hide" => "隱藏 Pourdown".to_string(),
            "app_hide_others" => "隱藏其他".to_string(),
            "app_show_all" => "全部顯示".to_string(),
            "app_quit" => "結束 Pourdown".to_string(),
            "window_minimize" => "縮到最小".to_string(),
            "window_zoom" => "縮放".to_string(),
            "window_fullscreen" => "切換全螢幕".to_string(),
            "window_close" => "關閉視窗".to_string(),
            _ => key.to_string(),
        },
        _ => match key {
            "file" => "File".to_string(),
            "file_new" => "New File".to_string(),
            "file_open" => "Open...".to_string(),
            "file_save" => "Save".to_string(),
            "file_save_as" => "Save As...".to_string(),
            "file_close_document" => "Close Document".to_string(),
            "format" => "Format".to_string(),
            "format_text" => "Text".to_string(),
            "format_bold" => "Bold".to_string(),
            "format_italic" => "Italic".to_string(),
            "format_strike" => "Strikethrough".to_string(),
            "format_inline_code" => "Inline Code".to_string(),
            "format_headings" => "Headings".to_string(),
            "format_paragraph" => "Paragraph".to_string(),
            "format_heading_1" => "Heading 1".to_string(),
            "format_heading_2" => "Heading 2".to_string(),
            "format_heading_3" => "Heading 3".to_string(),
            "format_heading_4" => "Heading 4".to_string(),
            "format_heading_5" => "Heading 5".to_string(),
            "format_heading_6" => "Heading 6".to_string(),
            "format_lists" => "Lists".to_string(),
            "format_bullet_list" => "Bullet List".to_string(),
            "format_ordered_list" => "Ordered List".to_string(),
            "format_blocks" => "Blocks".to_string(),
            "format_blockquote" => "Blockquote".to_string(),
            "format_code_block" => "Code Block".to_string(),
            "format_horizontal_rule" => "Horizontal Rule".to_string(),
            "view" => "View".to_string(),
            "view_source_code" => "Source Code".to_string(),
            "view_theme" => "Theme".to_string(),
            "view_language" => "Language".to_string(),
            "edit" => "Edit".to_string(),
            "edit_undo" => "Undo".to_string(),
            "edit_redo" => "Redo".to_string(),
            "edit_cut" => "Cut".to_string(),
            "edit_copy" => "Copy".to_string(),
            "edit_paste" => "Paste".to_string(),
            "edit_select_all" => "Select All".to_string(),
            "edit_find" => "Find...".to_string(),
            "edit_find_in_files" => "Find in Files".to_string(),
            "window" => "Window".to_string(),
            "help" => "Help".to_string(),
            "lang_en" => "English".to_string(),
            "lang_zh" => "繁體中文".to_string(),
            "file_import" => "Import".to_string(),
            "file_import_docx" => "From Word (.docx)".to_string(),
            "file_import_xlsx" => "From Spreadsheet (.xlsx)".to_string(),
            "file_import_pdf"  => "From PDF".to_string(),
            "file_import_pptx" => "From PowerPoint (.pptx)".to_string(),
            "file_export" => "Export".to_string(),
            "file_export_html" => "Export as HTML...".to_string(),
            "file_export_pdf"  => "Export as PDF...".to_string(),
            "app_about" => "About Pourdown".to_string(),
            "app_preferences" => "Preferences…".to_string(),
            "app_services" => "Services".to_string(),
            "app_hide" => "Hide Pourdown".to_string(),
            "app_hide_others" => "Hide Others".to_string(),
            "app_show_all" => "Show All".to_string(),
            "app_quit" => "Quit Pourdown".to_string(),
            "window_minimize" => "Minimize".to_string(),
            "window_zoom" => "Zoom".to_string(),
            "window_fullscreen" => "Toggle Full Screen".to_string(),
            "window_close" => "Close Window".to_string(),
            _ => key.to_string(),
        },
    }
}

// File entry for directory listing
#[derive(Serialize, Deserialize, Clone)]
struct FileEntry {
    name: String,
    path: String,
    is_directory: bool,
}

// Read a markdown file
#[tauri::command]
async fn read_markdown_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {}", e))
}

// Save a markdown file
#[tauri::command]
async fn save_markdown_file(path: String, content: String) -> Result<(), String> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = PathBuf::from(&path).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write file: {}", e))
}

// List directory contents
#[tauri::command]
async fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let entries = fs::read_dir(&path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;
    
    let mut file_entries = Vec::new();
    
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                
                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }
                
                let is_directory = path.is_dir();
                let path_str = path.to_string_lossy().to_string();
                
                file_entries.push(FileEntry {
                    name,
                    path: path_str,
                    is_directory,
                });
            }
            Err(_) => continue,
        }
    }
    
    // Sort: directories first, then files, both alphabetically
    file_entries.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    
    Ok(file_entries)
}

// Get recent files
#[tauri::command]
fn get_recent_files(state: State<AppState>) -> Result<Vec<String>, String> {
    let recent = state.recent_files.lock()
        .map_err(|_| "Failed to lock state".to_string())?;
    Ok(recent.iter().cloned().collect())
}

// Add a file to recent files
#[tauri::command]
fn add_recent_file(path: String, state: State<AppState>) -> Result<(), String> {
    let mut recent = state.recent_files.lock()
        .map_err(|_| "Failed to lock state".to_string())?;
    
    // Remove if already exists
    recent.retain(|p| p != &path);
    
    // Add to front
    recent.push_front(path);
    
    // Keep only 10 most recent
    recent.truncate(10);
    
    Ok(())
}

// Create a new file
#[tauri::command]
async fn create_file(path: String) -> Result<(), String> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = PathBuf::from(&path).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    // Create empty file
    fs::write(&path, "")
        .map_err(|e| format!("Failed to create file: {}", e))
}

// Delete a file
#[tauri::command]
async fn delete_file(path: String) -> Result<(), String> {
    fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete file: {}", e))
}

// Rename a file
#[tauri::command]
async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(&old_path, &new_path)
        .map_err(|e| format!("Failed to rename file: {}", e))
}

// Check if file exists
#[tauri::command]
fn file_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}

// Search result for cross-file search
#[derive(Serialize, Clone)]
struct SearchResult {
    file_path: String,
    line_number: usize,
    line_content: String,
    match_start: usize,
    match_end: usize,
}

// Search across all markdown files in a directory
#[tauri::command]
async fn search_in_files(
    root: String,
    query: String,
    case_sensitive: bool,
    use_regex: bool,
) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let pattern = if use_regex {
        query.clone()
    } else {
        regex::escape(&query)
    };

    let re = RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut results: Vec<SearchResult> = Vec::new();
    const MAX_RESULTS: usize = 500;

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if results.len() >= MAX_RESULTS {
            break;
        }

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "md" && ext != "markdown" {
            continue;
        }

        // Skip hidden files/dirs
        let is_hidden = path.components().any(|c| {
            c.as_os_str().to_str().map(|s| s.starts_with('.')).unwrap_or(false)
        });
        if is_hidden {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_path_str = path.to_string_lossy().to_string();

        for (line_idx, line) in content.lines().enumerate() {
            if results.len() >= MAX_RESULTS {
                break;
            }

            for m in re.find_iter(line) {
                results.push(SearchResult {
                    file_path: file_path_str.clone(),
                    line_number: line_idx + 1,
                    line_content: line.to_string(),
                    match_start: m.start(),
                    match_end: m.end(),
                });

                if results.len() >= MAX_RESULTS {
                    break;
                }
            }
        }
    }

    Ok(results)
}

/**
 * Normalize language code to supported format ('en' or 'zh')
 */
fn normalize_language(lang: &str) -> String {
    match lang.to_lowercase().split('-').next().unwrap_or("en") {
        "zh" => "zh".to_string(),
        _ => "en".to_string(),
    }
}

/**
 * Get system locale using backend detection (Tauri v2 best practice)
 * Detects locale at Rust level for better performance and reliability
 */
#[tauri::command]
fn get_system_locale() -> Result<String, String> {
    match tauri_plugin_os::locale() {
        Some(locale_str) => {
            let normalized = normalize_language(&locale_str);
            debug_log!("🌍 System locale detected: {} → normalized to: {}", 
                     locale_str, normalized);
            Ok(normalized)
        }
        None => {
            debug_log!("⚠️ System locale not available, using default: English");
            Ok("en".to_string())
        }
    }
}

/**
 * Get the current language setting
 */
#[tauri::command]
fn get_language(state: State<AppState>) -> Result<String, String> {
    let lang = state.language.lock()
        .map_err(|_| "Failed to lock language state".to_string())?;
    Ok(lang.clone())
}

/**
 * Set language (typically called from frontend after user changes preference)
 * This updates the state but NOT the menu (menu is handled in event handler)
 */
#[tauri::command]
fn set_language(state: State<AppState>, lang: String) -> Result<(), String> {
    let normalized_lang = normalize_language(&lang);
    
    let mut l = state.language.lock()
        .map_err(|_| "Failed to lock language state".to_string())?;
    *l = normalized_lang.clone();
    
    debug_log!("💾 Language state updated to: {}", normalized_lang);
    Ok(())
}

/**
 * Get user settings (including language preference from persistent storage)
 * Loads from config file or returns defaults
 * Tauri v2 best practice: store settings in app config directory
 */
#[tauri::command]
fn get_user_settings() -> Result<UserSettings, String> {
    let settings = UserSettings::load()?.unwrap_or_else(|| UserSettings { language: "en".to_string() });
    debug_log!("📂 User settings retrieved: language={}", settings.language);
    Ok(settings)
}

/**
 * Save user language preference to persistent storage
 * This ensures language preference survives app restarts
 *
 * Also rebuilds the native menu in the new language and re-syncs the
 * view_source_code checkmark — mirroring what the lang_en/lang_zh
 * on_menu_event branches already do. Language can now be changed from two
 * places: the native View > Language menu (which rebuilds itself) and the
 * in-app Preferences dialog (which only calls this command) — without this,
 * switching language from Preferences left the native menu bar stuck in the
 * old language until restart.
 */
#[tauri::command]
fn save_language_preference(app: AppHandle, lang: String, state: State<AppState>) -> Result<(), String> {
    let normalized_lang = normalize_language(&lang);

    // Load existing settings (to preserve other settings if any)
    let mut settings = UserSettings::load()?.unwrap_or_else(|| UserSettings { language: "en".to_string() });

    // Update language
    settings.language = normalized_lang.clone();

    // Save to file
    settings.save()?;

    // Also update in-memory state
    let mut l = state.language.lock()
        .map_err(|_| "Failed to lock language state".to_string())?;
    *l = normalized_lang.clone();
    drop(l);

    // Rebuild the native menu in the new language.
    if let Ok(menu) = create_app_menu(&app, &normalized_lang) {
        let _ = app.set_menu(menu);
    }
    // Re-sync view_source_code checkmark with current source_mode state
    if let Ok(sm) = state.source_mode.lock() {
        if let Some(menu) = app.menu() {
            if let Some(item) = menu.get("view_source_code") {
                if let Some(check_item) = item.as_check_menuitem() {
                    let _ = check_item.set_checked(*sm);
                }
            }
        }
    }

    debug_log!("💾 Language preference saved and state updated to: {}", normalized_lang);
    Ok(())
}

// Update check menu item state
#[tauri::command]
fn update_menu_item_state(app: AppHandle, state: State<AppState>, id: String, checked: bool) -> Result<(), String> {
    // Track source_mode in AppState for cross-event consistency
    if id == "view_source_code" {
        if let Ok(mut sm) = state.source_mode.lock() {
            *sm = checked;
        }
    }
    if let Some(menu) = app.menu() {
        if let Some(item) = menu.get(&id) {
            if let Some(check_item) = item.as_check_menuitem() {
                let _ = check_item.set_checked(checked);
            }
        }
    }
    Ok(())
}

// Enable or disable a menu item by id
#[tauri::command]
fn enable_menu_item(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    if let Some(menu) = app.menu() {
        if let Some(item) = menu.get(&id) {
            match item {
                tauri::menu::MenuItemKind::MenuItem(mi) => mi.set_enabled(enabled).map_err(|e| e.to_string())?,
                tauri::menu::MenuItemKind::Submenu(sm) => sm.set_enabled(enabled).map_err(|e| e.to_string())?,
                tauri::menu::MenuItemKind::Check(cm) => cm.set_enabled(enabled).map_err(|e| e.to_string())?,
                tauri::menu::MenuItemKind::Icon(im) => im.set_enabled(enabled).map_err(|e| e.to_string())?,
                tauri::menu::MenuItemKind::Predefined(_) => {}
            }
        }
    }
    Ok(())
}

// Result of importing a document: the converted Markdown plus the directory
// (if any) holding sidecar images extracted during conversion. `media_dir` is
// empty when the import produced no images (nothing to clean up or relocate).
#[derive(Serialize)]
struct ImportResult {
    markdown: String,
    media_dir: String,
}

/// Create a fresh, empty staging directory under `imports/` for one import's
/// extracted media. Uses a timestamp+counter for uniqueness rather than a
/// `uuid` dependency, since the id only needs to be locally unique.
fn new_import_dir() -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let root = imports_root_dir()?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = root.join(format!("{}-{}", ts, n));
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create import directory: {}", e))?;
    Ok(dir)
}

// Import a document from a non-markdown format and return Markdown content
// plus the directory (if any) holding extracted sidecar images.
#[tauri::command]
async fn import_document(path: String, format: String) -> Result<ImportResult, String> {
    tokio::task::spawn_blocking(move || {
        let import_dir = new_import_dir()?;
        let assets_dir = import_dir.join("assets");
        // Downscaling is off by default (preserve originals verbatim). Wire a
        // `max_image_dimension` user setting through here (e.g. default
        // Some(2048)) once a Settings UI toggle for it exists.
        let mut media = convert::media::MediaSink::new(assets_dir).with_max_dimension(None);

        let markdown = match format.as_str() {
            "docx" => convert::docx::docx_to_markdown(&path, &mut media).map_err(String::from),
            "xlsx" => convert::xlsx::xlsx_to_markdown(&path, &mut media).map_err(String::from),
            "pdf" => convert::pdf::pdf_to_markdown(&path, &mut media).map_err(String::from),
            "pptx" => convert::pptx::pptx_to_markdown(&path, &mut media).map_err(String::from),
            other => Err(format!("Unsupported import format: {}", other)),
        }?;

        // Text-only import: don't leave an empty staging directory behind.
        if media.is_empty() {
            let _ = fs::remove_dir_all(&import_dir);
            return Ok(ImportResult {
                markdown,
                media_dir: String::new(),
            });
        }

        Ok(ImportResult {
            markdown,
            media_dir: import_dir.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Relocate an import's staging media directory to sit next to a saved
/// document (e.g. `<doc-basename>.assets/`). Tries an atomic rename first
/// (fast, common case) and falls back to a recursive copy + cleanup for
/// cross-device moves.
#[tauri::command]
async fn relocate_media(from: String, to: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let from = PathBuf::from(from);
        let to = PathBuf::from(to);
        if !from.exists() {
            return Ok(());
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if fs::rename(&from, &to).is_ok() {
            return Ok(());
        }
        copy_dir_recursive(&from, &to).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir_all(&from);
        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Discard an import's staging media directory (called when a document that
/// was never saved is closed). Scoped to `imports/` so it can't be pointed at
/// arbitrary paths.
#[tauri::command]
async fn discard_media(import_dir: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let dir = PathBuf::from(&import_dir);
        if let Ok(root) = imports_root_dir() {
            if dir.starts_with(&root) {
                let _ = fs::remove_dir_all(&dir);
            }
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

// Export Markdown content to a non-markdown format
#[tauri::command]
async fn export_document(content: String, path: String, format: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || match format.as_str() {
        "pdf"  => convert::pdf::markdown_to_pdf(&content, &path).map_err(String::from),
        "html" => convert::html::markdown_to_html(&content, &path).map_err(String::from),
        other  => Err(format!("Unsupported export format: {}", other)),
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

// Drain any pending open-file requests (used on app startup).
#[tauri::command]
fn take_pending_open_files(state: State<AppState>) -> Result<Vec<String>, String> {
    let mut pending = state.pending_open_files
        .lock()
        .map_err(|_| "Failed to lock pending open files".to_string())?;
    Ok(pending.drain(..).collect())
}

// Get OS platform (compile-time detection for early initialization)
#[tauri::command]
fn get_os_platform() -> String {
    #[cfg(target_os = "macos")]
    {
        "macos".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "windows".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "gnome".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown".to_string()
    }
}

#[derive(Serialize, Clone)]
struct MenuCommandPayload {
    command: String,
    level: Option<u8>,
}

fn emit_editor_command(app: &tauri::AppHandle, command: &str, level: Option<u8>) {
    let payload = MenuCommandPayload {
        command: command.to_string(),
        level,
    };
    let _ = app.emit("menu-editor-command", payload);
}

/**
 * Helper function to save language preference to persistent storage
 * Used by menu event handlers
 */
fn save_language_to_storage(lang: &str) -> Result<(), String> {
    let mut settings = UserSettings::load()?.unwrap_or_else(|| UserSettings { language: "en".to_string() });
    settings.language = lang.to_string();
    settings.save()
}

fn normalize_open_path(arg: &str) -> Option<String> {
    let trimmed = arg.trim_matches('"');
    if trimmed.is_empty() || trimmed.starts_with("-psn_") {
        return None;
    }

    let path_str = if let Ok(url) = tauri::Url::parse(trimmed) {
        if url.scheme() == "file" {
            url.to_file_path().ok()
        } else {
            None
        }
    } else {
        Some(PathBuf::from(trimmed))
    };

    let path = path_str?;

    if !path.is_file() {
        return None;
    }

    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase();
    if ext != "md" && ext != "markdown" {
        return None;
    }

    Some(path.to_string_lossy().to_string())
}

fn collect_open_paths<I>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    args.into_iter()
        .filter_map(|arg| normalize_open_path(&arg))
        .collect()
}

fn queue_open_files(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }

    if let Ok(mut pending) = app.state::<AppState>().pending_open_files.lock() {
        for path in &paths {
            if !pending.contains(path) {
                pending.push_back(path.clone());
            }
        }
    }

    for path in paths {
        let _ = app.emit("open-file", path);
    }
}

// App metadata shown in the native About panel (macOS app menu, Help menu on
// Windows/Linux). Version is read from Cargo.toml at compile time so it never
// drifts from the crate's own version field; "(beta)" is appended here rather
// than baked into the version number itself, so Cargo.toml / package.json /
// tauri.conf.json can all stay a plain semver (0.5.0) that Windows' MSI
// installer accepts without complaint.
fn about_metadata<'a>() -> AboutMetadata<'a> {
    AboutMetadataBuilder::new()
        .name(Some("Pourdown"))
        .version(Some(format!("{} (beta)", env!("CARGO_PKG_VERSION"))))
        .authors(Some(vec!["passpier".into()]))
        .comments(Some("Turn any document into clean, editable Markdown."))
        .copyright(Some("© 2026 passpier · MIT License"))
        .website(Some("https://passpier.github.io/Pourdown/"))
        .website_label(Some("Website"))
        .icon(Image::from_bytes(include_bytes!("../icons/128x128@2x.png")).ok())
        .build()
}

fn create_app_menu<R: tauri::Runtime>(handle: &AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(handle)?;

    // macOS App Name Menu — the leftmost slot (app name is filled automatically by macOS)
    #[cfg(target_os = "macos")]
    {
        let preferences_item = MenuItem::with_id(handle, "app_preferences", get_label(lang, "app_preferences"), true, Some("CmdOrCtrl+Comma"))?;
        let app_menu = Submenu::with_items(
            handle,
            "Pourdown",
            true,
            &[
                &PredefinedMenuItem::about(handle, Some(&get_label(lang, "app_about")), Some(about_metadata()))?,
                &PredefinedMenuItem::separator(handle)?,
                &preferences_item,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::services(handle, Some(&get_label(lang, "app_services")))?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::hide(handle, Some(&get_label(lang, "app_hide")))?,
                &PredefinedMenuItem::hide_others(handle, Some(&get_label(lang, "app_hide_others")))?,
                &PredefinedMenuItem::show_all(handle, Some(&get_label(lang, "app_show_all")))?,
                &PredefinedMenuItem::separator(handle)?,
                &PredefinedMenuItem::quit(handle, Some(&get_label(lang, "app_quit")))?,
            ],
        )?;
        menu.append(&app_menu)?;
    }

    // File Menu
    let new_item = MenuItem::with_id(handle, "file_new", get_label(lang, "file_new"), true, Some("CmdOrCtrl+N"))?;
    let open_item = MenuItem::with_id(handle, "file_open", get_label(lang, "file_open"), true, Some("CmdOrCtrl+O"))?;
    let save_item = MenuItem::with_id(handle, "file_save", get_label(lang, "file_save"), true, Some("CmdOrCtrl+S"))?;
    let save_as_item = MenuItem::with_id(handle, "file_save_as", get_label(lang, "file_save_as"), true, Some("CmdOrCtrl+Shift+S"))?;
    let close_document_item = MenuItem::with_id(handle, "file_close_document", get_label(lang, "file_close_document"), true, Some("CmdOrCtrl+W"))?;

    let import_docx_item = MenuItem::with_id(handle, "file_import_docx", get_label(lang, "file_import_docx"), true, None::<&str>)?;
    let import_xlsx_item = MenuItem::with_id(handle, "file_import_xlsx", get_label(lang, "file_import_xlsx"), true, None::<&str>)?;
    let import_pdf_item  = MenuItem::with_id(handle, "file_import_pdf",  get_label(lang, "file_import_pdf"),  true, None::<&str>)?;
    let import_pptx_item = MenuItem::with_id(handle, "file_import_pptx", get_label(lang, "file_import_pptx"), true, None::<&str>)?;
    let import_submenu = Submenu::with_items(
        handle,
        get_label(lang, "file_import"),
        true,
        &[&import_docx_item, &import_xlsx_item, &import_pdf_item, &import_pptx_item],
    )?;

    let export_html_item = MenuItem::with_id(handle, "file_export_html", get_label(lang, "file_export_html"), true, None::<&str>)?;
    let export_pdf_item  = MenuItem::with_id(handle, "file_export_pdf",  get_label(lang, "file_export_pdf"),  true, None::<&str>)?;
    let export_submenu = Submenu::with_items(
        handle,
        get_label(lang, "file_export"),
        true,
        &[&export_html_item, &export_pdf_item],
    )?;

    let file_menu = Submenu::with_items(
        handle,
        get_label(lang, "file"),
        true,
        &[
            &new_item,
            &open_item,
            &PredefinedMenuItem::separator(handle)?,
            &save_item,
            &save_as_item,
            &close_document_item,
            &PredefinedMenuItem::separator(handle)?,
            &import_submenu,
            &export_submenu,
        ],
    )?;
    menu.append(&file_menu)?;

    // Edit Menu
    let find_item = MenuItem::with_id(handle, "edit_find", get_label(lang, "edit_find"), true, Some("CmdOrCtrl+F"))?;
    let find_in_files_item = MenuItem::with_id(handle, "edit_find_in_files", get_label(lang, "edit_find_in_files"), true, Some("CmdOrCtrl+Shift+F"))?;
    let edit_menu = Submenu::with_items(
        handle,
        get_label(lang, "edit"),
        true,
        &[
            &PredefinedMenuItem::undo(handle, Some(&get_label(lang, "edit_undo")))?,
            &PredefinedMenuItem::redo(handle, Some(&get_label(lang, "edit_redo")))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, Some(&get_label(lang, "edit_cut")))?,
            &PredefinedMenuItem::copy(handle, Some(&get_label(lang, "edit_copy")))?,
            &PredefinedMenuItem::paste(handle, Some(&get_label(lang, "edit_paste")))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::select_all(handle, Some(&get_label(lang, "edit_select_all")))?,
            &PredefinedMenuItem::separator(handle)?,
            &find_item,
            &find_in_files_item,
        ],
    )?;
    menu.append(&edit_menu)?;

    // Format Menu
    let bold_item = MenuItem::with_id(handle, "editor_bold", get_label(lang, "format_bold"), true, Some("CmdOrCtrl+B"))?;
    let italic_item = MenuItem::with_id(handle, "editor_italic", get_label(lang, "format_italic"), true, Some("CmdOrCtrl+I"))?;
    let strike_item = MenuItem::with_id(handle, "editor_strike", get_label(lang, "format_strike"), true, Some("CmdOrCtrl+Shift+X"))?;
    let inline_code_item = MenuItem::with_id(handle, "editor_inline_code", get_label(lang, "format_inline_code"), true, Some("CmdOrCtrl+Shift+C"))?;
    let paragraph_item = MenuItem::with_id(handle, "editor_paragraph", get_label(lang, "format_paragraph"), true, None::<&str>)?;
    let heading_1_item = MenuItem::with_id(handle, "editor_heading_1", get_label(lang, "format_heading_1"), true, Some("CmdOrCtrl+Option+1"))?;
    let heading_2_item = MenuItem::with_id(handle, "editor_heading_2", get_label(lang, "format_heading_2"), true, Some("CmdOrCtrl+Option+2"))?;
    let heading_3_item = MenuItem::with_id(handle, "editor_heading_3", get_label(lang, "format_heading_3"), true, Some("CmdOrCtrl+Option+3"))?;
    let heading_4_item = MenuItem::with_id(handle, "editor_heading_4", get_label(lang, "format_heading_4"), true, Some("CmdOrCtrl+Option+4"))?;
    let heading_5_item = MenuItem::with_id(handle, "editor_heading_5", get_label(lang, "format_heading_5"), true, Some("CmdOrCtrl+Option+5"))?;
    let heading_6_item = MenuItem::with_id(handle, "editor_heading_6", get_label(lang, "format_heading_6"), true, Some("CmdOrCtrl+Option+6"))?;
    let bullet_list_item = MenuItem::with_id(handle, "editor_bullet_list", get_label(lang, "format_bullet_list"), true, Some("CmdOrCtrl+Shift+8"))?;
    let ordered_list_item = MenuItem::with_id(handle, "editor_ordered_list", get_label(lang, "format_ordered_list"), true, Some("CmdOrCtrl+Shift+7"))?;
    let blockquote_item = MenuItem::with_id(handle, "editor_blockquote", get_label(lang, "format_blockquote"), true, None::<&str>)?;
    let code_block_item = MenuItem::with_id(handle, "editor_code_block", get_label(lang, "format_code_block"), true, None::<&str>)?;
    let horizontal_rule_item = MenuItem::with_id(handle, "editor_horizontal_rule", get_label(lang, "format_horizontal_rule"), true, None::<&str>)?;

    let text_menu = Submenu::with_items(
        handle,
        get_label(lang, "format_text"),
        true,
        &[&bold_item, &italic_item, &strike_item, &inline_code_item],
    )?;
    let heading_menu = Submenu::with_items(
        handle,
        get_label(lang, "format_headings"),
        true,
        &[&paragraph_item, &heading_1_item, &heading_2_item, &heading_3_item, &heading_4_item, &heading_5_item, &heading_6_item],
    )?;
    let list_menu = Submenu::with_items(
        handle,
        get_label(lang, "format_lists"),
        true,
        &[&bullet_list_item, &ordered_list_item],
    )?;
    let block_menu = Submenu::with_items(
        handle,
        get_label(lang, "format_blocks"),
        true,
        &[&blockquote_item, &code_block_item, &horizontal_rule_item],
    )?;
    let format_menu = Submenu::with_items(
        handle,
        get_label(lang, "format"),
        true,
        &[&text_menu, &heading_menu, &list_menu, &block_menu],
    )?;
    menu.append(&format_menu)?;

    // View Menu
    let theme_github_light = MenuItem::with_id(handle, "view_theme_github_light", "GitHub Light", true, None::<&str>)?;
    let theme_solarized_light = MenuItem::with_id(handle, "view_theme_solarized_light", "Solarized Light", true, None::<&str>)?;
    let theme_dracula = MenuItem::with_id(handle, "view_theme_dracula", "Dracula", true, None::<&str>)?;
    let theme_nord = MenuItem::with_id(handle, "view_theme_nord", "Nord", true, None::<&str>)?;
    let theme_one_dark_pro = MenuItem::with_id(handle, "view_theme_one_dark_pro", "One Dark Pro", true, None::<&str>)?;
    let theme_tokyo_night = MenuItem::with_id(handle, "view_theme_tokyo_night", "Tokyo Night", true, None::<&str>)?;
    let theme_gruvbox = MenuItem::with_id(handle, "view_theme_gruvbox", "Gruvbox", true, None::<&str>)?;
    let theme_menu = Submenu::with_items(
        handle,
        get_label(lang, "view_theme"),
        true,
        &[&theme_github_light, &theme_solarized_light, &theme_dracula, &theme_nord, &theme_one_dark_pro, &theme_tokyo_night, &theme_gruvbox],
    )?;

    let lang_en_item = CheckMenuItem::with_id(handle, "lang_en", get_label(lang, "lang_en"), true, lang == "en", None::<&str>)?;
    let lang_zh_item = CheckMenuItem::with_id(handle, "lang_zh", get_label(lang, "lang_zh"), true, lang == "zh", None::<&str>)?;
    let language_menu = Submenu::with_items(
        handle,
        get_label(lang, "view_language"),
        true,
        &[&lang_en_item, &lang_zh_item],
    )?;

    let source_code_item = CheckMenuItem::with_id(
        handle,
        "view_source_code",
        get_label(lang, "view_source_code"),
        true,
        false,
        Some("CmdOrCtrl+Alt+S"),
    )?;

    // Preferences lives in the macOS app-name submenu above (platform
    // convention); that submenu doesn't exist on Windows/Linux, so give it a
    // home in the View menu there instead, keeping the same accelerator.
    #[cfg(not(target_os = "macos"))]
    let preferences_view_item = MenuItem::with_id(handle, "app_preferences", get_label(lang, "app_preferences"), true, Some("CmdOrCtrl+Comma"))?;

    #[cfg(not(target_os = "macos"))]
    let view_menu = Submenu::with_items(
        handle,
        get_label(lang, "view"),
        true,
        &[
            &source_code_item,
            &PredefinedMenuItem::separator(handle)?,
            &theme_menu,
            &language_menu,
            &PredefinedMenuItem::separator(handle)?,
            &preferences_view_item,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let view_menu = Submenu::with_items(
        handle,
        get_label(lang, "view"),
        true,
        &[
            &source_code_item,
            &PredefinedMenuItem::separator(handle)?,
            &theme_menu,
            &language_menu,
        ],
    )?;
    menu.append(&view_menu)?;

    // Window Menu
    let window_menu = Submenu::with_items(
        handle,
        get_label(lang, "window"),
        true,
        &[
            &PredefinedMenuItem::minimize(handle, Some(&get_label(lang, "window_minimize")))?,
            &PredefinedMenuItem::maximize(handle, Some(&get_label(lang, "window_zoom")))?,
            &PredefinedMenuItem::fullscreen(handle, Some(&get_label(lang, "window_fullscreen")))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, Some(&get_label(lang, "window_close")))?,
        ],
    )?;
    menu.append(&window_menu)?;

    // Help Menu — carries the About item on Windows/Linux; macOS already has
    // one in its app-name submenu above (platform convention), so it's
    // omitted here to avoid a redundant second About entry on macOS.
    #[cfg(not(target_os = "macos"))]
    let help_menu = Submenu::with_items(
        handle,
        get_label(lang, "help"),
        true,
        &[&PredefinedMenuItem::about(handle, Some(&get_label(lang, "app_about")), Some(about_metadata()))?],
    )?;
    #[cfg(target_os = "macos")]
    let help_menu = Submenu::with_items(
        handle,
        get_label(lang, "help"),
        true,
        &[],
    )?;
    menu.append(&help_menu)?;

    Ok(menu)
}

fn main() {
    // Language initialization priority (Tauri v2 best practice):
    // 1. Load from persistent storage (user saved preference)
    // 2. Fall back to system locale
    // 3. Default to English
    
    let default_language = match UserSettings::load() {
        Ok(Some(settings)) => {
            debug_log!("✅ User settings loaded from storage: language={}", settings.language);
            settings.language
        }
        Ok(None) => {
            // First launch — no saved preference; detect system locale
            match tauri_plugin_os::locale() {
                Some(locale_str) => {
                    let normalized = normalize_language(&locale_str);
                    debug_log!("🌍 No saved preference; using system locale: {} → normalized to: {}",
                             locale_str, normalized);
                    normalized
                }
                None => {
                    debug_log!("⚠️ System locale not available, using default: English");
                    "en".to_string()
                }
            }
        }
        Err(e) => {
            debug_log!("⚠️ Failed to load user settings: {}", e);
            // Settings file is corrupt or unreadable; fall back to system locale
            match tauri_plugin_os::locale() {
                Some(locale_str) => {
                    let normalized = normalize_language(&locale_str);
                    debug_log!("🌍 Falling back to system locale: {} → normalized to: {}",
                             locale_str, normalized);
                    normalized
                }
                None => {
                    debug_log!("⚠️ System locale not available, using default: English");
                    "en".to_string()
                }
            }
        }
    };
    
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let paths = collect_open_paths(argv);
            queue_open_files(app, paths);
        }))
        .manage(AppState::new(default_language.clone(), false))
        .setup(|app| {
            let args = std::env::args().skip(1).collect::<Vec<_>>();
            let paths = collect_open_paths(args);
            queue_open_files(app.handle(), paths);

            // Prune stale import-media staging dirs from any previous run.
            // Documents aren't restored across restarts, so anything left
            // under `imports/` at startup is necessarily orphaned.
            if let Ok(root) = imports_root_dir() {
                if let Ok(entries) = fs::read_dir(&root) {
                    for entry in entries.flatten() {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }

            Ok(())
        })
        .menu(move |handle| {
            // Menu starts with detected system language
            create_app_menu(handle, &default_language)
        })
        .on_menu_event(|app, event| {
            // ... (rest of menu event handler remains the same)
            if event.id() == "file_new" {
                let _ = app.emit("menu-new-file", ());
            } else if event.id() == "file_open" {
                let _ = app.emit("menu-open-file", ());
            } else if event.id() == "file_save" {
                let _ = app.emit("menu-save-file", ());
            } else if event.id() == "file_save_as" {
                let _ = app.emit("menu-save-as", ());
            } else if event.id() == "file_close_document" {
                let _ = app.emit("menu-close-document", ());
            } else if event.id() == "app_preferences" {
                let _ = app.emit("menu-open-preferences", ());
            } else if event.id() == "view_source_code" {
                // Toggle source_mode state in AppState
                let new_mode = if let Ok(mut sm) = app.state::<AppState>().source_mode.lock() {
                    *sm = !*sm;
                    *sm
                } else {
                    false
                };
                // Explicitly confirm the checkmark state (do not rely solely on macOS auto-toggle)
                if let Some(menu) = app.menu() {
                    if let Some(item) = menu.get("view_source_code") {
                        if let Some(check_item) = item.as_check_menuitem() {
                            let _ = check_item.set_checked(new_mode);
                        }
                    }
                }
                let _ = app.emit("menu-toggle-editor-mode", ());
            } else if event.id() == "view_theme_github_light" {
                let _ = app.emit("menu-set-theme", "github-light");
            } else if event.id() == "view_theme_dracula" {
                let _ = app.emit("menu-set-theme", "dracula");
            } else if event.id() == "view_theme_nord" {
                let _ = app.emit("menu-set-theme", "nord");
            } else if event.id() == "view_theme_solarized_light" {
                let _ = app.emit("menu-set-theme", "solarized-light");
            } else if event.id() == "view_theme_one_dark_pro" {
                let _ = app.emit("menu-set-theme", "one-dark-pro");
            } else if event.id() == "view_theme_tokyo_night" {
                let _ = app.emit("menu-set-theme", "tokyo-night");
            } else if event.id() == "view_theme_gruvbox" {
                let _ = app.emit("menu-set-theme", "gruvbox");
            } else if event.id() == "lang_en" {
                debug_log!("🌐 User selected: English");
                // Save preference to persistent storage
                if let Err(e) = save_language_to_storage("en") {
                    debug_log!("❌ Failed to save language preference: {}", e);
                }
                // Update menu directly
                if let Ok(menu) = create_app_menu(app, "en") {
                    let _ = app.set_menu(menu);
                }
                // Re-sync view_source_code checkmark with current source_mode state
                if let Ok(sm) = app.state::<AppState>().source_mode.lock() {
                    if let Some(menu) = app.menu() {
                        if let Some(item) = menu.get("view_source_code") {
                            if let Some(check_item) = item.as_check_menuitem() {
                                let _ = check_item.set_checked(*sm);
                            }
                        }
                    }
                }
                // Update backend state
                if let Ok(mut lang) = app.state::<AppState>().language.lock() {
                    *lang = "en".to_string();
                }
                // Notify frontend about the language change
                let _ = app.emit("language-changed", "en");
                debug_log!("✅ Language changed to: English");
            } else if event.id() == "lang_zh" {
                debug_log!("🌐 User selected: Chinese");
                // Save preference to persistent storage
                if let Err(e) = save_language_to_storage("zh") {
                    debug_log!("❌ Failed to save language preference: {}", e);
                }
                // Update menu directly
                if let Ok(menu) = create_app_menu(app, "zh") {
                    let _ = app.set_menu(menu);
                }
                // Re-sync view_source_code checkmark with current source_mode state
                if let Ok(sm) = app.state::<AppState>().source_mode.lock() {
                    if let Some(menu) = app.menu() {
                        if let Some(item) = menu.get("view_source_code") {
                            if let Some(check_item) = item.as_check_menuitem() {
                                let _ = check_item.set_checked(*sm);
                            }
                        }
                    }
                }
                // Update backend state
                if let Ok(mut lang) = app.state::<AppState>().language.lock() {
                    *lang = "zh".to_string();
                }
                // Notify frontend about the language change
                let _ = app.emit("language-changed", "zh");
                debug_log!("✅ Language changed to: Chinese");
            } else if event.id() == "editor_bold" {
                emit_editor_command(app, "bold", None);
            } else if event.id() == "editor_italic" {
                emit_editor_command(app, "italic", None);
            } else if event.id() == "editor_strike" {
                emit_editor_command(app, "strike", None);
            } else if event.id() == "editor_inline_code" {
                emit_editor_command(app, "inline_code", None);
            } else if event.id() == "editor_paragraph" {
                emit_editor_command(app, "paragraph", None);
            } else if event.id() == "editor_heading_1" {
                emit_editor_command(app, "heading", Some(1));
            } else if event.id() == "editor_heading_2" {
                emit_editor_command(app, "heading", Some(2));
            } else if event.id() == "editor_heading_3" {
                emit_editor_command(app, "heading", Some(3));
            } else if event.id() == "editor_heading_4" {
                emit_editor_command(app, "heading", Some(4));
            } else if event.id() == "editor_heading_5" {
                emit_editor_command(app, "heading", Some(5));
            } else if event.id() == "editor_heading_6" {
                emit_editor_command(app, "heading", Some(6));
            } else if event.id() == "editor_bullet_list" {
                emit_editor_command(app, "bullet_list", None);
            } else if event.id() == "editor_ordered_list" {
                emit_editor_command(app, "ordered_list", None);
            } else if event.id() == "editor_blockquote" {
                emit_editor_command(app, "blockquote", None);
            } else if event.id() == "editor_code_block" {
                emit_editor_command(app, "code_block", None);
            } else if event.id() == "editor_horizontal_rule" {
                emit_editor_command(app, "horizontal_rule", None);
            } else if event.id() == "edit_find" {
                let _ = app.emit("menu-find", ());
            } else if event.id() == "edit_find_in_files" {
                let _ = app.emit("menu-find-in-files", ());
            } else if event.id().0.starts_with("file_import_") {
                let fmt = event.id().0.strip_prefix("file_import_").unwrap_or("").to_string();
                let _ = app.emit("menu-import", fmt);
            } else if event.id().0.starts_with("file_export_") {
                let fmt = event.id().0.strip_prefix("file_export_").unwrap_or("").to_string();
                let _ = app.emit("menu-export", fmt);
            }
        })
        .invoke_handler(tauri::generate_handler![
            read_markdown_file,
            save_markdown_file,
            list_directory,
            get_recent_files,
            add_recent_file,
            create_file,
            delete_file,
            rename_file,
            file_exists,
            update_menu_item_state,
            enable_menu_item,
            import_document,
            export_document,
            relocate_media,
            discard_media,
            take_pending_open_files,
            get_os_platform,
            get_system_locale,
            get_language,
            set_language,
            get_user_settings,
            save_language_preference,
            search_in_files,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Ready = event {
            // Emit platform info as soon as the app is ready
            let platform = if cfg!(target_os = "macos") {
                "macos"
            } else if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "linux") {
                "gnome"
            } else {
                "unknown"
            };
            let _ = app_handle.emit("init-platform", platform);
        }

        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = event {
            let paths: Vec<String> = urls
                .into_iter()
                .filter_map(|url| {
                    if url.scheme() == "file" {
                        url.to_file_path()
                            .ok()
                            .map(|p| p.to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            queue_open_files(app_handle, paths);
        }
    });
}
