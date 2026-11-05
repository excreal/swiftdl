#!/usr/bin/env python3
"""Generate local HLS fixtures and exercise SwiftDL's real yt-dlp runner.

Requires ffmpeg, ffprobe and yt-dlp on PATH. No public network or browser
cookies are used. Run from any directory: python3 tools/test-hls.py
"""
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.parse


def main():
    for tool in ("ffmpeg", "ffprobe", "yt-dlp", "cargo"):
        if not shutil.which(tool):
            raise SystemExit(f"Missing fixture dependency: {tool}")
    with tempfile.TemporaryDirectory(prefix="swiftdl-hls-") as temporary:
        root = Path(temporary)
        for kind in ("ts", "fmp4", "aes"):
            folder = root / kind
            folder.mkdir()
            extra = []
            if kind == "fmp4":
                extra = ["-hls_segment_type", "fmp4", "-hls_fmp4_init_filename", "init.mp4"]
            if kind == "aes":
                (folder / "key").write_bytes(bytes(range(16)))
                (folder / "keyinfo").write_text(f"key\n{folder / 'key'}\n")
                extra = ["-hls_key_info_file", str(folder / "keyinfo")]
            subprocess.run([
                "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
                "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10",
                "-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=44100",
                "-t", "8", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "10",
                "-c:a", "aac", "-f", "hls", "-hls_time", "1", "-hls_playlist_type", "vod",
                *extra, str(folder / "media.m3u8"),
            ], check=True)
            (folder / "master.m3u8").write_text(
                '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=400000,RESOLUTION=160x90,CODECS="avc1.42c00a,mp4a.40.2"\nmedia.m3u8\n'
            )

        separate = root / "separate"
        separate.mkdir()
        for name, selection in (("audio", "-vn"), ("video", "-an")):
            subprocess.run([
                "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
                "-i", str(root / "ts" / "media.m3u8"), selection, "-c", "copy",
                "-f", "hls", "-hls_time", "1", "-hls_playlist_type", "vod",
                str(separate / f"{name}.m3u8"),
            ], check=True)
        (separate / "master.m3u8").write_text(
            '#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="English",DEFAULT=YES,AUTOSELECT=YES,URI="audio.m3u8"\n'
            '#EXT-X-STREAM-INF:BANDWIDTH=400000,RESOLUTION=160x90,CODECS="avc1.42c00a,mp4a.40.2",AUDIO="audio"\nvideo.m3u8\n'
        )

        mixed = root / "mixed"
        mixed.mkdir()
        (mixed / "master.m3u8").write_text(
            '#EXTM3U\n'
            '#EXT-X-STREAM-INF:BANDWIDTH=400000,RESOLUTION=160x90,CODECS="avc1.42c00a,mp4a.40.2"\n../ts/media.m3u8\n'
            '#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=320x180,CODECS="avc1.42c00a,mp4a.40.2"\n../drm.m3u8\n'
            '#EXT-X-STREAM-INF:BANDWIDTH=1200000,RESOLUTION=480x270,CODECS="avc1.42c00a,mp4a.40.2"\n../live.m3u8\n'
            '#EXT-X-STREAM-INF:BANDWIDTH=1600000,RESOLUTION=640x360,CODECS="avc1.42c00a,mp4a.40.2"\n../expired.m3u8\n'
        )

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass  # Do not log signed URLs or credentials, even fixture ones.

            def do_GET(self):
                url = urllib.parse.urlsplit(self.path)
                if url.path == "/redirect.m3u8":
                    self.send_response(302)
                    self.send_header("Location", f"http://localhost:{self.server.server_port}/cookie-check.m3u8")
                    self.end_headers()
                    return
                if url.path == "/cookie-check.m3u8":
                    if self.headers.get("Cookie"):
                        self.send_error(403, "Cookies leaked across hosts")
                    else:
                        self.send_response(200)
                        self.end_headers()
                        self.wfile.write(b"#EXTM3U\n#EXTINF:1,\na.ts\n#EXT-X-ENDLIST\n")
                    return
                if (self.headers.get("Referer") != "https://fixture.test/watch"
                        or self.headers.get("Origin") != "https://fixture.test"
                        or self.headers.get("User-Agent") != "SwiftDL-HLS-test"
                        or self.headers.get("Accept") != "*/*"
                        or self.headers.get("Accept-Language") != "en-US,en;q=0.9"
                        or "session=fixture-only" not in self.headers.get("Cookie", "")):
                    self.send_error(403)
                    return
                if url.path == "/expired.m3u8":
                    self.send_error(403)
                    return
                if url.path == "/live.m3u8":
                    data = b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\na.ts\n"
                elif url.path == "/drm.m3u8":
                    data = b'#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI="key"\n#EXT-X-ENDLIST\n'
                elif url.path == "/cycle.m3u8":
                    data = b"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=123\ncycle.m3u8\n"
                elif url.path == "/detect":
                    if url.query != "sig=exact%2Fvalue&rn=keep":
                        self.send_error(403)
                        return
                    data = (root / "ts" / "master.m3u8").read_bytes()
                else:
                    if url.path.endswith("/master.m3u8") and url.query != "sig=exact%2Fvalue&rn=keep":
                        self.send_error(403)
                        return
                    file = (root / url.path.lstrip("/")).resolve()
                    if not file.is_relative_to(root) or not file.is_file():
                        self.send_error(404)
                        return
                    data = file.read_bytes()
                    if file.suffix in (".ts", ".m4s"):
                        time.sleep(0.4)  # Makes fragment-level pause reproducible.
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream" if url.path == "/detect" else "application/vnd.apple.mpegurl" if url.path.endswith(".m3u8") else "application/octet-stream")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass  # Expected when pausing a download.

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env = dict(os.environ, SWIFTDL_HLS_FIXTURE=f"http://127.0.0.1:{server.server_port}")
        try:
            subprocess.run([
                "cargo", "test", "-p", "swiftdl-app", "local_hls_vod_download_and_resume",
                "--", "--ignored", "--nocapture",
            ], cwd=Path(__file__).resolve().parent.parent, env=env, check=True)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    main()
