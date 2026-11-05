//! Video sites (YouTube and others), through yt-dlp.
//!
//! These sites don't serve a plain file. YouTube's player streams through a
//! private protocol with no download address, so SwiftDL hands these pages
//! to yt-dlp, which keeps up with such changes, and to ffmpeg, which joins
//! the separate video and audio streams. Both are fetched into the app's
//! data folder the first time they're needed, and yt-dlp is kept current.

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
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
];

pub fn is_media_url(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    SITES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: String,
    pub site: String,
    pub duration: Option<f64>,
    pub qualities: Vec<Quality>,
}

/// Asks yt-dlp what the page offers, as a short list of qualities.
pub async fn probe(client: &Client, url: &str, cookies: Option<String>) -> Result<MediaInfo> {
    let note: Note = Arc::default();
    let tools = ensure_tools(client, &note, false).await?;
    let url = url.to_string();
    let out = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = command(&tools.ytdlp);
        cmd.args(["-J", "--no-playlist", "--no-warnings"]);
        if let Some(c) = cookies.filter(|c| !c.is_empty()) {
            cmd.arg("--add-headers").arg(format!("Cookie:{c}"));
        }
        cmd.arg(&url).output()
    })
    .await??;
    if !out.status.success() {
        bail!("{}", last_error(&String::from_utf8_lossy(&out.stderr)).unwrap_or_else(|| "yt-dlp couldn't read this page".into()));
    }
    let info: Value = serde_json::from_slice(&out.stdout).context("yt-dlp gave an unexpected answer")?;
    Ok(MediaInfo {
        title: info["title"].as_str().unwrap_or("video").to_string(),
        site: info["extractor_key"].as_str().unwrap_or("").to_string(),
        duration: info["duration"].as_f64(),
        qualities: qualities(&info),
    })
}

fn size_of(f: &Value) -> Option<u64> {
    f["filesize"].as_u64().or_else(|| f["filesize_approx"].as_u64())
}

/// One entry per video height, best first, then audio only.
fn qualities(info: &Value) -> Vec<Quality> {
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
        });
    }
    if list.is_empty() {
        list.push(Quality { label: "Best quality".into(), format: "b".into(), ext: info["ext"].as_str().unwrap_or("mp4").into(), size: None });
    }
    list
}

fn last_error(stderr: &str) -> Option<String> {
    stderr.lines().rev().find_map(|l| l.strip_prefix("ERROR: ")).map(|e| e.trim().to_string())
}

// ---------- downloading ----------

pub struct Job {
    pub url: String,
    pub format: String,
    pub dest: PathBuf,
    pub cookies: Option<String>,
}

/// Runs yt-dlp for one download, reporting into `progress` and `note`.
/// Cancelling stops yt-dlp; its partial files stay, so running the same job
/// again continues where it left off.
pub async fn run(client: &Client, job: Job, progress: Arc<Progress>, note: Note, cancel: CancellationToken) -> Result<Outcome> {
    // yt-dlp resumes interrupted downloads from its .part/.ytdl files. The
    // engine's default is false until an HTTP range probe says otherwise.
    progress.resumable.store(true, Ordering::Relaxed);
    let merges = job.format.contains('+');
    let tools = ensure_tools(client, &note, merges).await?;
    if merges && tools.ffmpeg.is_none() {
        bail!("Joining video and audio needs ffmpeg. Install it (brew install ffmpeg, or sudo apt install ffmpeg) and try again.");
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Paused);
    }
    if let Some(dir) = job.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    set_note(&note, "Getting video details");

    let mut cmd = command(&tools.ytdlp);
    cmd.args(["-f", &job.format, "--no-playlist", "--newline", "--no-colors", "--no-mtime", "-N", "4"])
        .args(["--progress-template", "download:SDL|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s"]);
    if merges {
        cmd.args(["--merge-output-format", "mp4"]);
    }
    if let Some(dir) = &tools.ffmpeg {
        cmd.arg("--ffmpeg-location").arg(dir);
    }
    if let Some(c) = job.cookies.as_ref().filter(|c| !c.is_empty()) {
        cmd.arg("--add-headers").arg(format!("Cookie:{c}"));
    }
    // yt-dlp treats % in the output name as a template field.
    cmd.arg("-o").arg(job.dest.to_string_lossy().replace('%', "%%")).arg(&job.url);
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("couldn't start yt-dlp")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let errors = Arc::new(Mutex::new(String::new()));
    {
        let errors = errors.clone();
        std::thread::spawn(move || {
            let text: String = BufReader::new(stderr).lines().map_while(Result::ok).collect::<Vec<_>>().join("\n");
            *errors.lock().unwrap() = text;
        });
    }
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
    progress.connections.store(0, Ordering::Relaxed);
    *note.lock().unwrap() = None;

    match exit {
        None => Ok(Outcome::Paused),
        Some(true) if job.dest.exists() => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_video_sites() {
        assert!(is_media_url("https://www.youtube.com/watch?v=aqz-KE-bpKQ"));
        assert!(is_media_url("https://youtu.be/aqz-KE-bpKQ"));
        assert!(is_media_url("https://m.youtube.com/shorts/x"));
        assert!(!is_media_url("https://drive.usercontent.google.com/download?id=1"));
        assert!(!is_media_url("https://notyoutube.com/watch"));
    }

    #[test]
    fn lists_one_quality_per_height_then_audio() {
        let info = serde_json::json!({ "formats": [
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
