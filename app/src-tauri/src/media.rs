//! Video sites (YouTube and others), through yt-dlp.
//!
//! These sites don't serve a plain file. YouTube's player streams through a
//! private protocol with no download address, so SwiftDL hands these pages
//! to yt-dlp, which keeps up with such changes, and to ffmpeg, which joins
//! the separate video and audio streams. Both are fetched into the app's
//! data folder the first time they're needed, and yt-dlp is kept current.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime};
use swiftdl_engine::{CancellationToken, Client, Outcome, Progress};

/// Sites sent to yt-dlp instead of the regular engine.
const SITES: &[&str] = &[
    "youtube.com", "youtu.be", "youtube-nocookie.com", "vimeo.com", "x.com", "twitter.com", "reddit.com",
    "instagram.com", "tiktok.com", "facebook.com", "twitch.tv", "dailymotion.com", "soundcloud.com", "bilibili.com",
    "xvideos.com", "xnxx.com",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind { Hls }

/// Browser cookies retain their original scope. Never send a raw Cookie header
/// to every segment/key host through yt-dlp's global --add-headers option.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub host_only: bool,
    #[serde(default)]
    pub expiration_date: Option<f64>,
}

pub fn is_hls_url(url: &str) -> bool {
    tauri::Url::parse(url.trim()).ok().is_some_and(|u| matches!(u.scheme(), "http" | "https") && u.path().to_ascii_lowercase().ends_with(".m3u8"))
}

pub fn is_media_url(url: &str) -> bool {
    if is_hls_url(url) { return true; }
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    SITES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

pub fn scoped_cookies(url: &str, headers: &[(String, String)], cookies: &[Cookie]) -> Vec<Cookie> {
    if !cookies.is_empty() { return cookies.to_vec(); }
    let Some(host) = tauri::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)) else { return vec![]; };
    headers.iter().filter(|(key, _)| key.eq_ignore_ascii_case("cookie")).flat_map(|(_, value)| value.split(';')).filter_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        Some(Cookie { name: name.into(), value: value.into(), domain: host.clone(), path: "/".into(), secure: url.starts_with("https:"), host_only: true, expiration_date: None })
    }).collect()
}

fn cookie_matches(cookie: &Cookie, url: &tauri::Url) -> bool {
    let host = url.host_str().unwrap_or("");
    let domain = cookie.domain.trim_start_matches('.');
    let domain_match = host.eq_ignore_ascii_case(domain) || (!cookie.host_only && host.to_ascii_lowercase().ends_with(&format!(".{}", domain.to_ascii_lowercase())));
    let path = if cookie.path.is_empty() { "/" } else { &cookie.path };
    let path_match = url.path() == path || (url.path().starts_with(path) && (path.ends_with('/') || url.path().as_bytes().get(path.len()) == Some(&b'/')));
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    domain_match && path_match && (!cookie.secure || url.scheme() == "https") && cookie.expiration_date.is_none_or(|expiry| expiry > now)
}

pub fn request_headers(url: &tauri::Url, headers: &[(String, String)], cookies: &[Cookie]) -> swiftdl_engine::header::HeaderMap {
    let mut forwarded: Vec<_> = headers.iter().filter(|(key, _)| is_session_header(key)).cloned().collect();
    let cookie = cookies.iter().filter(|c| cookie_matches(c, url)).map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ");
    if !cookie.is_empty() { forwarded.push(("Cookie".into(), cookie)); }
    crate::manager::to_header_map(&forwarded)
}

struct Auth {
    jar: tempfile::NamedTempFile,
    headers: Vec<(String, String)>,
}

pub fn is_session_header(name: &str) -> bool {
    ["referer", "user-agent", "origin", "accept", "accept-language"].iter().any(|key| name.eq_ignore_ascii_case(key))
}

impl Auth {
    fn new(url: &str, headers: &[(String, String)], cookies: &[Cookie]) -> Result<Self> {
        let mut jar = tempfile::NamedTempFile::new()?;
        writeln!(jar, "# Netscape HTTP Cookie File")?;
        for cookie in scoped_cookies(url, headers, cookies) {
            if [&cookie.name, &cookie.value, &cookie.domain, &cookie.path].iter().any(|s| s.contains(['\t', '\r', '\n'])) { bail!("Invalid browser cookie data"); }
            if cookie.domain.is_empty() || cookie.name.is_empty() { continue; }
            let domain = if cookie.host_only { cookie.domain.trim_start_matches('.').to_string() } else { format!(".{}", cookie.domain.trim_start_matches('.')) };
            let expiry = cookie.expiration_date.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.0) as u64;
            writeln!(jar, "{}\t{}\t{}\t{}\t{}\t{}\t{}", domain, if cookie.host_only { "FALSE" } else { "TRUE" }, if cookie.path.is_empty() { "/" } else { &cookie.path }, if cookie.secure { "TRUE" } else { "FALSE" }, expiry, cookie.name, cookie.value)?;
        }
        jar.flush()?;
        Ok(Self { jar, headers: headers.to_vec() })
    }

    fn apply(&self, cmd: &mut Command) {
        cmd.arg("--cookies").arg(self.jar.path());
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("referer") { cmd.arg("--referer").arg(value); }
            if name.eq_ignore_ascii_case("user-agent") { cmd.arg("--user-agent").arg(value); }
            if ["origin", "accept", "accept-language"].iter().any(|key| name.eq_ignore_ascii_case(key)) {
                cmd.arg("--add-headers").arg(format!("{name}:{value}"));
            }
        }
    }
}

/// Dailymotion's configurable player id identifies the player, not a video.
/// yt-dlp needs the actual video id from the embed's query parameters.
fn download_url(url: &str) -> Result<String> {
    let url = url.trim();
    if let Ok(parsed) = tauri::Url::parse(url) {
        if parsed.host_str() == Some("geo.dailymotion.com") && parsed.path().starts_with("/player/") {
            let video = parsed.query_pairs().find(|(key, _)| key == "video").map(|(_, value)| value.into_owned());
            let Some(video) = video.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())) else {
                bail!("This Dailymotion player link has no valid video ID. Copy the original dailymotion.com/video/... link, or a player link containing ?video=..., and try again.");
            };
            return Ok(format!("https://www.dailymotion.com/video/{video}"));
        }
    }
    Ok(url.to_string())
}

/// Live status text for a media download ("Merging video and audio").
pub type Note = Arc<Mutex<Option<String>>>;

fn set_note(note: &Note, text: impl Into<String>) {
    *note.lock().unwrap() = Some(text.into());
}

// ---------- tools ----------

fn tools_dir() -> PathBuf {
    crate::browser::data_dir().join("tools")
}

fn ytdlp_path() -> PathBuf {
    tools_dir().join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" })
}

fn ytdlp_asset() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    }
}

const FFMPEG_EXE: &str = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };

/// Folder holding ffmpeg: our own copy (Windows), or the system's.
fn ffmpeg_dir() -> Option<PathBuf> {
    let own = tools_dir().join("ffmpeg");
    if own.join(FFMPEG_EXE).exists() {
        return Some(own);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from))
        .find(|d| d.join(FFMPEG_EXE).is_file())
}

static TOOLS: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Clone)]
struct Tools {
    ytdlp: PathBuf,
    ffmpeg: Option<PathBuf>,
}

/// Downloads yt-dlp (and ffmpeg on Windows) if they're missing.
async fn ensure_tools(client: &Client, note: &Note, need_ffmpeg: bool) -> Result<Tools> {
    let _guard = TOOLS.lock().await;
    std::fs::create_dir_all(tools_dir())?;
    let ytdlp = ytdlp_path();
    if !ytdlp.exists() {
        let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", ytdlp_asset());
        fetch(client, &url, &ytdlp, note, "Getting yt-dlp").await.context("couldn't download yt-dlp")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ytdlp, std::fs::Permissions::from_mode(0o755))?;
        }
        stamp_update();
    } else {
        update_in_background(&ytdlp);
    }
    if need_ffmpeg && ffmpeg_dir().is_none() && cfg!(windows) {
        let zip = tools_dir().join("ffmpeg.zip");
        let url = "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip";
        fetch(client, url, &zip, note, "Getting ffmpeg").await.context("couldn't download ffmpeg")?;
        set_note(note, "Unpacking ffmpeg");
        let unpacked = unpack_ffmpeg(&zip, &tools_dir().join("ffmpeg"));
        let _ = std::fs::remove_file(&zip);
        unpacked.context("couldn't unpack ffmpeg")?;
    }
    *note.lock().unwrap() = None;
    Ok(Tools { ytdlp, ffmpeg: ffmpeg_dir() })
}

/// Streams `url` to `dest` (via a temp file), showing progress in `note`.
async fn fetch(client: &Client, url: &str, dest: &Path, note: &Note, label: &str) -> Result<()> {
    set_note(note, label);
    let mut resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let tmp = dest.with_extension("download");
    let mut file = std::fs::File::create(&tmp)?;
    let mut got = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk)?;
        got += chunk.len() as u64;
        if total > 0 {
            set_note(note, format!("{label} · {}%", got * 100 / total));
        }
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Copies the programs and libraries from the archive's `bin` folder.
fn unpack_ffmpeg(zip: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.replace('\\', "/");
        let Some((dir, file)) = name.rsplit_once('/') else { continue };
        if !dir.ends_with("/bin") || file.is_empty() {
            continue;
        }
        let mut out = std::fs::File::create(dest.join(file))?;
        let mut buf = vec![];
        entry.read_to_end(&mut buf)?;
        out.write_all(&buf)?;
    }
    if !dest.join(FFMPEG_EXE).exists() {
        bail!("ffmpeg wasn't in the archive");
    }
    Ok(())
}

fn stamp_path() -> PathBuf {
    tools_dir().join("yt-dlp.checked")
}

fn stamp_update() {
    let _ = std::fs::write(stamp_path(), b"");
}

/// YouTube changes often and yt-dlp follows; check for a new yt-dlp once a day.
fn update_in_background(ytdlp: &Path) {
    let fresh = std::fs::metadata(stamp_path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(24 * 3600));
    if fresh {
        return;
    }
    stamp_update();
    let ytdlp = ytdlp.to_path_buf();
    std::thread::spawn(move || {
        let _ = command(&ytdlp).arg("-U").stdout(Stdio::null()).stderr(Stdio::null()).status();
    });
}

fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

// ---------- probing ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub label: String,
    /// yt-dlp format selector.
    pub format: String,
    pub ext: String,
    pub size: Option<u64>,
    pub audio_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: String,
    pub site: String,
    pub duration: Option<f64>,
    pub qualities: Vec<Quality>,
    pub stream_kind: Option<StreamKind>,
}

/// Asks yt-dlp what the page offers, as a short list of qualities.
pub async fn probe(client: &Client, url: &str, headers: Vec<(String, String)>, cookies: Vec<Cookie>, stream_kind: Option<StreamKind>) -> Result<MediaInfo> {
    probe_inner(client, url, headers, cookies, stream_kind, None).await
}

async fn probe_inner(client: &Client, url: &str, headers: Vec<(String, String)>, cookies: Vec<Cookie>, stream_kind: Option<StreamKind>, tools: Option<Tools>) -> Result<MediaInfo> {
    let url = download_url(url)?;
    let hls = stream_kind == Some(StreamKind::Hls) || is_hls_url(&url);
    let cookies = scoped_cookies(&url, &headers, &cookies);
    let playable = if hls {
        Some(crate::hls::vod_playlists(&url, &headers, &cookies).await?)
    } else { None };
    let auth = Auth::new(&url, &headers, &cookies)?;
    let note: Note = Arc::default();
    let tools = match tools { Some(tools) => tools, None => ensure_tools(client, &note, false).await? };
    let out = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = command(&tools.ytdlp);
        cmd.args(["--ignore-config", "-J", "--no-playlist", "--no-warnings"]);
        auth.apply(&mut cmd);
        cmd.arg(&url).output()
    })
    .await??;
    if !out.status.success() {
        bail!("{}", last_error(&String::from_utf8_lossy(&out.stderr)).unwrap_or_else(|| "yt-dlp couldn't read this page".into()));
    }
    let mut info: Value = serde_json::from_slice(&out.stdout).context("yt-dlp gave an unexpected answer")?;
    if let Some(playable) = playable {
        if let Some(formats) = info["formats"].as_array_mut() {
            formats.retain(|f| f["url"].as_str().is_some_and(|url| playable.contains(url)));
        }
    }
    if hls && (info["is_live"].as_bool() == Some(true) || matches!(info["live_status"].as_str(), Some("is_live" | "is_upcoming" | "post_live"))) {
        bail!("Live HLS recording is not supported yet. Choose a finished video (VOD).");
    }
    if info["has_drm"].as_bool() == Some(true) { bail!("DRM-protected media cannot be downloaded."); }
    let qualities = if hls { hls_qualities(&info)? } else { qualities(&info) };
    Ok(MediaInfo {
        title: info["title"].as_str().unwrap_or("video").to_string(),
        site: info["extractor_key"].as_str().unwrap_or("").to_string(),
        duration: info["duration"].as_f64(),
        qualities,
        stream_kind: hls.then_some(StreamKind::Hls),
    })
}

fn hls_qualities(info: &Value) -> Result<Vec<Quality>> {
    let formats = info["formats"].as_array().context("No downloadable HLS formats were found")?;
    let clear: Vec<_> = formats.iter().filter(|f| f["has_drm"].as_bool() != Some(true)
        && f["format_id"].as_str().is_some_and(|id| !id.is_empty())
        && !(f["vcodec"].as_str() == Some("none") && f["acodec"].as_str() == Some("none"))).collect();
    let audio = clear.iter().any(|f| f["vcodec"].as_str() == Some("none") && f["acodec"].as_str() != Some("none"));
    let mut result: Vec<_> = clear.iter().filter(|f| f["vcodec"].as_str() != Some("none")).map(|f| {
        let id = f["format_id"].as_str().unwrap();
        let format = if f["acodec"].as_str() == Some("none") && audio { format!("{id}+ba") } else { id.into() };
        let height = f["height"].as_u64();
        let bitrate = f["tbr"].as_f64();
        Quality { label: height.map(|h| format!("{h}p")).unwrap_or_else(|| bitrate.map(|b| format!("HLS · {b:.0} kbps")).unwrap_or_else(|| "HLS video".into())), format, ext: "mkv".into(), size: if f["acodec"].as_str() == Some("none") && audio { None } else { size_of(f) }, audio_only: false }
    }).collect();
    result.reverse(); // yt-dlp sorts formats from worst to best.
    for f in clear.iter().filter(|f| f["vcodec"].as_str() == Some("none")) {
        result.push(Quality { label: format!("Audio only · {}", f["language"].as_str().unwrap_or("default")), format: f["format_id"].as_str().unwrap().into(), ext: "mkv".into(), size: size_of(f), audio_only: true });
    }
    if result.is_empty() { bail!("No non-DRM HLS formats were found."); }
    Ok(result)
}

fn size_of(f: &Value) -> Option<u64> {
    f["filesize"].as_u64().or_else(|| f["filesize_approx"].as_u64())
}

/// Site extractors can omit codecs and dimensions even for playable video.
/// Keep their exact format IDs so selecting a quality cannot silently pick
/// another rendition. Only an explicit `none` means a stream is absent.
fn site_qualities(info: &Value) -> Vec<Quality> {
    let Some(formats) = info["formats"].as_array() else { return vec![]; };
    let clear: Vec<_> = formats.iter().filter(|f| {
        f["has_drm"].as_bool() != Some(true)
            && f["format_id"].as_str().is_some_and(|id| !id.is_empty())
            && !(f["vcodec"].as_str() == Some("none") && f["acodec"].as_str() == Some("none"))
    }).collect();
    let audio = clear.iter().any(|f| f["vcodec"].as_str() == Some("none"));
    let mut list = Vec::new();
    // yt-dlp sorts formats from worst to best. Put audio-only choices last.
    for audio_only in [false, true] {
        for f in clear.iter().rev().filter(|f| (f["vcodec"].as_str() == Some("none")) == audio_only) {
            let id = f["format_id"].as_str().unwrap();
            let merge = !audio_only && f["acodec"].as_str() == Some("none") && audio;
            let description = if audio_only {
                "Audio only".to_string()
            } else if let Some(height) = f["height"].as_u64().filter(|h| *h > 0) {
                let fps = f["fps"].as_f64().unwrap_or(0.0);
                if fps > 30.5 { format!("{height}p{}", fps.round()) } else { format!("{height}p") }
            } else {
                f["format_note"].as_str().filter(|s| !s.is_empty()).unwrap_or("Video").to_string()
            };
            list.push(Quality {
                label: format!("{description} · {id}"),
                format: if merge { format!("{id}+ba") } else { id.into() },
                ext: if merge { "mp4" } else { f["ext"].as_str().unwrap_or("mp4") }.into(),
                size: if merge { None } else { size_of(f) },
                audio_only,
            });
        }
    }
    list
}

/// YouTube groups codec variants by height; other sites expose every format.
fn qualities(info: &Value) -> Vec<Quality> {
    if !info["extractor_key"].as_str().is_some_and(|key| key.eq_ignore_ascii_case("youtube")) {
        let list = site_qualities(info);
        if !list.is_empty() { return list; }
    }
    let formats = info["formats"].as_array().cloned().unwrap_or_default();
    let has = |f: &Value, key: &str| f[key].as_str().is_some_and(|c| c != "none");
    let videos: Vec<&Value> = formats.iter().filter(|f| has(f, "vcodec") && f["height"].as_u64().unwrap_or(0) > 0).collect();
    let audios: Vec<&Value> = formats.iter().filter(|f| has(f, "acodec") && !has(f, "vcodec")).collect();
    let tbr = |f: &&Value| f["tbr"].as_f64().unwrap_or(0.0);
    let best_audio = audios.iter().copied().max_by(|a, b| tbr(a).total_cmp(&tbr(b)));
    let audio_size = best_audio.and_then(size_of);

    let mut heights: Vec<u64> = videos.iter().filter_map(|f| f["height"].as_u64()).collect();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();
    let mut list: Vec<Quality> = heights
        .into_iter()
        .map(|h| {
            let at: Vec<&&Value> = videos.iter().filter(|f| f["height"].as_u64() == Some(h)).collect();
            let best = at.iter().copied().max_by(|a, b| tbr(a).total_cmp(&tbr(b))).copied();
            let fps = at.iter().filter_map(|f| f["fps"].as_f64()).fold(0.0, f64::max);
            let progressive = best.is_some_and(|f| has(f, "acodec"));
            let size = best.and_then(size_of).map(|v| if progressive { v } else { v + audio_size.unwrap_or(0) });
            Quality {
                label: if fps > 30.5 { format!("{h}p{}", fps.round()) } else { format!("{h}p") },
                // H.264 + AAC first: it plays everywhere. AV1/VP9 only
                // where YouTube has nothing else (1440p and 4K).
                format: format!(
                    "bv*[height={h}][vcodec^=avc1]+ba[ext=m4a]/bv*[height={h}]+ba[ext=m4a]/bv*[height={h}]+ba/b[height={h}]/bv*[height<={h}]+ba/b"
                ),
                ext: "mp4".into(),
                size,
                audio_only: false,
            }
        })
        .collect();
    if let Some(a) = best_audio {
        let m4a = audios.iter().any(|f| f["ext"].as_str() == Some("m4a"));
        list.push(Quality {
            label: "Audio only".into(),
            format: "ba[ext=m4a]/ba".into(),
            ext: if m4a { "m4a".into() } else { a["ext"].as_str().unwrap_or("m4a").into() },
            size: if m4a { audios.iter().filter(|f| f["ext"].as_str() == Some("m4a")).filter_map(|f| size_of(f)).max() } else { size_of(a) },
            audio_only: true,
        });
    }
    if list.is_empty() {
        list.push(Quality { label: "Best quality".into(), format: "b".into(), ext: info["ext"].as_str().unwrap_or("mp4").into(), size: None, audio_only: false });
    }
    list
}

fn last_error(stderr: &str) -> Option<String> {
    stderr.lines().rev().find_map(|l| l.strip_prefix("ERROR: ")).map(|e| {
        e.split_whitespace().map(|word| if word.contains("https://") || word.contains("http://") { "[URL redacted]" } else { word }).collect::<Vec<_>>().join(" ")
    })
}

// ---------- downloading ----------

#[derive(Clone)]
pub struct Job {
    pub url: String,
    pub format: String,
    pub dest: PathBuf,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<Cookie>,
    pub stream_kind: Option<StreamKind>,
}

/// Runs yt-dlp for one download, reporting into `progress` and `note`.
/// Cancelling stops yt-dlp; its partial files stay, so running the same job
/// again continues where it left off.
pub async fn run(client: &Client, job: Job, progress: Arc<Progress>, note: Note, cancel: CancellationToken) -> Result<Outcome> {
    run_inner(client, job, progress, note, cancel, None).await
}

async fn run_inner(client: &Client, job: Job, progress: Arc<Progress>, note: Note, cancel: CancellationToken, tools: Option<Tools>) -> Result<Outcome> {
    // yt-dlp resumes interrupted downloads from its .part/.ytdl files. The
    // engine's default is false until an HTTP range probe says otherwise.
    progress.resumable.store(true, Ordering::Relaxed);
    let url = download_url(&job.url)?;
    let hls = job.stream_kind == Some(StreamKind::Hls) || is_hls_url(&url);
    let cookies = scoped_cookies(&url, &job.headers, &job.cookies);
    let merges = job.format.contains('+');
    let tools = match tools { Some(tools) => tools, None => tokio::select! {
        biased;
        _ = cancel.cancelled() => return Ok(Outcome::Paused),
        tools = ensure_tools(client, &note, merges || hls) => tools?,
    }};
    if (merges || hls) && tools.ffmpeg.is_none() {
        bail!("HLS downloads and joining video/audio need ffmpeg. Install it (brew install ffmpeg, or sudo apt install ffmpeg) and try again.");
    }
    if hls {
        let info = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(Outcome::Paused),
            result = probe_inner(client, &url, job.headers.clone(), cookies.clone(), job.stream_kind, Some(tools.clone())) => result?,
        };
        if !info.qualities.iter().any(|q| q.format == job.format) {
            bail!("The selected HLS rendition is no longer a finished, non-DRM video. Refresh the page and choose an available quality.");
        }
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Paused);
    }
    if let Some(dir) = job.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let working_dest = if hls {
        let dir = hls_work_dir(&job.dest);
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
                }
                std::fs::write(dir.join(".owner"), b"SwiftDL HLS fragments v1")?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && owned_hls_dir(&dir) => {}
            Err(e) => return Err(e).context("could not prepare the HLS fragment directory"),
        }
        dir.join(job.dest.file_name().context("HLS destination has no filename")?)
    } else { job.dest.clone() };
    set_note(&note, "Getting video details");

    let mut cmd = command(&tools.ytdlp);
    // The standalone yt-dlp binary launches a Python child, and may launch
    // ffmpeg too. Keep them in one group so pause stops all pipe writers.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.args(["--ignore-config", "-f", &job.format, "--no-playlist", "--newline", "--no-colors", "--no-mtime", "--continue", "--abort-on-unavailable-fragments", "-N", "4"])
        .args(["--progress-template", "download:SDL|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s"]);
    if hls {
        cmd.args(["--merge-output-format", "mkv", "--remux-video", "mkv", "--hls-prefer-native"]);
    } else if merges {
        cmd.args(["--merge-output-format", "mp4"]);
    }
    if let Some(dir) = &tools.ffmpeg {
        cmd.arg("--ffmpeg-location").arg(dir);
    }
    let auth = Auth::new(&url, &job.headers, &cookies)?;
    auth.apply(&mut cmd);
    // Let yt-dlp use the source extension for intermediate files, then remux
    // to .mkv. A literal .mkv template can otherwise produce .mkv.mkv output.
    let output = if hls {
        format!("{}.%(ext)s", working_dest.with_extension("").to_string_lossy().replace('%', "%%"))
    } else { job.dest.to_string_lossy().replace('%', "%%") };
    cmd.arg("-o").arg(output).arg(&url);
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("couldn't start yt-dlp")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let errors = Arc::new(Mutex::new(String::new()));
    let error_reader = {
        let errors = errors.clone();
        std::thread::spawn(move || {
            let text: String = BufReader::new(stderr).lines().map_while(Result::ok).collect::<Vec<_>>().join("\n");
            *errors.lock().unwrap() = text;
        })
    };
    let reader = {
        let (progress, note) = (progress.clone(), note.clone());
        std::thread::spawn(move || read_progress(stdout, &progress, &note))
    };

    let child = Arc::new(Mutex::new(child));
    let waiter = {
        let (child, cancel) = (child.clone(), cancel.clone());
        tauri::async_runtime::spawn_blocking(move || wait(&child, &cancel))
    };
    let exit = waiter.await??;
    let _ = reader.join();
    let _ = error_reader.join();
    progress.connections.store(0, Ordering::Relaxed);
    *note.lock().unwrap() = None;

    match exit {
        None => Ok(Outcome::Paused),
        Some(true) if working_dest.exists() => {
            if hls {
                std::fs::rename(&working_dest, &job.dest).context("could not move the completed HLS file to its destination")?;
                cleanup_hls_dir(&job.dest);
            }
            let size = std::fs::metadata(&job.dest).map(|m| m.len()).unwrap_or(0);
            progress.total.store(size, Ordering::Relaxed);
            progress.downloaded.store(size, Ordering::Relaxed);
            Ok(Outcome::Completed)
        }
        Some(true) => Err(anyhow!("yt-dlp finished but the file wasn't created")),
        Some(false) => {
            let stderr = errors.lock().unwrap().clone();
            Err(anyhow!(last_error(&stderr).unwrap_or_else(|| "yt-dlp stopped with an error".into())))
        }
    }
}

/// Waits for yt-dlp. `None` when cancelled, else whether it succeeded.
fn wait(child: &Mutex<Child>, cancel: &CancellationToken) -> Result<Option<bool>> {
    loop {
        if cancel.is_cancelled() {
            let mut c = child.lock().unwrap();
            #[cfg(unix)]
            {
                // SAFETY: yt-dlp was started in its own process group, whose
                // id is the child's pid. A negative pid targets that group.
                unsafe { libc::kill(-(c.id() as i32), libc::SIGKILL); }
            }
            #[cfg(windows)]
            {
                let _ = command(Path::new("taskkill"))
                    .args(["/PID", &c.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null()).stderr(Stdio::null()).status();
            }
            let _ = c.kill();
            let _ = c.wait();
            return Ok(None);
        }
        if let Some(status) = child.lock().unwrap().try_wait()? {
            return Ok(Some(status.success()));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Turns yt-dlp's output into progress. A video download is two files
/// (video, then audio), each reported from zero, so finished files are
/// added to a running base.
fn read_progress(stdout: impl std::io::Read, progress: &Progress, note: &Note) {
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0).map(|n| n as u64);
    let (mut base, mut cur) = (0u64, 0u64);
    let hint = progress.total.load(Ordering::Relaxed);
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("SDL|") {
            let f: Vec<&str> = rest.split('|').collect();
            if f.len() < 4 {
                continue;
            }
            let done = num(f[1]).unwrap_or(0);
            let total = num(f[2]).or_else(|| num(f[3])).unwrap_or(0);
            let cur_total;
            if f[0] == "finished" {
                base += total.max(done).max(cur);
                (cur, cur_total) = (0, 0);
            } else {
                (cur, cur_total) = (done, total);
                *note.lock().unwrap() = None;
                progress.connections.store(1, Ordering::Relaxed);
            }
            progress.downloaded.store(base + cur, Ordering::Relaxed);
            progress.total.store(hint.max(base + cur_total), Ordering::Relaxed);
        } else if line.starts_with("[Merger]") || line.starts_with("[VideoRemuxer]") {
            set_note(note, "Merging video and audio");
            progress.connections.store(0, Ordering::Relaxed);
        } else if line.starts_with("[Fixup") || line.starts_with("[ExtractAudio]") {
            set_note(note, "Finishing");
        }
    }
}

/// Removes yt-dlp's partial files for a download that was cancelled.
pub fn cleanup(dest: &Path) {
    cleanup_hls_dir(dest);
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name().map(|n| n.to_string_lossy().into_owned())) else { return };
    let stem = dest.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let partial = n.ends_with(".part") || n.ends_with(".ytdl") || n.contains(".part-Frag");
        if partial && (n.starts_with(&name) || n.starts_with(&format!("{stem}.f"))) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn hls_work_dir(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".swiftdl-hls");
    PathBuf::from(name)
}

fn owned_hls_dir(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_dir())
        && std::fs::read(dir.join(".owner")).is_ok_and(|v| v == b"SwiftDL HLS fragments v1")
}

fn cleanup_hls_dir(dest: &Path) {
    let dir = hls_work_dir(dest);
    if owned_hls_dir(&dir) { let _ = std::fs::remove_dir_all(dir); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_session_headers_are_used_without_impersonation() {
        let headers = vec![
            ("Referer".into(), "https://player.example.test/".into()),
            ("User-Agent".into(), "captured-browser-agent".into()),
            ("Origin".into(), "https://player.example.test".into()),
            ("Accept".into(), "*/*".into()),
            ("Accept-Language".into(), "en-US,en;q=0.9".into()),
        ];
        let auth = Auth::new("https://cdn.example.test/video.m3u8", &headers, &[]).unwrap();
        let mut cmd = Command::new("yt-dlp");
        auth.apply(&mut cmd);
        let args: Vec<_> = cmd.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert!(!args.iter().any(|s| s.contains("impersonate")));
        assert!(args.iter().any(|s| s == "captured-browser-agent"));
        assert!(args.iter().any(|s| s == "Origin:https://player.example.test"));
        assert!(args.iter().any(|s| s == "Accept:*/*"));
        assert!(args.iter().any(|s| s == "Accept-Language:en-US,en;q=0.9"));
        let forwarded = request_headers(&tauri::Url::parse("https://cdn.example.test/video.m3u8").unwrap(), &headers, &[]);
        assert_eq!(forwarded["user-agent"], "captured-browser-agent");
        assert_eq!(forwarded["accept-language"], "en-US,en;q=0.9");
    }

    #[test]
    fn cookie_scope_and_cookie_jar_are_preserved() {
        let cookie = Cookie { name: "session".into(), value: "secret".into(), domain: ".example.test".into(), path: "/media".into(), secure: true, host_only: false, expiration_date: None };
        assert!(cookie_matches(&cookie, &tauri::Url::parse("https://cdn.example.test/media/segment.ts").unwrap()));
        for url in ["http://cdn.example.test/media/segment.ts", "https://other.test/media/segment.ts", "https://cdn.example.test/mediabad/segment.ts"] {
            assert!(!cookie_matches(&cookie, &tauri::Url::parse(url).unwrap()), "{url}");
        }
        let mut host_only = cookie.clone();
        host_only.host_only = true;
        assert!(!cookie_matches(&host_only, &tauri::Url::parse("https://cdn.example.test/media/a.ts").unwrap()));
        let headers = vec![("Referer".into(), "https://example.test/watch".into()), ("User-Agent".into(), "fixture".into()), ("Cookie".into(), "must-not-forward=secret".into())];
        let auth = Auth::new("https://example.test/media/list.m3u8", &headers, &[cookie]).unwrap();
        let jar = std::fs::read_to_string(auth.jar.path()).unwrap();
        assert!(jar.contains(".example.test\tTRUE\t/media\tTRUE\t0\tsession\tsecret"));
        let mut cmd = Command::new("yt-dlp");
        auth.apply(&mut cmd);
        let args = cmd.get_args().map(|v| v.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(!args.iter().any(|a| a.contains("must-not-forward") || a.contains("Cookie:")));
        assert!(args.iter().any(|a| a == "--referer"));
        assert!(args.iter().any(|a| a == "--user-agent"));
    }

    #[test]
    fn hls_detection_error_redaction_and_owned_cleanup() {
        assert!(is_media_url("https://unknown.example/manifest.M3U8?sig=secret"));
        assert!(!is_hls_url("https://unknown.example/video?next=a.m3u8"));
        assert!(!last_error("ERROR: Unsupported URL: https://cdn.test/a?sig=secret").unwrap().contains("secret"));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("video.mkv");
        let work = hls_work_dir(&dest);
        std::fs::create_dir(&work).unwrap();
        std::fs::write(work.join("unrelated"), b"keep").unwrap();
        cleanup(&dest);
        assert!(work.exists(), "do not delete a directory not owned by SwiftDL");
        std::fs::write(work.join(".owner"), b"SwiftDL HLS fragments v1").unwrap();
        cleanup(&dest);
        assert!(!work.exists());
    }

    #[test]
    fn hls_qualities_include_muxed_video_separate_audio_and_unknown_height() {
        let info = serde_json::json!({"formats": [
            {"format_id": "audio", "vcodec": "none", "acodec": "aac", "language": "en"},
            {"format_id": "low", "vcodec": "avc1", "acodec": "aac", "height": 360},
            {"format_id": "high", "vcodec": "avc1", "acodec": "none", "height": 720},
            {"format_id": "unknown", "tbr": 3000},
            {"format_id": "storyboard", "vcodec": "none", "acodec": "none"},
            {"format_id": "drm", "has_drm": true, "height": 1080}
        ]});
        let choices = hls_qualities(&info).unwrap();
        assert!(choices.iter().any(|q| q.format == "high+ba"));
        assert!(choices.iter().any(|q| q.format == "low"));
        assert!(choices.iter().any(|q| q.format == "unknown"));
        assert!(choices.iter().any(|q| q.format == "audio" && q.audio_only));
        assert!(!choices.iter().any(|q| q.format.contains("drm")));
        assert!(!choices.iter().any(|q| q.format == "storyboard"));
        assert!(choices.iter().all(|q| q.ext == "mkv"));
    }

    /// Local-only fixture server, generated by tools/test-hls.py. Does not use
    /// the app's data directory, download tools, or contact a public website.
    #[tokio::test]
    #[ignore = "Run with python3 tools/test-hls.py (needs yt-dlp and ffmpeg)"]
    async fn local_hls_vod_download_and_resume() {
        let base = std::env::var("SWIFTDL_HLS_FIXTURE").expect("use tools/test-hls.py");
        let client = swiftdl_engine::build_client().unwrap();
        let headers = vec![
            ("Referer".into(), "https://fixture.test/watch".into()),
            ("Origin".into(), "https://fixture.test".into()),
            ("User-Agent".into(), "SwiftDL-HLS-test".into()),
            ("Accept".into(), "*/*".into()),
            ("Accept-Language".into(), "en-US,en;q=0.9".into()),
        ];
        let cookies = vec![Cookie { name: "session".into(), value: "fixture-only".into(), domain: "127.0.0.1".into(), path: "/".into(), host_only: true, secure: false, expiration_date: None }];
        let tools = || Some(Tools { ytdlp: "yt-dlp".into(), ffmpeg: ffmpeg_dir() });
        let dir = tempfile::tempdir().unwrap();
        let mut sniff_headers = headers.clone();
        sniff_headers.push(("Cookie".into(), "session=fixture-only".into()));
        assert!(crate::hls::detect(&format!("{base}/detect?sig=exact%2Fvalue&rn=keep"), &sniff_headers).await.unwrap());
        for variant in ["ts", "fmp4", "aes", "separate", "mixed"] {
            let url = format!("{base}/{variant}/master.m3u8?sig=exact%2Fvalue&rn=keep");
            let info = probe_inner(&client, &url, headers.clone(), cookies.clone(), Some(StreamKind::Hls), tools()).await.unwrap();
            if variant == "mixed" {
                assert_eq!(info.qualities.len(), 1, "only the finished, clear rendition should be offered");
                assert!(info.qualities[0].label.starts_with("90p"));
            }
            let quality = info.qualities.iter().find(|q| !q.audio_only).unwrap();
            let dest = dir.path().join(format!("{variant} 100%.mkv"));
            let job = Job { url, format: quality.format.clone(), dest: dest.clone(), headers: headers.clone(), cookies: cookies.clone(), stream_kind: Some(StreamKind::Hls) };
            if variant == "mixed" {
                let mut unavailable = job.clone();
                unavailable.format = "unavailable-rendition".into();
                let error = run_inner(&client, unavailable, Arc::default(), Note::default(), CancellationToken::new(), tools()).await.unwrap_err().to_string();
                assert!(error.contains("selected HLS rendition"));
            }
            if variant == "ts" {
                let progress = Arc::new(Progress::default());
                let cancel = CancellationToken::new();
                let task = {
                    let (client, job, progress, cancel) = (client.clone(), job.clone(), progress.clone(), cancel.clone());
                    tokio::spawn(async move { run_inner(&client, job, progress, Note::default(), cancel, tools()).await })
                };
                // Startup now includes a fresh rendition probe as well as the
                // downloader's extraction; standalone Python startup can be slow.
                let started = tokio::time::timeout(Duration::from_secs(60), async {
                    while progress.downloaded.load(Ordering::Relaxed) == 0 && !task.is_finished() {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }).await;
                cancel.cancel();
                let outcome = task.await.unwrap();
                assert!(started.is_ok(), "no fragment progress: {outcome:?}");
                assert!(progress.downloaded.load(Ordering::Relaxed) > 0, "download ended before pause could be tested: {outcome:?}");
                assert!(matches!(outcome.unwrap(), Outcome::Paused));
                assert!(std::fs::read_dir(hls_work_dir(&dest)).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(".part")));
            }
            assert!(matches!(run_inner(&client, job, Arc::default(), Note::default(), CancellationToken::new(), tools()).await.unwrap(), Outcome::Completed));
            assert!(dest.exists(), "final output filename must match the stored download path");
            let output = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0"]).arg(&dest).output().unwrap();
            assert!(output.status.success());
            let streams = String::from_utf8_lossy(&output.stdout);
            assert!(streams.contains("video") && streams.contains("audio"), "{variant}: {streams}");
            if variant == "separate" {
                let audio = info.qualities.iter().find(|q| q.audio_only).unwrap();
                let dest = dir.path().join("audio only.mkv");
                let job = Job { url: format!("{base}/{variant}/master.m3u8?sig=exact%2Fvalue&rn=keep"), format: audio.format.clone(), dest: dest.clone(), headers: headers.clone(), cookies: cookies.clone(), stream_kind: Some(StreamKind::Hls) };
                assert!(matches!(run_inner(&client, job, Arc::default(), Note::default(), CancellationToken::new(), tools()).await.unwrap(), Outcome::Completed));
                let output = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0"]).arg(dest).output().unwrap();
                let streams = String::from_utf8_lossy(&output.stdout);
                assert!(output.status.success() && streams.contains("audio") && !streams.contains("video"));
            }
        }
        crate::hls::validate_vod(&format!("{base}/redirect.m3u8"), &headers, &cookies).await.unwrap();
        for (route, message) in [("live.m3u8", "Live HLS"), ("drm.m3u8", "DRM"), ("expired.m3u8", "403"), ("cycle.m3u8", "no finished")] {
            let error = crate::hls::validate_vod(&format!("{base}/{route}"), &headers, &cookies).await.unwrap_err().to_string();
            assert!(error.contains(message), "{route}: {error}");
        }
    }

    #[test]
    fn normalises_dailymotion_player_links_to_video_links() {
        assert_eq!(download_url("https://geo.dailymotion.com/player/xtv3w.html?video=x123abc&autoplay=1").unwrap(), "https://www.dailymotion.com/video/x123abc");
        assert_eq!(download_url("https://geo.dailymotion.com/player/xtv3w.html?autoplay=1&video=%78123abc").unwrap(), "https://www.dailymotion.com/video/x123abc");
        let regular = "https://www.dailymotion.com/video/x123abc";
        assert_eq!(download_url(regular).unwrap(), regular);
        let youtube = "https://www.youtube.com/watch?v=abc123";
        assert_eq!(download_url(youtube).unwrap(), youtube);
    }

    #[test]
    fn player_id_is_not_mistaken_for_a_video_id() {
        for url in [
            "https://geo.dailymotion.com/player/xtv3w.html?",
            "https://geo.dailymotion.com/player/xtv3w.html?video=",
            "https://geo.dailymotion.com/player/xtv3w.html?video=bad%2Fid",
        ] {
            assert!(download_url(url).unwrap_err().to_string().contains("original dailymotion.com/video/"));
        }
    }

    #[tokio::test]
    async fn pause_before_tool_setup_does_not_download_tools() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let job = Job { url: "https://example.invalid/video".into(), format: "b".into(), dest: "unused.mp4".into(), headers: vec![], cookies: vec![], stream_kind: None };
        let result = run(&swiftdl_engine::build_client().unwrap(), job, Arc::default(), Note::default(), cancel).await.unwrap();
        assert!(matches!(result, Outcome::Paused));
    }

    #[cfg(unix)]
    #[test]
    fn pause_stops_descendants_and_closes_progress_pipe() {
        use std::os::unix::process::CommandExt;
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & echo ready; wait"])
            .process_group(0).stdout(Stdio::piped()).spawn().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut ready = String::new();
        stdout.read_line(&mut ready).unwrap();
        assert_eq!(ready.trim(), "ready");
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(wait(&Mutex::new(child), &cancel).unwrap(), None);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut rest = String::new();
            tx.send(stdout.read_to_string(&mut rest)).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(2)).expect("a descendant kept stdout open after pause").unwrap();
    }

    #[test]
    fn recognises_video_sites() {
        assert!(is_media_url("https://www.youtube.com/watch?v=aqz-KE-bpKQ"));
        assert!(is_media_url("https://youtu.be/aqz-KE-bpKQ"));
        assert!(is_media_url("https://m.youtube.com/shorts/x"));
        assert!(!is_media_url("https://drive.usercontent.google.com/download?id=1"));
        assert!(!is_media_url("https://notyoutube.com/watch"));
    }

    #[test]
    fn recognises_additional_ytdlp_sites_without_matching_lookalikes() {
        assert!(!is_media_url("https://hqporner.com/hdporn/128499-video.html"));
        assert!(!is_media_url("https://www.hqporner.com/hdporn/128499-video.html"));
        for site in ["xvideos.com", "xnxx.com"] {
            for host in [site.to_string(), format!("www.{site}"), format!("m.{site}"), site.to_uppercase()] {
                assert!(is_media_url(&format!("https://{host}/video/123")), "{host}");
            }
            assert!(!is_media_url(&format!("https://not{site}/video/123")));
            assert!(!is_media_url(&format!("https://{site}.example.org/video/123")));
            assert!(!is_media_url(&format!("https://example.org/?url=https://{site}/video/123")));
        }
    }

    #[test]
    fn site_qualities_include_unknown_codecs_and_unknown_resolutions() {
        let mut formats = vec![
            serde_json::json!({"format_id": "mp4-low", "ext": "mp4"}),
            serde_json::json!({"format_id": "mp4-high", "ext": "mp4", "vcodec": null, "acodec": null}),
        ];
        for height in [250, 360, 480, 720, 1080] {
            formats.push(serde_json::json!({"format_id": format!("hls-{height}p"), "height": height, "ext": "mp4", "filesize_approx": height * 1000}));
        }
        let choices = qualities(&serde_json::json!({"extractor_key": "XVideos", "formats": formats}));
        let ids: Vec<_> = choices.iter().map(|q| q.format.as_str()).collect();
        assert_eq!(ids, ["hls-1080p", "hls-720p", "hls-480p", "hls-360p", "hls-250p", "mp4-high", "mp4-low"]);
        assert_eq!(choices[0].label, "1080p · hls-1080p");
        assert_eq!(choices[0].size, Some(1_080_000));
        assert!(choices.iter().all(|q| q.ext == "mp4" && !q.audio_only));
    }

    #[test]
    fn site_qualities_preserve_variants_and_skip_drm_and_storyboards() {
        let choices = qualities(&serde_json::json!({"extractor_key": "Generic", "formats": [
            {"format_id": "audio", "vcodec": "none", "acodec": "aac", "ext": "m4a"},
            {"format_id": "https-720", "height": 720, "ext": "mp4", "vcodec": "avc1", "acodec": "aac", "filesize": 100},
            {"format_id": "hls-720", "height": 720, "ext": "mp4"},
            {"format_id": "video-1080", "height": 1080, "vcodec": "avc1", "acodec": "none"},
            {"format_id": "drm", "height": 2160, "has_drm": true},
            {"format_id": "storyboard", "vcodec": "none", "acodec": "none"}
        ]}));
        let ids: Vec<_> = choices.iter().map(|q| q.format.as_str()).collect();
        assert_eq!(ids, ["video-1080+ba", "hls-720", "https-720", "audio"]);
        assert_eq!(choices[0].size, None);
        assert_eq!(choices[2].size, Some(100));
        assert!(choices[3].audio_only);
        assert_eq!(choices[3].ext, "m4a");
    }

    #[test]
    fn lists_one_quality_per_height_then_audio() {
        let info = serde_json::json!({ "extractor_key": "Youtube", "formats": [
            { "format_id": "137", "vcodec": "avc1", "acodec": "none", "height": 1080, "fps": 60, "tbr": 4000, "filesize": 400 },
            { "format_id": "248", "vcodec": "vp9", "acodec": "none", "height": 1080, "fps": 60, "tbr": 3000, "filesize": 300 },
            { "format_id": "136", "vcodec": "avc1", "acodec": "none", "height": 720, "fps": 30, "tbr": 2000, "filesize_approx": 200 },
            { "format_id": "140", "vcodec": "none", "acodec": "mp4a", "ext": "m4a", "tbr": 128, "filesize": 10 },
            { "format_id": "251", "vcodec": "none", "acodec": "opus", "ext": "webm", "tbr": 160, "filesize": 12 },
        ]});
        let q = qualities(&info);
        let labels: Vec<&str> = q.iter().map(|q| q.label.as_str()).collect();
        assert_eq!(labels, ["1080p60", "720p", "Audio only"]);
        assert_eq!(q[0].size, Some(400 + 12), "best video plus best audio");
        assert_eq!(q[2].ext, "m4a");
        assert_eq!(q[2].size, Some(10));
    }

    #[test]
    fn adds_up_progress_across_video_and_audio() {
        let out = "[download] Destination: a.f396.mp4\nSDL|downloading|1024|15298808|NA\nSDL|finished|15298808|15298808|NA\n\
                   [download] Destination: a.f251.webm\nSDL|downloading|5000000|10202210|NA\nSDL|finished|10202210|10202210|NA\n\
                   [Merger] Merging formats into \"a.mp4\"\n";
        let progress = Progress::default();
        progress.total.store(25_000_000, Ordering::Relaxed);
        let note = Note::default();
        read_progress(out.as_bytes(), &progress, &note);
        assert_eq!(progress.downloaded.load(Ordering::Relaxed), 15298808 + 10202210);
        assert_eq!(progress.total.load(Ordering::Relaxed), 15298808 + 10202210);
        assert_eq!(note.lock().unwrap().as_deref(), Some("Merging video and audio"));
    }
}
