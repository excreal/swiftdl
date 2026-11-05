//! The download list: queueing, persistence, live progress, and the bridge
//! between the engine and the UI. All state sits behind one mutex that is
//! never held across an `.await`.

use crate::settings::Settings;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use swiftdl_engine::header::{HeaderMap, HeaderName, HeaderValue};
use swiftdl_engine::{CancellationToken, Client, Outcome, Progress, Request, SpeedLimiter};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub downloaded: u64,
    pub status: Status,
    pub error: Option<String>,
    pub category: String,
    pub added_at: u64,
    pub completed_at: Option<u64>,
    pub resumable: bool,
    /// Cookie/Referer/User-Agent captured by the browser extension. Kept so
    /// resuming still works on sites that need a login; cleared on completion.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Missing byte ranges, for the connection map. Empty when complete or
    /// when the server can't do ranges.
    #[serde(default)]
    pub remaining: Vec<(u64, u64)>,
    /// Live `(start, pos, end)` per connection segment, for the download
    /// window's connection view.
    #[serde(default, skip_deserializing)]
    pub segments: Vec<(u64, u64, u64)>,
    /// Bytes per second, smoothed. Live only, not meaningful on disk.
    #[serde(default, skip_deserializing)]
    pub speed: f64,
    #[serde(default, skip_deserializing)]
    pub connections: usize,
    /// yt-dlp format for downloads from video sites (YouTube and others),
    /// which go through yt-dlp instead of the engine.
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub stream_kind: Option<crate::media::StreamKind>,
    #[serde(default)]
    pub media_cookies: Vec<crate::media::Cookie>,
    /// For a torrent or magnet link: the files chosen (empty = all). Its
    /// presence marks the download as a torrent.
    #[serde(default)]
    pub torrent_files: Option<Vec<usize>>,
    /// Live status text, such as "Merging video and audio".
    #[serde(default, skip_deserializing)]
    pub note: Option<String>,
}

/// A download sent by the browser extension, waiting for the user to
/// confirm it in the Add dialog.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incoming {
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub media_headers: Vec<(String, String)>,
    #[serde(default)]
    pub stream_kind: Option<crate::media::StreamKind>,
    #[serde(default)]
    pub media_cookies: Vec<crate::media::Cookie>,
}

impl Incoming {
    /// A usable file name sent by the extension (e.g. a video page's title),
    /// if it has an extension; otherwise the server's name is better.
    pub fn suggested_name(&self) -> Option<String> {
        let name: String = self.filename.as_deref()?.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
        let name = name.trim().trim_end_matches('.').to_string();
        Path::new(&name).extension().is_some().then_some(name)
    }

    pub fn headers(&self) -> Vec<(String, String)> {
        let mut h: Vec<(String, String)> = vec![];
        if let Some(c) = self.cookies.as_ref().filter(|c| !c.is_empty()) {
            h.push(("Cookie".into(), c.clone()));
        }
        if let Some(r) = self.referrer.as_ref().filter(|r| !r.is_empty()) {
            h.push(("Referer".into(), r.clone()));
        }
        if let Some(u) = self.user_agent.as_ref().filter(|u| !u.is_empty()) {
            h.push(("User-Agent".into(), u.clone()));
        }
        for (name, value) in &self.media_headers {
            if crate::media::is_session_header(name) {
                h.retain(|(key, _)| !key.eq_ignore_ascii_case(name));
                h.push((name.clone(), value.clone()));
            }
        }
        h
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    pub filename: String,
    pub dir: PathBuf,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// False adds it paused ("Download later").
    pub start: bool,
    /// Label of the window this was added from (the "New download"
    /// window), which then becomes the download's progress window.
    #[serde(default)]
    pub window: Option<String>,
    /// yt-dlp format, for a video-site download (see `Item::format`).
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub stream_kind: Option<crate::media::StreamKind>,
    #[serde(default)]
    pub media_cookies: Vec<crate::media::Cookie>,
    /// Expected size, when known up front (video qualities).
    #[serde(default)]
    pub size: Option<u64>,
    /// A torrent: the files to download (see `Item::torrent_files`).
    #[serde(default)]
    pub torrent_files: Option<Vec<usize>>,
}

struct Running {
    path: PathBuf,
    cancel: CancellationToken,
    progress: Arc<Progress>,
    last: (u64, Instant),
    speed: f64,
    note: crate::media::Note,
}

struct Inner {
    items: Vec<Item>,
    settings: Settings,
    running: HashMap<u64, Running>,
    pending: Vec<Incoming>,
    /// Downloads caught from the browser, waiting in a "New download" window.
    confirms: HashMap<u64, Incoming>,
    next_confirm: u64,
    /// Download id -> label of the window showing its progress.
    windows: HashMap<u64, String>,
    next_id: u64,
    dirty: bool,
}

pub struct DownloadManager {
    app: AppHandle,
    client: Client,
    limiter: Arc<SpeedLimiter>,
    data_dir: PathBuf,
    inner: Mutex<Inner>,
}

impl DownloadManager {
    pub fn new(app: AppHandle, data_dir: PathBuf, default_download_dir: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&data_dir)?;
        let mut settings: Settings = load_json(&data_dir.join("settings.json")).unwrap_or_default();
        if settings.download_dir.as_os_str().is_empty() {
            settings.download_dir = default_download_dir;
        }
        let mut items: Vec<Item> = load_json(&data_dir.join("downloads.json")).unwrap_or_default();
        for item in &mut items {
            // The app was closed mid-download: leave it paused, like IDM.
            if item.status == Status::Downloading {
                item.status = Status::Paused;
            }
            if item.status != Status::Completed && item.remaining.is_empty() {
                item.remaining = swiftdl_engine::saved_remaining(&item.path).unwrap_or_default();
            }
        }
        let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        Ok(Arc::new(Self {
            app,
            client: swiftdl_engine::build_client()?,
            limiter: Arc::new(SpeedLimiter::new(settings.speed_limit_kib * 1024)),
            data_dir,
            inner: Mutex::new(Inner {
                items,
                settings,
                running: HashMap::new(),
                pending: vec![],
                confirms: HashMap::new(),
                next_confirm: 1,
                windows: HashMap::new(),
                next_id,
                dirty: false,
            }),
        }))
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn app_handle(&self) -> &AppHandle {
        &self.app
    }

    pub fn settings(&self) -> Settings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn snapshot(&self) -> Vec<Item> {
        self.inner.lock().unwrap().items.clone()
    }

    /// Where a file of this category goes by default.
    pub fn default_dir(&self, category: &str) -> PathBuf {
        let s = self.settings();
        if s.sort_into_categories && category != "Other" {
            s.download_dir.join(category)
        } else {
            s.download_dir
        }
    }

    pub fn add(self: &Arc<Self>, req: AddRequest) -> u64 {
        let filename = req.filename.trim().to_string();
        let id = {
            let mut g = self.inner.lock().unwrap();
            // Allocate under the same lock that inserts the item. Pending and
            // paused jobs own their paths even before a final file exists.
            let path = available_dest(&req.dir, &filename, |path| {
                g.items.iter().any(|item| item.path == path)
                    || g.running.values().any(|job| job.path == path)
            });
            let id = g.next_id;
            g.next_id += 1;
            g.items.push(Item {
                id,
                url: req.url,
                filename: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(filename.clone()),
                category: category_for(&filename).to_string(),
                path,
                size: req.size,
                downloaded: 0,
                status: if req.start { Status::Queued } else { Status::Paused },
                error: None,
                added_at: now_ms(),
                completed_at: None,
                resumable: true,
                headers: req.headers,
                remaining: vec![],
                segments: vec![],
                speed: 0.0,
                connections: 0,
                format: req.format,
                stream_kind: req.stream_kind,
                media_cookies: req.media_cookies,
                torrent_files: req.torrent_files,
                note: None,
            });
            if let Some(label) = req.window {
                g.windows.insert(id, label);
            }
            id
        };
        self.changed();
        self.schedule();
        id
    }

    pub fn pause(self: &Arc<Self>, ids: &[u64]) {
        {
            let mut g = self.inner.lock().unwrap();
            for id in ids {
                if let Some(r) = g.running.get(id) {
                    r.cancel.cancel(); // the task marks it paused when it stops
                } else if let Some(item) = g.items.iter_mut().find(|i| i.id == *id && i.status == Status::Queued) {
                    item.status = Status::Paused;
                }
            }
        }
        self.changed();
    }

    pub fn resume(self: &Arc<Self>, ids: &[u64]) {
        {
            let mut g = self.inner.lock().unwrap();
            for item in g.items.iter_mut().filter(|i| ids.contains(&i.id)) {
                if matches!(item.status, Status::Paused | Status::Error) {
                    item.status = Status::Queued;
                    item.error = None;
                }
            }
        }
        self.changed();
        self.schedule();
    }

    pub fn remove(self: &Arc<Self>, ids: &[u64], delete_files: bool) {
        let removed: Vec<Item> = {
            let mut g = self.inner.lock().unwrap();
            for id in ids {
                if let Some(r) = g.running.get(id) {
                    r.cancel.cancel();
                }
            }
            let (gone, keep) = std::mem::take(&mut g.items).into_iter().partition(|i| ids.contains(&i.id));
            g.items = keep;
            gone
        };
        for item in &removed {
            // Running ones clean up their own temp files when their task ends.
            if item.status != Status::Downloading {
                remove_temp_files(&item.path);
            }
            if delete_files && item.status == Status::Completed {
                let _ = std::fs::remove_file(&item.path);
            }
        }
        self.changed();
    }

    pub fn set_settings(self: &Arc<Self>, settings: Settings) {
        self.limiter.set_rate(settings.speed_limit_kib * 1024);
        if let Err(e) = crate::browser::offer_extension(settings.offer_extension) {
            eprintln!("could not update browser extension offer: {e:#}");
        }
        // Open windows (download windows) follow the theme at once.
        let _ = self.app.emit("settings", &settings);
        self.inner.lock().unwrap().settings = settings;
        self.save();
        self.schedule();
    }

    pub fn item(&self, id: u64) -> Option<Item> {
        self.inner.lock().unwrap().items.iter().find(|i| i.id == id).cloned()
    }

    /// A download arrived from the browser extension. Like IDM, it gets its
    /// own small "New download" window rather than the main window.
    pub fn incoming(self: &Arc<Self>, inc: Incoming) {
        // Video sites always ask: there's a quality to choose.
        if self.settings().ask_before_download || inc.stream_kind.is_some() || crate::media::is_media_url(&inc.url) || crate::torrent::is_torrent(&inc.url, inc.filename.as_deref()) {
            self.new_download_window(inc);
            return;
        }
        // No dialog: probe for the real file name, then start right away.
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let headers = inc.headers();
            let probed = swiftdl_engine::probe(&this.client, &inc.url, &to_header_map(&headers)).await.ok().map(|r| r.filename);
            let filename = inc.suggested_name().or(probed).unwrap_or_else(|| "download.bin".into());
            let dir = this.default_dir(category_for(&filename));
            this.add(AddRequest { url: inc.url, filename, dir, headers, start: true, window: None, format: None, stream_kind: None, media_cookies: vec![], size: None, torrent_files: None });
        });
    }

    /// Opens a "New download" window for `inc`.
    pub fn new_download_window(&self, inc: Incoming) {
        let token = {
            let mut g = self.inner.lock().unwrap();
            let token = g.next_confirm;
            g.next_confirm += 1;
            g.confirms.insert(token, inc);
            token
        };
        if let Err(e) = crate::windows::open(&self.app, &format!("new-{token}"), &format!("confirm={token}"), "New download") {
            eprintln!("could not open the New download window: {e:#}");
        }
    }

    pub fn take_pending(&self) -> Vec<Incoming> {
        std::mem::take(&mut self.inner.lock().unwrap().pending)
    }

    /// What a "New download" window was opened for.
    pub fn confirm_info(&self, token: u64) -> Option<Incoming> {
        self.inner.lock().unwrap().confirms.get(&token).cloned()
    }

    pub fn forget_confirm(&self, token: u64) {
        self.inner.lock().unwrap().confirms.remove(&token);
    }

    /// Opens (or keeps) the progress window for each newly started
    /// download, unless the user turned that off.
    fn open_progress_windows(&self, ids: &[u64]) {
        use tauri::Manager as _;
        let to_open: Vec<(u64, String)> = {
            let mut g = self.inner.lock().unwrap();
            if !g.settings.show_progress_window {
                return;
            }
            let missing: Vec<u64> = ids
                .iter()
                .copied()
                .filter(|id| g.windows.get(id).is_none_or(|label| self.app.get_webview_window(label).is_none()))
                .collect();
            missing
                .into_iter()
                .map(|id| {
                    g.windows.insert(id, format!("dl-{id}"));
                    let title = g.items.iter().find(|i| i.id == id).map(|i| i.filename.clone()).unwrap_or_default();
                    (id, title)
                })
                .collect()
        };
        for (id, title) in to_open {
            if let Err(e) = crate::windows::open(&self.app, &format!("dl-{id}"), &format!("id={id}"), &title) {
                eprintln!("could not open the download window: {e:#}");
            }
        }
    }

    /// Starts queued downloads until `max_concurrent` are running.
    fn schedule(self: &Arc<Self>) {
        let to_start: Vec<(u64, Request, Kind, Arc<Progress>, crate::media::Note, CancellationToken)> = {
            let mut g = self.inner.lock().unwrap();
            let Inner { items, running, settings, .. } = &mut *g;
            let free = settings.max_concurrent.max(1).saturating_sub(running.len());
            let connections = settings.connections;
            items
                .iter_mut()
                .filter(|i| i.status == Status::Queued)
                .take(free)
                .map(|item| {
                    let id = item.id;
                    item.status = Status::Downloading;
                    let req = Request {
                        url: item.url.clone(),
                        dest: item.path.clone(),
                        connections,
                        headers: to_header_map(&item.headers),
                        restart: false,
                    };
                    let progress = Arc::new(Progress::default());
                    progress.downloaded.store(item.downloaded, Ordering::Relaxed);
                    progress.total.store(item.size.unwrap_or(0), Ordering::Relaxed);
                    let cancel = CancellationToken::new();
                    let note = crate::media::Note::default();
                    running.insert(
                        id,
                        Running {
                            path: item.path.clone(),
                            cancel: cancel.clone(),
                            progress: progress.clone(),
                            last: (item.downloaded, Instant::now()),
                            speed: 0.0,
                            note: note.clone(),
                        },
                    );
                    let kind = match (&item.torrent_files, &item.format) {
                        (Some(files), _) => Kind::Torrent(files.clone()),
                        (None, Some(format)) => Kind::Media(format.clone()),
                        (None, None) => Kind::File,
                    };
                    (id, req, kind, progress, note, cancel)
                })
                .collect()
        };
        if to_start.is_empty() {
            return;
        }
        self.open_progress_windows(&to_start.iter().map(|t| t.0).collect::<Vec<_>>());
        for (id, req, kind, progress, note, cancel) in to_start {
            let this = self.clone();
            tauri::async_runtime::spawn(async move {
                let result = match kind {
                    Kind::Media(format) => {
                        let item = this.item(id);
                        let headers = item.as_ref().map(|i| i.headers.clone()).unwrap_or_default();
                        let cookies = item.as_ref().map(|i| i.media_cookies.clone()).unwrap_or_default();
                        let stream_kind = item.and_then(|i| i.stream_kind);
                        let job = crate::media::Job { url: req.url.clone(), format, dest: req.dest.clone(), headers, cookies, stream_kind };
                        crate::media::run(&this.client, job, progress, note, cancel).await
                    }
                    Kind::Torrent(files) => {
                        // A torrent saves into a folder; `dest` is its name inside it.
                        let dir = req.dest.parent().map(Path::to_path_buf).unwrap_or_default();
                        let seed = crate::torrent::Seed::from_setting(&this.settings().seed_until);
                        let job = crate::torrent::Job { url: req.url.clone(), headers: req.headers.clone(), dir, files, seed };
                        crate::torrent::run(&this.client, job, progress, note, cancel).await
                    }
                    Kind::File => swiftdl_engine::run(&this.client, &req, progress, cancel, this.limiter.clone()).await,
                };
                this.finish(id, result);
            });
        }
        self.changed();
    }

    fn finish(self: &Arc<Self>, id: u64, result: Result<Outcome>) {
        let mut notify = None;
        {
            let mut g = self.inner.lock().unwrap();
            let running = g.running.remove(&id);
            let notify_enabled = g.settings.notify_on_complete;
            match g.items.iter_mut().find(|i| i.id == id) {
                None => {
                    // Removed while running: drop what it left behind.
                    if let Some(r) = &running {
                        remove_temp_files(&r.path);
                    }
                }
                Some(item) => {
                    if let Some(r) = &running {
                        item.downloaded = r.progress.downloaded.load(Ordering::Relaxed);
                        let total = r.progress.total.load(Ordering::Relaxed);
                        if total > 0 {
                            item.size = Some(total);
                        }
                        item.resumable = r.progress.resumable.load(Ordering::Relaxed);
                        item.remaining = r.progress.remaining.lock().unwrap().clone();
                        item.segments = r.progress.segments.lock().unwrap().clone();
                    }
                    item.speed = 0.0;
                    item.connections = 0;
                    item.note = None;
                    match result {
                        Ok(Outcome::Completed) => {
                            item.status = Status::Completed;
                            item.completed_at = Some(now_ms());
                            item.size = item.size.or(Some(item.downloaded));
                            item.downloaded = item.size.unwrap_or(item.downloaded);
                            item.headers.clear();
                            item.media_cookies.clear();
                            item.remaining.clear();
                            if notify_enabled {
                                notify = Some(item.filename.clone());
                            }
                        }
                        Ok(Outcome::Paused) => item.status = Status::Paused,
                        Err(e) => {
                            item.status = Status::Error;
                            item.error = Some(format!("{e:#}"));
                        }
                    }
                }
            }
        }
        if let Some(name) = notify {
            use tauri_plugin_notification::NotificationExt;
            let _ = self.app.notification().builder().title("Download complete").body(name).show();
        }
        self.changed();
        self.save();
        self.schedule();
    }

    /// Called every 500 ms: copies live progress into items, computes
    /// speeds, and pushes a snapshot to the UI when something changed.
    pub fn tick(&self, n: u64) {
        let (snapshot, active) = {
            let mut g = self.inner.lock().unwrap();
            let Inner { items, running, dirty, .. } = &mut *g;
            let now = Instant::now();
            for (id, r) in running.iter_mut() {
                let done = r.progress.downloaded.load(Ordering::Relaxed);
                let dt = now.duration_since(r.last.1).as_secs_f64();
                if dt > 0.0 {
                    let instant = done.saturating_sub(r.last.0) as f64 / dt;
                    r.speed = if r.speed == 0.0 { instant } else { r.speed * 0.6 + instant * 0.4 };
                }
                r.last = (done, now);
                if let Some(item) = items.iter_mut().find(|i| i.id == *id) {
                    item.downloaded = done;
                    let total = r.progress.total.load(Ordering::Relaxed);
                    if total > 0 {
                        item.size = Some(total);
                    }
                    item.speed = r.speed;
                    item.connections = r.progress.connections.load(Ordering::Relaxed);
                    item.remaining = r.progress.remaining.lock().unwrap().clone();
                    item.segments = r.progress.segments.lock().unwrap().clone();
                    item.note = r.note.lock().unwrap().clone();
                }
            }
            let active = !running.is_empty();
            if !active && !*dirty {
                return;
            }
            *dirty = false;
            (items.clone(), active)
        };
        let _ = self.app.emit("downloads", snapshot);
        if active && n % 10 == 0 {
            self.save();
        }
    }

    fn changed(&self) {
        self.inner.lock().unwrap().dirty = true;
    }

    pub fn save(&self) {
        let (items, settings) = {
            let g = self.inner.lock().unwrap();
            (g.items.clone(), g.settings.clone())
        };
        let _ = save_json(&self.data_dir.join("downloads.json"), &items);
        let _ = save_json(&self.data_dir.join("settings.json"), &settings);
    }

    pub fn start_ticker(self: &Arc<Self>) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            for n in 0u64.. {
                tick.tick().await;
                this.tick(n);
            }
        });
        self.schedule();
    }

}

/// How a download is fetched.
enum Kind {
    File,
    /// A video site, through yt-dlp, with this format.
    Media(String),
    /// A torrent or magnet link, with these files (empty = all).
    Torrent(Vec<usize>),
}

pub fn to_header_map(headers: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(k), Ok(v)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            map.insert(k, v);
        }
    }
    map
}

pub fn category_for(filename: &str) -> &'static str {
    let ext = Path::new(filename).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "iso" | "img" => "Compressed",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "txt" | "rtf" | "epub" | "csv" | "md" => {
            "Documents"
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" | "wma" => "Music",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg" | "3gp" | "ts" => "Video",
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "apk" | "appimage" | "msix" | "bat" | "jar" => "Programs",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "heic" | "avif" => "Images",
        _ => "Other",
    }
}

fn remove_temp_files(path: &Path) {
    crate::media::cleanup(path);
    let _ = std::fs::remove_file(swiftdl_engine::part_path(path));
    let mut state = path.as_os_str().to_owned();
    state.push(".swiftdl");
    let _ = std::fs::remove_file(PathBuf::from(state));
}

fn available_dest(dir: &Path, filename: &str, reserved: impl Fn(&Path) -> bool) -> PathBuf {
    let original = dir.join(filename);
    let stem = original.file_stem().unwrap_or_default().to_string_lossy();
    let ext = original.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for index in 0.. {
        let path = if index == 0 { original.clone() } else { original.with_file_name(format!("{stem} ({index}){ext}")) };
        let mut hls = path.as_os_str().to_owned();
        hls.push(".swiftdl-hls");
        let mut state = path.as_os_str().to_owned();
        state.push(".swiftdl");
        if !reserved(&path) && !path.exists() && !swiftdl_engine::part_path(&path).exists()
            && !Path::new(&hls).exists() && !Path::new(&state).exists() {
            return path;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod destination_tests {
    use super::*;

    #[test]
    fn incoming_media_headers_preserve_origin_and_override_guessed_referrer() {
        let incoming: Incoming = serde_json::from_value(serde_json::json!({
            "url": "https://cdn.test/video.m3u8", "referrer": "https://kwik.cx/e/fixture", "userAgent": "fallback",
            "mediaHeaders": [["Referer", "https://kwik.cx/"], ["Origin", "https://kwik.cx"], ["User-Agent", "observed"], ["Cookie", "must-not-copy"], ["Authorization", "must-not-copy"]]
        })).unwrap();
        let headers = incoming.headers();
        assert_eq!(headers.len(), 3);
        assert!(headers.contains(&("Referer".into(), "https://kwik.cx/".into())));
        assert!(headers.contains(&("Origin".into(), "https://kwik.cx".into())));
        assert!(headers.contains(&("User-Agent".into(), "observed".into())));
    }

    #[test]
    fn reserves_queued_running_and_partial_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let first = available_dest(dir.path(), "video.mkv", |_| false);
        let second = available_dest(dir.path(), "video.mkv", |p| p == first);
        assert_eq!(second.file_name().unwrap(), "video (1).mkv");
        std::fs::write(swiftdl_engine::part_path(&second), b"partial").unwrap();
        let hls = dir.path().join("video (2).mkv.swiftdl-hls");
        std::fs::create_dir(hls).unwrap();
        let third = available_dest(dir.path(), "video.mkv", |p| p == first);
        assert_eq!(third.file_name().unwrap(), "video (3).mkv");
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    Ok(())
}
