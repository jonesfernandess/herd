use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl,
};

const DEFAULT_BROWSER_URL: &str = "https://example.com/";
const BROWSER_URL_EVENT: &str = "browser-url-changed";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWebviewState {
    pub current_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserUrlChangedEvent {
    pane_id: String,
    url: String,
    loading: bool,
}

fn browser_webview_label(pane_id: &str) -> String {
    let mut label = String::from("browser-pane-");
    for byte in pane_id.as_bytes() {
        label.push_str(&format!("{byte:02x}"));
    }
    label
}

fn hidden_browser_viewport() -> BrowserViewport {
    BrowserViewport {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        visible: false,
    }
}

fn parse_browser_url(raw: Option<&str>) -> Result<Url, String> {
    let value = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_BROWSER_URL);
    let parsed = Url::parse(value).map_err(|error| format!("invalid browser URL {value}: {error}"))?;
    match parsed.scheme() {
        "http" | "https" | "about" | "file" => Ok(parsed),
        scheme => Err(format!("unsupported browser URL scheme: {scheme}")),
    }
}

fn resolve_browser_file_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("browser load path cannot be empty".to_string());
    }

    let requested = PathBuf::from(trimmed);
    let absolute = if requested.is_absolute() {
        requested
    } else {
        crate::runtime::project_root_dir().join(requested)
    };
    let canonical = std::fs::canonicalize(&absolute)
        .map_err(|error| format!("failed to resolve browser path {}: {error}", absolute.display()))?;
    if !canonical.is_file() {
        return Err(format!("browser load path is not a file: {}", canonical.display()));
    }
    Ok(canonical)
}

fn resolve_browser_file_url(path: &str) -> Result<Url, String> {
    let canonical = resolve_browser_file_path(path)?;
    Url::from_file_path(&canonical)
        .map_err(|_| format!("failed to convert browser path to file URL: {}", canonical.display()))
}

fn browser_current_url(webview: &tauri::Webview) -> Result<String, String> {
    webview
        .url()
        .map(|url| url.to_string())
        .map_err(|error| format!("failed to read browser URL: {error}"))
}

pub fn current_url_for_pane(app: &tauri::AppHandle, pane_id: &str) -> Option<String> {
    let webview = get_browser_webview(app, pane_id)?;
    browser_current_url(&webview).ok()
}

fn emit_browser_url_changed(
    app: &tauri::AppHandle,
    pane_id: &str,
    url: String,
    loading: bool,
) {
    let _ = app.emit(
        BROWSER_URL_EVENT,
        BrowserUrlChangedEvent {
            pane_id: pane_id.to_string(),
            url,
            loading,
        },
    );
}

fn apply_browser_viewport(
    webview: &tauri::Webview,
    viewport: &BrowserViewport,
) -> Result<(), String> {
    if viewport.visible && viewport.width > 1.0 && viewport.height > 1.0 {
        webview
            .set_position(LogicalPosition::new(viewport.x, viewport.y))
            .map_err(|error| format!("failed to position browser webview: {error}"))?;
        webview
            .set_size(LogicalSize::new(viewport.width, viewport.height))
            .map_err(|error| format!("failed to resize browser webview: {error}"))?;
        webview
            .show()
            .map_err(|error| format!("failed to show browser webview: {error}"))?;
    } else {
        webview
            .hide()
            .map_err(|error| format!("failed to hide browser webview: {error}"))?;
    }

    Ok(())
}

fn get_browser_webview(app: &tauri::AppHandle, pane_id: &str) -> Option<tauri::Webview> {
    app.get_webview(&browser_webview_label(pane_id))
}

fn ensure_browser_webview(
    app: &tauri::AppHandle,
    pane_id: &str,
    initial_url: Option<&str>,
    viewport: &BrowserViewport,
) -> Result<tauri::Webview, String> {
    if let Some(existing) = get_browser_webview(app, pane_id) {
        return Ok(existing);
    }

    let label = browser_webview_label(pane_id);
    let start_url = parse_browser_url(initial_url)?;
    let main_window = app
        .get_window("main")
        .ok_or_else(|| "main window is not available".to_string())?;
    let app_handle = app.clone();
    let pane_id_for_event = pane_id.to_string();
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(start_url.clone())).on_page_load(
        move |_webview, payload| {
            let loading = matches!(payload.event(), PageLoadEvent::Started);
            emit_browser_url_changed(
                &app_handle,
                &pane_id_for_event,
                payload.url().to_string(),
                loading,
            );
        },
    );

    let webview = main_window
        .add_child(
            builder,
            LogicalPosition::new(viewport.x, viewport.y),
            LogicalSize::new(viewport.width.max(1.0), viewport.height.max(1.0)),
        )
        .map_err(|error| format!("failed to create browser webview: {error}"))?;
    webview
        .set_auto_resize(false)
        .map_err(|error| format!("failed to disable browser auto-resize: {error}"))?;
    emit_browser_url_changed(app, pane_id, start_url.to_string(), true);
    Ok(webview)
}

pub fn close_browser_webview(app: &tauri::AppHandle, pane_id: &str) {
    if let Some(webview) = get_browser_webview(app, pane_id) {
        let _ = webview.close();
    }
}

fn navigate_existing_browser_webview(
    app: &tauri::AppHandle,
    pane_id: &str,
    parsed: Url,
) -> Result<BrowserWebviewState, String> {
    let webview = get_browser_webview(app, pane_id)
        .ok_or_else(|| format!("browser webview not found for pane {pane_id}"))?;
    webview
        .navigate(parsed.clone())
        .map_err(|error| format!("failed to navigate browser webview: {error}"))?;
    emit_browser_url_changed(app, pane_id, parsed.to_string(), true);
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

pub fn navigate_browser_webview(
    app: &tauri::AppHandle,
    pane_id: &str,
    url: &str,
) -> Result<BrowserWebviewState, String> {
    let parsed = parse_browser_url(Some(url))?;
    if get_browser_webview(app, pane_id).is_some() {
        return navigate_existing_browser_webview(app, pane_id, parsed);
    }

    let hidden = hidden_browser_viewport();
    let webview = ensure_browser_webview(app, pane_id, Some(parsed.as_str()), &hidden)?;
    apply_browser_viewport(&webview, &hidden)?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

pub fn load_browser_webview(
    app: &tauri::AppHandle,
    pane_id: &str,
    path: &str,
) -> Result<BrowserWebviewState, String> {
    let parsed = resolve_browser_file_url(path)?;
    if get_browser_webview(app, pane_id).is_some() {
        return navigate_existing_browser_webview(app, pane_id, parsed);
    }

    let hidden = hidden_browser_viewport();
    let webview = ensure_browser_webview(app, pane_id, Some(parsed.as_str()), &hidden)?;
    apply_browser_viewport(&webview, &hidden)?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

#[tauri::command]
pub async fn browser_webview_sync(
    app: tauri::AppHandle,
    pane_id: String,
    initial_url: Option<String>,
    viewport: BrowserViewport,
) -> Result<BrowserWebviewState, String> {
    let webview = ensure_browser_webview(&app, &pane_id, initial_url.as_deref(), &viewport)?;
    apply_browser_viewport(&webview, &viewport)?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

#[tauri::command]
pub async fn browser_webview_navigate(
    app: tauri::AppHandle,
    pane_id: String,
    url: String,
) -> Result<BrowserWebviewState, String> {
    navigate_browser_webview(&app, &pane_id, &url)
}

#[tauri::command]
pub async fn browser_webview_reload(
    app: tauri::AppHandle,
    pane_id: String,
) -> Result<BrowserWebviewState, String> {
    let webview = get_browser_webview(&app, &pane_id)
        .ok_or_else(|| format!("browser webview not found for pane {pane_id}"))?;
    webview
        .reload()
        .map_err(|error| format!("failed to reload browser webview: {error}"))?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

#[tauri::command]
pub async fn browser_webview_back(
    app: tauri::AppHandle,
    pane_id: String,
) -> Result<BrowserWebviewState, String> {
    let webview = get_browser_webview(&app, &pane_id)
        .ok_or_else(|| format!("browser webview not found for pane {pane_id}"))?;
    webview
        .eval("window.history.back();")
        .map_err(|error| format!("failed to navigate browser history backward: {error}"))?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

#[tauri::command]
pub async fn browser_webview_forward(
    app: tauri::AppHandle,
    pane_id: String,
) -> Result<BrowserWebviewState, String> {
    let webview = get_browser_webview(&app, &pane_id)
        .ok_or_else(|| format!("browser webview not found for pane {pane_id}"))?;
    webview
        .eval("window.history.forward();")
        .map_err(|error| format!("failed to navigate browser history forward: {error}"))?;
    Ok(BrowserWebviewState {
        current_url: browser_current_url(&webview)?,
    })
}

#[tauri::command]
pub async fn browser_webview_hide(app: tauri::AppHandle, pane_id: String) -> Result<(), String> {
    if let Some(webview) = get_browser_webview(&app, &pane_id) {
        webview
            .hide()
            .map_err(|error| format!("failed to hide browser webview: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_browser_url, resolve_browser_file_url};

    #[test]
    fn allows_file_scheme_browser_urls() {
        let url = parse_browser_url(Some("file:///tmp/herd-browser-test.html")).unwrap();
        assert_eq!(url.scheme(), "file");
    }

    #[test]
    fn resolves_relative_browser_file_urls_from_project_root() {
        let url = resolve_browser_file_url("src-tauri/Cargo.toml").unwrap();
        assert_eq!(url.scheme(), "file");
        assert!(url.path().ends_with("/src-tauri/Cargo.toml"));
    }

    #[test]
    fn rejects_missing_browser_file_paths() {
        let error = resolve_browser_file_url("this-file-should-not-exist-3c7f5d84.html").unwrap_err();
        assert!(error.contains("failed to resolve browser path"));
    }
}
