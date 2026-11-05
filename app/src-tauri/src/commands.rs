//! Functions the UI calls through `invoke(...)`.

use crate::manager::{AddRequest, DownloadManager, Incoming, Item, category_for, to_header_map};
use crate::settings::Settings;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

type Mgr<'a> = State<'a, Arc<DownloadManager>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    items: Vec<Item>,
    settings: Settings,
    pending: Vec<Incoming>,
    version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    filename: String,
    size: Option<u64>,
    resumable: bool,
    category: String,
    dir: PathBuf,
    stream_kind: Option<crate::media::StreamKind>,
}

#[tauri::command]
pub fn get_state(m: Mgr) -> AppState {
    AppState { items: m.snapshot(), settings: m.settings(), pending: m.take_pending(), version: env!("CARGO_PKG_VERSION") }
}

#[tauri::command]
pub async fn probe_url(m: Mgr<'_>, url: String, headers: Vec<(String, String)>) -> Result<ProbeInfo, String> {
    let extensionless = tauri::Url::parse(url.trim()).ok().is_some_and(|u| matches!(u.scheme(), "http" | "https") && std::path::Path::new(u.path()).extension().is_none());
    if crate::media::is_hls_url(&url) || (extensionless && crate::hls::detect(url.trim(), &headers).await.unwrap_or(false)) {
        return Ok(ProbeInfo { filename: "video.mkv".into(), size: None, resumable: true, category: "Video".into(), dir: m.default_dir("Video"), stream_kind: Some(crate::media::StreamKind::Hls) });
    }
    let remote = swiftdl_engine::probe(m.client(), url.trim(), &to_header_map(&headers)).await.map_err(|e| format!("{e:#}"))?;
    let category = category_for(&remote.filename).to_string();
    Ok(ProbeInfo {
        dir: m.default_dir(&category),
        filename: remote.filename,
        size: remote.size,
        resumable: remote.resumable,
        category,
        stream_kind: None,
    })
}

#[tauri::command]
pub fn default_dir(m: Mgr, filename: String) -> PathBuf {
    m.default_dir(category_for(&filename))
}

#[tauri::command]
pub fn add_download(m: Mgr, req: AddRequest) -> Result<u64, String> {
    check_add(&req)?;
    Ok(m.add(req))
}

/// What a download may come from: web links, and for torrents also magnet
/// links and .torrent files on this computer.
fn check_add(req: &AddRequest) -> Result<(), String> {
    let url = req.url.trim();
    let web = url.starts_with("http://") || url.starts_with("https://");
    let torrent = req.torrent_files.is_some() && (url.starts_with("magnet:") || std::path::Path::new(url).is_file());
    if !web && !torrent {
        return Err("Only web links, magnet links and .torrent files are supported".into());
    }
    if req.filename.trim().is_empty() {
        return Err("File name is empty".into());
    }
    if req.stream_kind == Some(crate::media::StreamKind::Hls) && (req.format.is_none() || !req.filename.to_ascii_lowercase().ends_with(".mkv")) {
        return Err("HLS downloads need a selected format and a .mkv destination".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str, torrent: bool) -> AddRequest {
        serde_json::from_value(serde_json::json!({
            "url": url, "filename": "name", "dir": "C:/Downloads", "start": true,
            "torrentFiles": if torrent { serde_json::json!([]) } else { serde_json::Value::Null },
        }))
        .unwrap()
    }

    #[test]
    fn accepts_every_kind_of_source() {
        assert!(check_add(&req("https://example.com/a.iso", false)).is_ok());
        assert!(check_add(&req("magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c", true)).is_ok());
        assert!(check_add(&req("https://example.com/a.torrent", true)).is_ok());
        let file = std::env::temp_dir().join("swiftdl-test.torrent");
        std::fs::write(&file, b"d4:infod4:name1:aee").unwrap();
        assert!(check_add(&req(&file.to_string_lossy(), true)).is_ok(), "a .torrent file on disk");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn refuses_other_sources() {
        assert!(check_add(&req("magnet:?xt=urn:btih:abc", false)).is_err(), "magnet only as a torrent");
        assert!(check_add(&req("file:///etc/passwd", false)).is_err());
        assert!(check_add(&req("C:/not/there.torrent", true)).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launcher_keeps_host_paths_but_not_bundled_paths() {
        let app_dir = std::path::Path::new("/tmp/.mount_SwiftDL");
        let value = std::ffi::OsStr::new("/tmp/.mount_SwiftDL/usr/lib:/usr/lib:/home/user/My Apps:/tmp/.mount_SwiftDL-other/lib");
        assert_eq!(host_paths(value, app_dir).unwrap(), "/usr/lib:/home/user/My Apps:/tmp/.mount_SwiftDL-other/lib");
        assert!(host_paths(std::ffi::OsStr::new("/tmp/.mount_SwiftDL/usr/lib"), app_dir).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_open_grants_only_owner_execute_permission() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("swiftdl-open-{}.AppImage", std::process::id()));
        std::fs::write(&path, b"\x7fELF\x02\x01\x01\x00AI\x02").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        prepare_appimage(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o740);
        std::fs::write(&path, b"not an AppImage").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(prepare_appimage(&path).is_err());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn default_file_opener_passes_paths_and_reports_failures() {
        let path = std::path::Path::new("/Downloads/a video (1080p).mp4");
        // With sh -c the first argument after the script is $0. A path must
        // remain one literal argument, even with spaces and parentheses.
        assert!(open_with_launchers(path, None, &[("sh", &["-c", "test \"$0\" = '/Downloads/a video (1080p).mp4'"])]).is_ok());
        assert!(open_with_launchers(path, None, &[("false", &[]), ("true", &[])]).is_ok());
        let error = open_with_launchers(path, None, &[("sh", &["-c", "echo 'No default video player' >&2; exit 1"])]).unwrap_err();
        assert!(error.contains("No default video player"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launch_reports_immediate_appimage_runtime_errors() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo 'AppImages require FUSE to run' >&2; exit 1"]);
        assert!(launch_checked(&mut cmd).unwrap_err().contains("AppImages require FUSE"));
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "exit 2"]);
        assert!(launch_checked(&mut cmd).unwrap_err().contains("exit status: 2"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launch_does_not_wait_for_inherited_stderr_to_close() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", "sleep 5 >&2 & exit 0"]);
            let _ = tx.send(launch_checked(&mut cmd));
        });
        rx.recv_timeout(std::time::Duration::from_secs(3)).expect("waited for descendant stderr EOF").unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launch_detaches_without_retrying_when_player_later_exits_nonzero() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = open_with_launchers(std::path::Path::new("/Downloads/video.mp4"), None, &[
                ("sh", &["-c", "sleep 5; exit 1"]),
                ("sh", &["-c", "echo 'unwanted fallback' >&2; exit 1"]),
            ]);
            let _ = tx.send(result);
        });
        rx.recv_timeout(std::time::Duration::from_secs(3)).expect("waited for player exit").unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launch_drains_noisy_stderr_without_unbounded_diagnostics() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "head -c 100000 /dev/zero >&2; echo 'startup failure' >&2; exit 1"]);
        let error = launch_checked(&mut cmd).unwrap_err();
        assert!(error.contains("startup failure"));
        assert!(error.len() <= 8192);
    }
}

#[tauri::command]
pub fn pause(m: Mgr, ids: Vec<u64>) {
    m.pause(&ids);
}

#[tauri::command]
pub fn resume(m: Mgr, ids: Vec<u64>) {
    m.resume(&ids);
}

#[tauri::command]
pub fn remove(m: Mgr, ids: Vec<u64>, delete_files: bool) {
    m.remove(&ids, delete_files);
}

#[tauri::command]
pub async fn open_file(m: Mgr<'_>, id: u64) -> Result<(), String> {
    let item = m.item(id).ok_or("not found")?;
    let path = std::fs::canonicalize(&item.path)
        .map_err(|e| format!("Cannot open {}: {e}", item.path.display()))?;
    #[cfg(target_os = "linux")]
    {
        tauri::async_runtime::spawn_blocking(move || open_linux_file(&path))
            .await.map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        m.app_handle().opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "linux")]
fn open_linux_file(path: &std::path::Path) -> Result<(), String> {
    let app_dir = std::env::var_os("APPDIR").map(PathBuf::from);
    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("AppImage")) {
        prepare_appimage(path).map_err(|e| format!("Cannot launch {}: {e}", path.display()))?;
        let mut cmd = std::process::Command::new(path);
        if let Some(dir) = &app_dir {
            clean_appimage_environment(&mut cmd, dir);
        }
        // These describe SwiftDL's own bundle, not the app being launched.
        cmd.env_remove("APPIMAGE").env_remove("APPDIR").env_remove("ARGV0");
        return launch_checked(&mut cmd)
            .map_err(|e| format!("Cannot launch {}: {e}", path.display()));
    }
    open_with_launchers(path, app_dir.as_deref(), &[("xdg-open", &[]), ("gio", &["open"])])
}

#[cfg(target_os = "linux")]
fn open_with_launchers(path: &std::path::Path, app_dir: Option<&std::path::Path>, launchers: &[(&str, &[&str])]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (program, args) in launchers {
        let mut cmd = std::process::Command::new(program);
        cmd.args(*args).arg(path);
        if let Some(dir) = app_dir {
            clean_appimage_environment(&mut cmd, dir);
        }
        match launch_checked(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(e) => errors.push(format!("{program}: {e}")),
        }
    }
    Err(format!("Could not open {} with its default application. {}", path.display(), errors.join("; ")))
}

/// Report early startup failures, but never wait for a desktop app to close.
/// Keep draining stderr after returning so inherited pipes cannot block it.
#[cfg(target_os = "linux")]
fn launch_checked(cmd: &mut std::process::Command) -> Result<(), String> {
    use std::io::Read;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let (mut errors, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
    errors.set_nonblocking(true).map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(OwnedFd::from(writer)));
    let mut child = Some(cmd.spawn().map_err(|e| e.to_string())?);
    // Command retains its configured descriptor; close that extra writer too.
    cmd.stderr(Stdio::null());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(750);
        let mut reply = Some(tx);
        let mut diagnostic = std::collections::VecDeque::<u8>::new();
        let mut eof = false;
        loop {
            // Bound both retained diagnostics and work per tick, even for a
            // noisy process. No read or EOF wait may delay the startup reply.
            for _ in 0..64 {
                let mut buf = [0; 1024];
                match errors.read(&mut buf) {
                    Ok(0) => { eof = true; break; }
                    Ok(n) if reply.is_some() => {
                        diagnostic.extend(&buf[..n]);
                        while diagnostic.len() > 8192 { diagnostic.pop_front(); }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => { eof = true; break; }
                }
            }
            if let Some(process) = child.as_mut() {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        child = None;
                        if let Some(tx) = reply.take() {
                            // Read once more to capture errors written
                            // between the drain above and process termination.
                            if !status.success() {
                                for _ in 0..256 {
                                    let mut tail = [0; 1024];
                                    match errors.read(&mut tail) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            diagnostic.extend(&tail[..n]);
                                            while diagnostic.len() > 8192 { diagnostic.pop_front(); }
                                        }
                                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                                        Err(_) => break,
                                    }
                                }
                                let text = String::from_utf8_lossy(&diagnostic.into_iter().collect::<Vec<_>>()).trim().to_string();
                                let _ = tx.send(Err(if text.is_empty() { format!("Process exited with {status}") } else { text }));
                                return;
                            }
                            let _ = tx.send(Ok(()));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if let Some(tx) = reply.take() { let _ = tx.send(Err(e.to_string())); }
                        let _ = process.kill();
                        let _ = process.wait();
                        return;
                    }
                }
            }
            if Instant::now() >= deadline {
                if let Some(tx) = reply.take() { let _ = tx.send(Ok(())); }
            }
            if child.is_none() && eof { return; }
            std::thread::sleep(Duration::from_millis(if reply.is_some() { 10 } else { 100 }));
        }
    });
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(target_os = "linux")]
fn prepare_appimage(path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let mut header = [0; 11];
    std::fs::File::open(path)?.read_exact(&mut header)?;
    if &header[..4] != b"\x7fELF" || &header[8..10] != b"AI" || !matches!(header[10], 1 | 2) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "The downloaded file is not a valid AppImage"));
    }
    let mut permissions = std::fs::metadata(path)?.permissions();
    // Only grant the owner execute permission, and only after an Open click.
    permissions.set_mode(permissions.mode() | 0o100);
    std::fs::set_permissions(path, permissions)
}

// Bundled libraries/plugins are for SwiftDL, not the user's media player.
// Revealing a file uses D-Bus, so it doesn't suffer from this inherited env.
#[cfg(target_os = "linux")]
fn clean_appimage_environment(cmd: &mut std::process::Command, app_dir: &std::path::Path) {
    for key in ["PATH", "LD_LIBRARY_PATH", "LD_PRELOAD", "XDG_DATA_DIRS"] {
        if let Some(value) = std::env::var_os(key) {
            if let Some(value) = host_paths(&value, app_dir) {
                cmd.env(key, value);
            } else {
                cmd.env_remove(key);
            }
        }
    }
    for key in ["GIO_MODULE_DIR", "GIO_EXTRA_MODULES", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR", "GTK_PATH", "GTK_EXE_PREFIX", "GTK_DATA_PREFIX", "QT_PLUGIN_PATH", "QT_QPA_PLATFORM_PLUGIN_PATH"] {
        if std::env::var_os(key).is_some_and(|v| std::env::split_paths(&v).any(|p| p.starts_with(app_dir))) {
            cmd.env_remove(key);
        }
    }
}

#[cfg(target_os = "linux")]
fn host_paths(value: &std::ffi::OsStr, app_dir: &std::path::Path) -> Option<std::ffi::OsString> {
    let paths: Vec<_> = std::env::split_paths(value).filter(|p| !p.starts_with(app_dir)).collect();
    if paths.is_empty() { None } else { std::env::join_paths(paths).ok() }
}

#[tauri::command]
pub fn open_folder(m: Mgr, id: u64) -> Result<(), String> {
    let item = m.item(id).ok_or("not found")?;
    let target = if item.path.exists() { item.path.clone() } else { swiftdl_engine::part_path(&item.path) };
    if target.exists() {
        m.app_handle().opener().reveal_item_in_dir(target).map_err(|e| e.to_string())
    } else {
        let dir = item.path.parent().map(PathBuf::from).unwrap_or_default();
        m.app_handle().opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn set_settings(m: Mgr, settings: Settings) {
    m.set_settings(settings);
}

#[tauri::command]
pub fn open_url(m: Mgr, url: String) -> Result<(), String> {
    m.app_handle().opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn extension_dir(m: Mgr) -> Option<PathBuf> {
    crate::browser::bundled_extension_dir(m.app_handle())
}

#[tauri::command]
pub fn open_extension_folder(m: Mgr) -> Result<(), String> {
    let dir = crate::browser::bundled_extension_dir(m.app_handle()).ok_or("extension folder not found")?;
    m.app_handle().opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_item(m: Mgr, id: u64) -> Option<Item> {
    m.item(id)
}

/// The browser download a "New download" window was opened for.
#[tauri::command]
pub fn confirm_info(m: Mgr, token: u64) -> Option<Incoming> {
    m.confirm_info(token)
}

#[tauri::command]
pub fn forget_confirm(m: Mgr, token: u64) {
    m.forget_confirm(token);
}

#[tauri::command]
pub fn show_main(m: Mgr) {
    crate::show_main_window(m.app_handle());
}

#[tauri::command]
pub fn integration_status() -> crate::browser::IntegrationStatus {
    crate::browser::status()
}

#[tauri::command]
pub fn open_store(m: Mgr, browser: String) -> Result<(), String> {
    crate::browser::open_store_in(m.app_handle(), &browser).map_err(|e| format!("{e:#}"))
}

/// Runs off the main thread (async command), so the blocking picker is safe.
#[tauri::command]
pub async fn pick_folder(m: Mgr<'_>, start: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = m.app_handle().dialog().file();
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    Ok(dialog.blocking_pick_folder().and_then(|p| p.into_path().ok()))
}

#[tauri::command]
pub fn is_torrent_url(url: String, filename: Option<String>) -> bool {
    crate::torrent::is_torrent(&url, filename.as_deref())
}

#[tauri::command]
pub async fn probe_torrent(m: Mgr<'_>, url: String, headers: Vec<(String, String)>) -> Result<crate::torrent::TorrentInfo, String> {
    crate::torrent::probe(m.client(), &url, &to_header_map(&headers)).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn is_media_url(url: String) -> bool {
    crate::media::is_media_url(url.trim())
}

#[tauri::command]
pub async fn probe_media(m: Mgr<'_>, url: String, cookies: Option<String>, headers: Option<Vec<(String, String)>>, media_cookies: Option<Vec<crate::media::Cookie>>, stream_kind: Option<crate::media::StreamKind>) -> Result<crate::media::MediaInfo, String> {
    let mut headers = headers.unwrap_or_default();
    if let Some(cookies) = cookies.filter(|v| !v.is_empty()) {
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie")) { headers.push(("Cookie".into(), cookies)); }
    }
    crate::media::probe(m.client(), url.trim(), headers, media_cookies.unwrap_or_default(), stream_kind).await.map_err(|e| format!("{e:#}"))
}

/// A video-site link pasted into the main window: choose its quality in a
/// "New download" window, like a link caught from the browser.
#[tauri::command]
pub fn open_new_download(m: Mgr, url: String, stream_kind: Option<crate::media::StreamKind>, headers: Option<Vec<(String, String)>>) {
    let headers = headers.unwrap_or_default();
    let value = |name: &str| headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.clone());
    m.new_download_window(Incoming { url: url.trim().to_string(), filename: None, referrer: value("referer"), cookies: value("cookie"), user_agent: value("user-agent"), stream_kind, media_cookies: vec![] });
}
