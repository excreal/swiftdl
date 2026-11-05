const DEFAULTS = { enabled: true, minSizeKB: 0, disabledSites: [] };
const $ = (id) => document.getElementById(id);
const DL_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11M7.5 10.5 12 15l4.5-4.5M5 20h14"/></svg>`;
const CHECK = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m5.5 12.5 4 4 9-9"/></svg>`;

const bytes = (n) => {
  if (!n) return "";
  const u = ["B", "KB", "MB", "GB"]; let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i ? n.toFixed(1) : Math.round(n)} ${u[i]}`;
};
const VIDEO_SITE = /(^|\.)(youtube\.com|youtube-nocookie\.com|vimeo\.com|x\.com|twitter\.com|reddit\.com|instagram\.com|tiktok\.com|facebook\.com|twitch\.tv|dailymotion\.com|bilibili\.com|xvideos\.com|xnxx\.com|hqporner\.com)$/i;
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

async function main() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const settings = await chrome.storage.sync.get(DEFAULTS);
  // Only real websites get the per-site switch, not browser or extension pages.
  let host = "";
  try { const u = new URL(tab.url); if (/^https?:$/.test(u.protocol)) host = u.hostname.replace(/^www\./, ""); } catch {}

  $("enabled").checked = settings.enabled;
  $("min-size").value = String(settings.minSizeKB);
  $("site").textContent = host;
  $("site-row").hidden = !host;
  $("site-on").checked = !settings.disabledSites.includes(host);

  $("enabled").onchange = (e) => chrome.storage.sync.set({ enabled: e.target.checked });
  $("min-size").onchange = (e) => chrome.storage.sync.set({ minSizeKB: Number(e.target.value) });
  $("site-on").onchange = () => chrome.runtime.sendMessage({ type: "toggle-site", tabId: tab.id, pageUrl: tab.url });
  $("open").onclick = () => chrome.runtime.sendMessage({ type: "show" }).then(() => window.close());

  chrome.runtime.sendMessage({ type: "ping" }).then((r) => {
    $("status").className = `status ${r?.ok ? "ok" : "off"}`;
    $("status").querySelector("span").textContent = r?.ok ? "Connected" : "Not connected";
    $("missing").hidden = !!r?.ok;
    $("main").hidden = !r?.ok;
    $("open").hidden = !r?.ok;
  });

  const { items = [] } = await chrome.runtime.sendMessage({ type: "media", tabId: tab.id, pageUrl: tab.url });
  const files = items.filter((m) => m.kind === "file");
  const streams = items.filter((m) => m.kind !== "file");
  const title = (tab.title || "download").replace(/[\\/:*?"<>|]+/g, " ").replace(/\s+/g, " ").trim().slice(0, 120);
  const ext = (m) => (/\.([a-z0-9]{2,4})(?:$|[?#])/i.exec(new URL(m.url).pathname) || [])[1]?.toLowerCase() || (m.mime.split("/")[1] || "mp4").replace(/^x-/, "").slice(0, 4);

  let html = files.map((m, i) => {
    const e = ext(m);
    const audio = /^audio/.test(m.mime) || /mp3|m4a|aac|ogg|opus|flac|wav/.test(e);
    return `<div class="m"><span class="tag">${e.toUpperCase()}</span><span class="mt"><b>${audio ? "Audio" : "Video"}</b><span title="${esc(m.url)}">${m.size ? bytes(m.size) : esc(new URL(m.url).hostname)}</span></span><button class="dl" data-i="${i}" title="Download with SwiftDL">${DL_ICON}</button></div>`;
  }).join("");
  html += streams.slice(0, 2).map((m) => `<div class="m"><span class="tag stream">${m.kind.toUpperCase()}</span><span class="mt"><b>Stream</b><span>Not supported yet</span></span></div>`).join("");
  // Video sites go through SwiftDL's quality picker, whatever the page loaded.
  if (host && VIDEO_SITE.test(host)) {
    html = `<div class="m"><span class="tag">PAGE</span><span class="mt"><b>This video</b><span>Choose the quality in SwiftDL</span></span><button class="dl" data-page title="Download with SwiftDL">${DL_ICON}</button></div>`;
  }
  $("media").innerHTML = html;
  $("found").hidden = !html;
  $("media").onclick = async (e) => {
    const b = e.target.closest(".dl");
    if (!b) return;
    const m = b.hasAttribute("data-page") ? { url: tab.url } : files[Number(b.dataset.i)];
    const name = b.hasAttribute("data-page") ? title : `${title}.${ext(m)}`;
    const r = await chrome.runtime.sendMessage({ type: "grab", url: m.url, filename: name, tabId: tab.id, pageUrl: tab.url });
    b.innerHTML = r?.ok ? CHECK : DL_ICON;
    b.classList.toggle("done", !!r?.ok);
  };
}
main();
