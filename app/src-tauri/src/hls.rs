//! Bounded playlist detection and VOD checks. yt-dlp owns segment downloading,
//! retries, AES-128 keys and fragment checkpoints; this module never fetches keys.
use anyhow::{Result, bail};
use crate::media::{Cookie, request_headers};
use std::collections::HashSet;
use std::time::Duration;
use tauri::Url;

const MAX_PLAYLIST: usize = 512 * 1024;

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(15)).build()?)
}

async fn response(client: &reqwest::Client, url: &str, headers: &[(String, String)], cookies: &[Cookie], sniff: bool) -> Result<reqwest::Response> {
    let mut target = Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid HLS URL"))?;
    for _ in 0..10 {
        if !matches!(target.scheme(), "http" | "https") { bail!("HLS playlists must use HTTP or HTTPS"); }
        let mut request = client.get(target.clone()).headers(request_headers(&target, headers, cookies));
        if sniff { request = request.header("Range", "bytes=0-1023"); }
        let response = request.send().await.map_err(|_| anyhow::anyhow!("Could not reach the HLS server; the link may have expired"))?;
        if response.status().is_redirection() {
            let location = response.headers().get("location").and_then(|v| v.to_str().ok()).ok_or_else(|| anyhow::anyhow!("HLS redirect has no valid destination"))?;
            target = target.join(location).map_err(|_| anyhow::anyhow!("Invalid HLS redirect"))?;
            continue;
        }
        if !response.status().is_success() { bail!("HLS server returned {}. Refresh the page and capture the stream again if the link has expired.", response.status()); }
        return Ok(response);
    }
    bail!("Too many HLS redirects")
}

fn signature(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}').trim_start().starts_with("#EXTM3U")
}

/// Only called for extensionless web links, before falling back to file probing.
pub async fn detect(url: &str, headers: &[(String, String)]) -> Result<bool> {
    tokio::time::timeout(Duration::from_secs(10), detect_inner(url, headers)).await
        .map_err(|_| anyhow::anyhow!("Timed out inspecting the stream response"))?
}

async fn detect_inner(url: &str, headers: &[(String, String)]) -> Result<bool> {
    let cookies = crate::media::scoped_cookies(url, headers, &[]);
    let client = client()?;
    let mut resp = response(&client, url, headers, &cookies, true).await?;
    if resp.headers().get("content-type").and_then(|h| h.to_str().ok()).is_some_and(|h| h.to_ascii_lowercase().contains("mpegurl")) { return Ok(true); }
    let mut prefix = Vec::new();
    while prefix.len() < 1024 {
        let Some(chunk) = resp.chunk().await.map_err(|_| anyhow::anyhow!("Could not inspect the stream response"))? else { break };
        prefix.extend_from_slice(&chunk[..chunk.len().min(1024 - prefix.len())]);
        if signature(&prefix) { return Ok(true); }
        if prefix.len() >= 32 { return Ok(false); }
    }
    Ok(signature(&prefix))
}

fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let fields = line.split_once(':')?.1;
    let (mut start, mut quoted) = (0, false);
    for (index, c) in fields.char_indices().chain(std::iter::once((fields.len(), ','))) {
        if c == '"' { quoted = !quoted; }
        if c == ',' && !quoted {
            if let Some((key, value)) = fields[start..index].split_once('=') {
                if key.trim() == name { return Some(value.trim().trim_matches('"')); }
            }
            start = index + 1;
        }
    }
    None
}

fn playlist_children(text: &str) -> Result<Vec<String>> {
    let mut children = Vec::new();
    let mut variant = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("#EXT-X-KEY:") || line.starts_with("#EXT-X-SESSION-KEY:") {
            let method = attribute(line, "METHOD");
            if method != Some("NONE") && (method != Some("AES-128") || attribute(line, "KEYFORMAT").is_some_and(|v| v != "identity")) {
                bail!("This HLS stream uses DRM or unsupported encryption. Only clear streams and standard AES-128 are supported.");
            }
        }
        if line.starts_with("#EXT-X-STREAM-INF:") { variant = true; }
        else if variant && !line.is_empty() && !line.starts_with('#') { children.push(line.to_string()); variant = false; }
        if line.starts_with("#EXT-X-MEDIA:") && matches!(attribute(line, "TYPE"), Some("AUDIO" | "VIDEO")) {
            if let Some(uri) = attribute(line, "URI") { children.push(uri.to_string()); }
        }
    }
    if variant { bail!("Invalid HLS master playlist: a variant URL is missing"); }
    if children.is_empty() && !text.lines().any(|line| line.trim() == "#EXT-X-ENDLIST") {
        bail!("Live HLS recording is not supported yet. Choose a finished video (VOD).");
    }
    Ok(children)
}

#[cfg(test)]
pub async fn validate_vod(url: &str, headers: &[(String, String)], cookies: &[Cookie]) -> Result<()> {
    vod_playlists(url, headers, cookies).await.map(|_| ())
}

/// Inspect renditions independently: a dead or protected sibling must not
/// prevent downloading a finished clear rendition. Return only usable URLs.
pub async fn vod_playlists(url: &str, headers: &[(String, String)], cookies: &[Cookie]) -> Result<HashSet<String>> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let client = client()?;
        let mut pending = vec![url.to_string()];
        let mut seen = HashSet::new();
        let mut playable = HashSet::new();
        let mut last_error = None;
        while let Some(url) = pending.pop() {
            if !seen.insert(url.clone()) { continue; }
            if seen.len() > 32 { bail!("This HLS master playlist has too many variants to inspect safely"); }
            let inspected: Result<_> = async {
                let mut resp = response(&client, &url, headers, cookies, false).await?;
                let base = resp.url().clone();
                let mut data = Vec::new();
                while let Some(chunk) = resp.chunk().await.map_err(|_| anyhow::anyhow!("Could not read the HLS playlist"))? {
                    if data.len() + chunk.len() > MAX_PLAYLIST { bail!("HLS playlist exceeds the inspection size limit"); }
                    data.extend_from_slice(&chunk);
                }
                if !signature(&data) { bail!("The server did not return an HLS playlist. The link may have expired or require a login."); }
                let text = std::str::from_utf8(&data).map_err(|_| anyhow::anyhow!("HLS playlist is not valid UTF-8"))?;
                Ok((base, playlist_children(text)?))
            }.await;
            match inspected {
                Ok((base, children)) if children.is_empty() => {
                    playable.insert(url);
                    playable.insert(base.to_string());
                }
                Ok((base, children)) => {
                    for child in children {
                        match base.join(&child) {
                            Ok(url) => pending.push(url.to_string()),
                            Err(_) => last_error = Some(anyhow::anyhow!("Invalid URL in HLS master playlist")),
                        }
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        if playable.is_empty() { return Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HLS master playlist contains no finished media playlist"))); }
        Ok(playable)
    }).await.map_err(|_| anyhow::anyhow!("Timed out checking HLS playlists"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vod_master_audio_and_encryption() {
        let master = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,URI=\"audio.m3u8?sig=a,b\"\n#EXT-X-STREAM-INF:BANDWIDTH=123\nvideo.m3u8?token=exact\n";
        assert_eq!(playlist_children(master).unwrap(), ["audio.m3u8?sig=a,b", "video.m3u8?token=exact"]);
        assert!(playlist_children("#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n#EXTINF:1,\na.ts\n#EXT-X-ENDLIST\n").is_ok());
        assert!(playlist_children("#EXTM3U\n#EXTINF:1,\na.ts\n").unwrap_err().to_string().contains("Live HLS"));
        assert!(playlist_children("#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"key\"\n#EXT-X-ENDLIST").unwrap_err().to_string().contains("DRM"));
    }
}
