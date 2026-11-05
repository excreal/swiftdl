// SwiftDL page helper.
// - Shows a "Download video" button over video players. Clicking it lists
//   the media files the page loaded and sends the chosen one to SwiftDL.
// - Alt+click on a link tells the background to let the browser handle
//   that one download itself.
// Everything lives in a closed shadow root so page styles can't touch it.

(() => {
  if (window.__swiftdlLoaded) return;
  window.__swiftdlLoaded = true;
  chrome.runtime.sendMessage({ type: "frame-context" }).catch(() => {});

  // Alt+click: skip SwiftDL for this download.
  document.addEventListener("click", (e) => {
    if (e.altKey && e.target.closest?.("a[href]")) chrome.runtime.sendMessage({ type: "bypass" }).catch(() => {});
  }, true);

  // Magnet links aren't downloads, so the browser would hand them to another
  // app. Send them to SwiftDL; if it can't take them, open the link as usual.
  document.addEventListener("click", async (e) => {
    const a = e.target.closest?.('a[href^="magnet:"]');
    if (!a || e.altKey || e.button !== 0 || e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    let res;
    try { res = await chrome.runtime.sendMessage({ type: "magnet", url: a.href }); } catch { res = null; }
    if (!res?.ok) location.href = a.href;
  }, true);

  const MIN_W = 200, MIN_H = 112;
  // Video sites SwiftDL downloads through yt-dlp: the page itself is sent,
  // and the quality is chosen in SwiftDL.
  const VIDEO_SITE = /(^|\.)(youtube\.com|youtube-nocookie\.com|vimeo\.com|x\.com|twitter\.com|reddit\.com|instagram\.com|tiktok\.com|facebook\.com|twitch\.tv|dailymotion\.com|bilibili\.com|xvideos\.com|xnxx\.com|hqporner\.com)$/i;
  const onVideoSite = () => VIDEO_SITE.test(location.hostname);
  const LOGO = `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="1.5" y="1.5" width="21" height="21" rx="6" fill="#2a2c32"/><path d="M8 7l4 4 4-4" stroke="#fff" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round" opacity=".55"/><path d="M8 11.3l4 4 4-4" stroke="#fff" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/><rect x="7.6" y="17" width="8.8" height="1.9" rx=".95" fill="#fff"/></svg>`;

  let host, root, wrap, btn, panel, video = null, hideTimer = 0, raf = 0, open = false;

  function build() {
    host = document.createElement("swiftdl-overlay");
    host.style.cssText = "all:initial;position:fixed;inset:0 auto auto 0;width:0;height:0;z-index:2147483647;";
    root = host.attachShadow({ mode: "closed" });
    root.innerHTML = `<style>
      :host { all: initial; }
      * { box-sizing: border-box; font-family: "Segoe UI Variable Text", "Segoe UI", -apple-system, system-ui, sans-serif; }
      /* max-content: anchored at the video's right edge, the wrap would
         otherwise only get the space left before the window edge, and on a
         large video "Download video" broke onto two lines. */
      .wrap { position: fixed; width: max-content; display: flex; flex-direction: column; align-items: flex-end; gap: 6px; transform: translateX(-100%); opacity: 0; transition: opacity .15s; pointer-events: none; }
      .wrap.on { opacity: 1; pointer-events: auto; }
      .btn { display: inline-flex; align-items: center; gap: 7px; height: 30px; padding: 0 12px 0 8px; white-space: nowrap; border: 0; border-radius: 9px;
        background: rgba(20, 21, 26, .86); color: #fff; font-size: 12.5px; font-weight: 600; letter-spacing: -.005em; cursor: pointer;
        box-shadow: 0 0 0 1px rgba(255,255,255,.12), 0 6px 20px rgba(0,0,0,.35); backdrop-filter: blur(14px) saturate(1.5); }
      .btn:hover { background: rgba(30, 31, 38, .94); }
      .btn .chev { opacity: .6; margin-left: 2px; transition: transform .15s; }
      .wrap.open .btn .chev { transform: rotate(180deg); }
      .panel { width: 320px; max-height: 300px; overflow: auto; padding: 6px; border-radius: 12px; background: rgba(22, 23, 28, .94); color: #eef0f3;
        box-shadow: 0 0 0 1px rgba(255,255,255,.1), 0 16px 48px rgba(0,0,0,.5); backdrop-filter: blur(18px) saturate(1.5); display: none; }
      .wrap.open .panel { display: block; animation: pop .14s ease-out; }
      @keyframes pop { from { opacity: 0; transform: translateY(-4px); } }
      .head { padding: 6px 8px 6px; font-size: 11.5px; font-weight: 600; color: #8a8e98; }
      .item { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px; border: 0; border-radius: 8px; background: none; color: inherit; text-align: left; cursor: pointer; }
      .item:hover { background: rgba(255,255,255,.07); }
      .item[disabled] { cursor: default; opacity: .55; }
      .item[disabled]:hover { background: none; }
      .tag { flex: none; min-width: 42px; height: 20px; padding: 0 6px; border-radius: 5px; display: grid; place-items: center; font-size: 10.5px; font-weight: 700; letter-spacing: .03em;
        background: rgba(255,255,255,.1); color: #e6e8ec; }
      .tag.stream { background: rgba(255,255,255,.08); color: #8a8e98; }
      .meta { flex: 1; min-width: 0; }
      .meta b { display: block; font-size: 12.5px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .meta span { display: block; margin-top: 1px; font-size: 11.5px; color: #8a8e98; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .msg { padding: 10px 8px 12px; font-size: 12.5px; color: #a4a8b1; line-height: 1.5; }
      .msg.ok { color: #4cb782; } .msg.err { color: #eb6b6b; }
    </style>
    <div class="wrap"><button class="btn" title="Download with SwiftDL">${LOGO}<span>Download video</span><svg class="chev" width="10" height="10" viewBox="0 0 10 10"><path d="M2 3.5l3 3 3-3" stroke="#fff" stroke-width="1.4" fill="none" stroke-linecap="round"/></svg></button><div class="panel"></div></div>`;
    wrap = root.querySelector(".wrap");
    btn = root.querySelector(".btn");
    panel = root.querySelector(".panel");
    btn.addEventListener("click", (e) => { e.stopPropagation(); e.preventDefault(); toggle(); });
    panel.addEventListener("click", (e) => {
      e.stopPropagation();
      const item = e.target.closest(".item:not([disabled])");
      if (item) grab(item.dataset.url, item.dataset.name, item.dataset.kind);
    });
    wrap.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    wrap.addEventListener("mouseleave", () => scheduleHide());
    mount();
  }

  function mount() {
    const parent = document.fullscreenElement || document.documentElement;
    if (host && host.parentNode !== parent) parent.append(host);
  }
  document.addEventListener("fullscreenchange", () => host && mount());

  const bytes = (n) => {
    if (!n) return "";
    const u = ["B", "KB", "MB", "GB"]; let i = 0;
    while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
    return `${n < 10 && i ? n.toFixed(1) : Math.round(n)} ${u[i]}`;
  };
  const extOf = (url, mime) => (/\.([a-z0-9]{2,4})(?:$|[?#])/i.exec(new URL(url).pathname) || [])[1]?.toLowerCase() || (mime.split("/")[1] || "").replace(/^x-/, "").slice(0, 4) || "mp4";
  const title = () => (document.title || "video").replace(/[\\/:*?"<>|]+/g, " ").replace(/\s+/g, " ").trim().slice(0, 120);

  function position() {
    if (!video || !wrap.classList.contains("on")) { raf = 0; return; }
    const r = video.getBoundingClientRect();
    if (r.width < MIN_W || r.height < MIN_H || r.bottom < 0 || r.top > innerHeight) { hide(); raf = 0; return; }
    wrap.style.left = `${Math.min(innerWidth - 8, r.right - 10)}px`;
    wrap.style.top = `${Math.max(8, r.top + 10)}px`;
    raf = requestAnimationFrame(position);
  }
  function show(v) {
    if (!host) build();
    mount();
    if (video !== v) { video = v; setOpen(false); }
    clearTimeout(hideTimer);
    wrap.classList.add("on");
    if (!raf) raf = requestAnimationFrame(position);
  }
  function hide() { wrap?.classList.remove("on"); setOpen(false); video = null; }
  function scheduleHide() { clearTimeout(hideTimer); if (!open) hideTimer = setTimeout(hide, 1600); }
  function setOpen(v) { open = v; wrap?.classList.toggle("open", v); }

  async function toggle() {
    if (open) { setOpen(false); return; }
    setOpen(true);
    panel.innerHTML = `<div class="msg">Looking for downloadable files…</div>`;
    let res;
    try { res = await chrome.runtime.sendMessage({ type: "media" }); } catch { res = { items: [] }; }
    const items = [...(res?.items || [])];
    const src = video?.currentSrc;
    if (src && /^https?:/i.test(src) && !items.some((m) => m.url === src)) items.unshift({ url: src, kind: "file", mime: "video/mp4", size: 0 });
    const files = items.filter((m) => m.kind === "file" || m.kind === "hls");
    const streams = items.filter((m) => m.kind === "dash");
    const name = title();
    let html = onVideoSite() ? `<button class="item" data-url="${encodeURIComponent(location.href)}" data-name="${name.replace(/"/g, "'")}"><span class="tag">PAGE</span><span class="meta"><b>This video</b><span>Choose the quality in SwiftDL</span></span></button>` : "";
    if (files.length) {
      html += `<div class="head">Video and audio on this page</div>`;
      html += files.map((m) => {
        if (m.kind === "hls") return `<button class="item" data-url="${encodeURIComponent(m.url)}" data-kind="hls" data-name="${name.replace(/"/g, "'")}"><span class="tag">HLS</span><span class="meta"><b>HLS stream</b><span>Choose the quality in SwiftDL</span></span></button>`;
        const ext = extOf(m.url, m.mime || "");
        const kind = /^audio/.test(m.mime) || /mp3|m4a|aac|ogg|opus|flac|wav/.test(ext) ? "Audio" : "Video";
        return `<button class="item" data-url="${encodeURIComponent(m.url)}" data-name="${name.replace(/"/g, "'")}.${ext}"><span class="tag">${ext.toUpperCase()}</span><span class="meta"><b>${kind}${m.size ? ` · ${bytes(m.size)}` : ""}</b><span>${new URL(m.url).hostname}</span></span></button>`;
      }).join("");
    }
    if (streams.length) {
      html += `<div class="head">Streams</div>` + streams.slice(0, 3).map((m) =>
        `<button class="item" disabled><span class="tag stream">${m.kind.toUpperCase()}</span><span class="meta"><b>Streaming video</b><span>Not supported yet</span></span></button>`).join("");
    }
    if (!html && !streams.length) html = `<div class="msg">No downloadable file found yet. Play the video for a moment, then try again.</div>`;
    else if (!files.length && streams.length) html += `<div class="msg">DASH streams are not supported yet. HLS VOD streams can be downloaded.</div>`;
    panel.innerHTML = html;
  }

  async function grab(url, filename, streamKind) {
    panel.innerHTML = `<div class="msg">Sending to SwiftDL…</div>`;
    let res;
    try { res = await chrome.runtime.sendMessage({ type: "grab", url: decodeURIComponent(url), filename, streamKind }); } catch (e) { res = { ok: false, error: String(e) }; }
    panel.innerHTML = res?.ok
      ? `<div class="msg ok">Sent to SwiftDL ✓</div>`
      : `<div class="msg err">Couldn't reach SwiftDL. Make sure the app is installed and has been opened once.</div>`;
    setTimeout(() => { setOpen(false); scheduleHide(); }, 1800);
  }

  // Find the video under the pointer, even when the site layers controls
  // on top of it.
  let last = 0, disabled = null;
  document.addEventListener("mousemove", async (e) => {
    const now = performance.now();
    if (now - last < 120) return;
    last = now;
    if (host && e.composedPath().includes(host)) return;
    const v = document.elementsFromPoint(e.clientX, e.clientY).find((el) => el.tagName === "VIDEO");
    if (v) {
      const r = v.getBoundingClientRect();
      if (r.width < MIN_W || r.height < MIN_H) return;
      if (disabled === null) {
        try { disabled = (await chrome.runtime.sendMessage({ type: "media" }))?.disabled ?? false; } catch { disabled = false; }
      }
      if (!disabled) show(v);
    } else if (video) scheduleHide();
  }, { passive: true });
  document.addEventListener("click", (e) => { if (open && host && !e.composedPath().includes(host)) setOpen(false); }, true);
  chrome.storage.onChanged.addListener((c) => { if (c.disabledSites) disabled = null; });
})();
