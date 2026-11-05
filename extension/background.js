// SwiftDL browser integration.
//
// 1. Catches downloads the browser starts and hands them to the SwiftDL app
//    (through native messaging), with the cookies and referrer the site
//    expects, so downloads behind a login keep working. If the app isn't
//    installed or refuses, the browser simply finishes the download itself.
// 2. Watches each tab's network traffic for video and audio files so the
//    page button and the popup can offer them for download.

const HOST = "com.swiftdl.host";
const DEFAULTS = { enabled: true, minSizeKB: 0, disabledSites: [] };

const getSettings = () => chrome.storage.sync.get(DEFAULTS);
const hostOf = (url) => { try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return ""; } };

// Last few events, kept for troubleshooting (service worker console or
// chrome.storage.local "log").
async function log(event) {
  console.log("SwiftDL:", event);
  const { log: entries = [] } = await chrome.storage.local.get("log");
  entries.push(`${new Date().toISOString()} ${event}`);
  await chrome.storage.local.set({ log: entries.slice(-30) });
}

// ---------- talking to the app ----------

async function cookieHeader(url) {
  try {
    const cookies = await chrome.cookies.getAll({ url });
    return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch {
    return "";
  }
}

async function mediaCookies(urls) {
  const cookies = new Map();
  for (const url of [...new Set(urls.filter(Boolean))].slice(0, 64)) {
    for (const c of await chrome.cookies.getAll({ url }).catch(() => [])) {
      cookies.set(`${c.domain}\n${c.path}\n${c.name}`, { name: c.name, value: c.value, domain: c.domain, path: c.path, secure: c.secure, hostOnly: c.hostOnly, expirationDate: c.expirationDate });
    }
  }
  return [...cookies.values()];
}

async function sendToApp({ url, referrer, filename, streamKind, tabId }) {
  const { enabled } = await getSettings();
  const tab = tabId != null ? await chrome.tabs.get(tabId).catch(() => null) : null;
  if (!enabled || await siteDisabled(tab?.url || referrer || url) || await siteDisabled(referrer || url)) {
    throw new Error("SwiftDL is disabled for this page");
  }
  // HQPorner's video lives in a cross-origin player whose document title is
  // often just "video". Use the owning tab, not the iframe, for the filename.
  // Keep the actual file URL and player referrer: the CDN still needs them.
  if (tabId != null && MEDIA_EXT.test(url) && /(^|\.)bigcdn\.cc$/i.test(hostOf(url))) {
    const name = hqpornerFilename(tab, url);
    if (name) filename = name;
  }
  const hls = streamKind === "hls" || /\.m3u8(?:$|[?#])/i.test(url);
  const detected = tabId != null ? (await mediaFor(tabId)).find((m) => m.url === url) : null;
  const mediaHeaders = detected?.requestHeaders || [];
  const observed = (name) => mediaHeaders.find(([key]) => key.toLowerCase() === name)?.[1];
  const { [`hls-context:${tabId}`]: context = [] } = hls && tabId != null ? await chrome.storage.session.get(`hls-context:${tabId}`) : {};
  const message = {
    type: "download",
    url,
    referrer: observed("referer") ?? referrer ?? "",
    filename: filename ? filename.split(/[\\/]/).pop() : "",
    cookies: hls ? "" : await cookieHeader(url),
    userAgent: observed("user-agent") || navigator.userAgent,
    mediaHeaders,
    streamKind: hls ? "hls" : undefined,
    mediaCookies: hls ? await mediaCookies([url, referrer, ...context]) : [],
  };
  const reply = await chrome.runtime.sendNativeMessage(HOST, message);
  if (!reply?.ok) throw new Error(reply?.error || "SwiftDL did not accept the download");
}

const ping = () =>
  chrome.runtime.sendNativeMessage(HOST, { type: "ping" })
    .then((r) => ({ ok: !!r?.ok, version: r?.version }))
    .catch((e) => ({ ok: false, error: String(e?.message || e) }));

function flashBadge(text, color, tabId) {
  chrome.action.setBadgeBackgroundColor({ color });
  chrome.action.setBadgeText({ text, ...(tabId ? { tabId } : {}) });
  setTimeout(() => refreshBadge(tabId), 2500);
}

// ---------- catching browser downloads ----------

// Hold Alt while clicking a link to let the browser download it instead.
let bypassUntil = 0;

async function siteDisabled(url) {
  const { disabledSites } = await getSettings();
  const host = hostOf(url);
  return !!host && disabledSites.includes(host);
}

chrome.downloads.onCreated.addListener(async (item) => {
  const settings = await getSettings();
  const url = item.finalUrl || item.url;
  await log(`created #${item.id} state=${item.state} host=${hostOf(url)}`);
  if (!settings.enabled || item.state !== "in_progress") return;
  // blob:, data: and file: URLs only exist inside the browser.
  if (!/^https?:/i.test(url)) return;
  if (Date.now() < bypassUntil) { bypassUntil = 0; await log(`#${item.id} Alt held: left to the browser`); return; }
  if (await siteDisabled(item.referrer || url)) return;
  if (settings.minSizeKB > 0 && item.totalBytes > 0 && item.totalBytes < settings.minSizeKB * 1024) return;

  // Pause first so the browser doesn't keep downloading while we ask the app.
  await chrome.downloads.pause(item.id).catch(() => {});
  try {
    const context = await browserDownloadContext(item);
    await sendToApp({ url, referrer: item.referrer || context?.referrer, filename: item.filename, tabId: context?.id, streamKind: context?.streamKind });
    await chrome.downloads.cancel(item.id).catch(() => {});
    await chrome.downloads.erase({ id: item.id }).catch(() => {});
    await log(`#${item.id} handed to SwiftDL`);
    flashBadge("✓", "#2f9e6a");
  } catch (err) {
    await log(`#${item.id} left to the browser: ${err?.message || err}`);
    await chrome.downloads.resume(item.id).catch(() => {});
    flashBadge("!", "#dc4b4b");
  }
});

// Chromium DownloadItem has no tabId. Match observed media or the actual
// referring document, never the currently active (possibly unrelated) tab.
async function browserDownloadContext(item) {
  const tabs = await chrome.tabs.query({}).catch(() => []);
  const matches = [];
  for (const tab of tabs) {
    const data = await chrome.storage.session.get([key(tab.id), `media-frames:${tab.id}`]);
    const media = data[key(tab.id)] || [];
    const frames = data[`media-frames:${tab.id}`] || {};
    const detected = media.find((m) => m.url === item.url || m.url === item.finalUrl);
    if (detected
        || (item.referrer && (tab.url === item.referrer || Object.values(frames).includes(item.referrer)))) {
      matches.push({ ...tab, referrer: detected?.pageUrl || frames[detected?.frameId] || tab.url, streamKind: detected?.kind === "hls" ? "hls" : undefined });
    }
  }
  for (const tab of matches) {
    if (await siteDisabled(tab.url)) throw new Error("SwiftDL is disabled for this page");
  }
  return matches.length === 1 ? matches[0] : null;
}

// ---------- media detection ----------
// Per tab, the video/audio files and streams the page has loaded. Kept in
// session storage because the service worker can be stopped at any time.

const MEDIA_EXT = /\.(mp4|m4v|webm|mkv|mov|avi|flv|wmv|mp3|m4a|aac|ogg|oga|opus|flac|wav)(?:$|[?#])/i;
function hqpornerFilename(tab, url) {
  try {
    const page = new URL(tab?.url);
    const file = new URL(url);
    if (!/(^|\.)hqporner\.com$/i.test(page.hostname) || !/^\/hdporn\/\d+-[^/]+\.html$/.test(page.pathname)) return "";
    // Do not rename unrelated downloads or ads merely because the tab is HQP.
    if (!/(^|\.)bigcdn\.cc$/i.test(file.hostname)) return "";
    const ext = MEDIA_EXT.exec(url)?.[1];
    const title = (tab.title || "").replace(/\s*[-–|]\s*hqporner\.com\s*$/i, "")
      .replace(/[\\/:*?"<>|\u0000-\u001f]+/g, " ").replace(/\s+/g, " ").trim().replace(/[. ]+$/, "").slice(0, 120);
    return title && ext ? `${title}.${ext.toLowerCase()}` : "";
  } catch { return ""; }
}
const STREAM_EXT = /\.(m3u8|mpd)(?:$|[?#])/i;
const SEGMENT = /\.(ts|m4s|aac)(?:$|[?#])|[?&](range|bytestart)=/i;
const MIN_FILE_BYTES = 512 * 1024;
const key = (tabId) => `media:${tabId}`;
const updates = new Map();
function updateTab(tabId, work) {
  const next = (updates.get(tabId) || Promise.resolve()).catch(() => {}).then(work);
  updates.set(tabId, next);
  next.finally(() => { if (updates.get(tabId) === next) updates.delete(tabId); }).catch(() => {});
  return next;
}

function classify(url, mime, size) {
  if (/mpegurl/i.test(mime) || /\.m3u8(?:$|[?#])/i.test(url)) return "hls";
  if (/dash\+xml/i.test(mime) || /\.mpd(?:$|[?#])/i.test(url)) return "dash";
  const isMedia = /^(video|audio)\//i.test(mime) || MEDIA_EXT.test(url);
  if (!isMedia || /mp2t/i.test(mime) || SEGMENT.test(url)) return null;
  if (size && size < MIN_FILE_BYTES) return null;
  return "file";
}

function mediaIdentity(url, kind) {
  if (kind !== "hls") return url;
  const identity = new URL(url);
  // Only discard known rotating credentials/cache busters for display identity.
  // Keep media selectors (id, language, quality, etc.) and the actual download URL.
  for (const name of [...identity.searchParams.keys()]) {
    if (/^(token|sig|signature|expires?|exp|policy|key-pair-id|hdnts|hdnea|rn|rbuf|_)$/i.test(name)) identity.searchParams.delete(name);
  }
  identity.hash = "";
  identity.searchParams.sort();
  return identity.href;
}

function header(headers, name) {
  return headers?.find((h) => h.name.toLowerCase() === name)?.value || "";
}

// Frame URLs are not necessarily the Referer sent on the wire: referrer
// policy often reduces them to an origin. Never capture Cookie/Authorization
// here; cookies retain their domain/path scope through mediaCookies instead.
const captureRequestHeaders = (d) => {
  if (d.tabId < 0) return;
  const headers = (d.requestHeaders || []).filter((h) => ["referer", "origin", "user-agent", "accept", "accept-language"].includes(h.name.toLowerCase()) && typeof h.value === "string")
    .map((h) => [h.name, h.value]);
  if (!headers.some(([name]) => name.toLowerCase() === "referer")) headers.push(["Referer", ""]);
  return updateTab(d.tabId, async () => {
    const requestKey = `media-requests:${d.tabId}`;
    const { [requestKey]: requests = [] } = await chrome.storage.session.get(requestKey);
    await chrome.storage.session.set({ [requestKey]: [{ id: d.requestId, url: d.url, headers }, ...requests.filter((r) => r.id !== d.requestId)].slice(0, 128) });
  });
};
const requestFilter = { urls: ["http://*/*", "https://*/*"], types: ["media", "xmlhttprequest", "other", "object"] };
try {
  chrome.webRequest.onBeforeSendHeaders.addListener(captureRequestHeaders, requestFilter, ["requestHeaders", "extraHeaders"]);
} catch {
  // Firefox does not expose Chromium's extraHeaders option.
  chrome.webRequest.onBeforeSendHeaders.addListener(captureRequestHeaders, requestFilter, ["requestHeaders"]);
}

chrome.webRequest.onResponseStarted.addListener(
  async (d) => {
    if (d.tabId < 0 || d.statusCode >= 400) return;
    const mime = header(d.responseHeaders, "content-type").split(";")[0].trim().toLowerCase();
    const range = header(d.responseHeaders, "content-range");
    const size = Number(range.split("/")[1]) || Number(header(d.responseHeaders, "content-length")) || 0;
    const kind = classify(d.url, mime, size);
    // Keep only observed media/segment/key paths, not all browser cookies.
    if (kind || SEGMENT.test(d.url) || /octet-stream/i.test(mime)) {
      await updateTab(d.tabId, async () => {
        const contextKey = `hls-context:${d.tabId}`;
        const { [contextKey]: paths = [] } = await chrome.storage.session.get(contextKey);
        const u = new URL(d.url);
        const path = `${u.origin}${u.pathname}`;
        await chrome.storage.session.set({ [contextKey]: [path, ...paths.filter((p) => p !== path)].slice(0, 62) });
      });
    }
    if (!kind) return;
    // Any media URL can be signed, including ordinary MP4 files.
    const url = d.url;
    await updateTab(d.tabId, async () => {
      const { [key(d.tabId)]: list = [] } = await chrome.storage.session.get(key(d.tabId));
      const { [`media-requests:${d.tabId}`]: requests = [] } = await chrome.storage.session.get(`media-requests:${d.tabId}`);
      const request = requests.find((r) => r.id === d.requestId && r.url === d.url);
      const previous = list.find((m) => m.kind === kind && mediaIdentity(m.url, kind) === mediaIdentity(url, kind));
      if (previous) list.splice(list.indexOf(previous), 1);
      list.unshift({ url, kind, mime, size, frameId: d.frameId, pageUrl: d.documentUrl, requestHeaders: request?.headers || previous?.requestHeaders, at: Date.now() });
      await chrome.storage.session.set({ [key(d.tabId)]: list.slice(0, 25) });
      await refreshBadge(d.tabId);
    });
  },
  { urls: ["http://*/*", "https://*/*"], types: ["media", "xmlhttprequest", "other", "object"] },
  ["responseHeaders"],
);

async function mediaFor(tabId) {
  const { [key(tabId)]: list = [] } = await chrome.storage.session.get(key(tabId));
  return list;
}

async function refreshBadge(tabId) {
  if (tabId == null) { chrome.action.setBadgeText({ text: "" }); return; }
  const files = (await mediaFor(tabId)).filter((m) => m.kind === "file" || m.kind === "hls").length;
  chrome.action.setBadgeBackgroundColor({ color: "#0a84ff", tabId });
  chrome.action.setBadgeText({ text: files ? String(files) : "", tabId });
}

// A new page in the tab starts a fresh list.
chrome.tabs.onUpdated.addListener((tabId, info) => {
  if (info.status === "loading") updateTab(tabId, () => chrome.storage.session.remove([key(tabId), `hls-context:${tabId}`, `media-frames:${tabId}`, `media-requests:${tabId}`])).then(() => refreshBadge(tabId));
});
chrome.tabs.onRemoved.addListener((tabId) => updateTab(tabId, () => chrome.storage.session.remove([key(tabId), `hls-context:${tabId}`, `media-frames:${tabId}`, `media-requests:${tabId}`])));

// ---------- right-click menu ----------

function createMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "swiftdl-link", title: "Download with SwiftDL", contexts: ["link"] });
    chrome.contextMenus.create({ id: "swiftdl-media", title: "Download with SwiftDL", contexts: ["video", "audio", "image"] });
  });
}
chrome.runtime.onInstalled.addListener(() => { createMenus(); ping(); });
chrome.runtime.onStartup.addListener(() => { createMenus(); ping(); });

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url || !/^https?:/i.test(url)) return flashBadge("!", "#dc4b4b", tab?.id);
  try {
    await sendToApp({ url, referrer: info.frameUrl || info.pageUrl || tab?.url, tabId: tab?.id });
    flashBadge("✓", "#2f9e6a", tab?.id);
  } catch (err) {
    await log(`context menu: could not reach the app: ${err?.message || err}`);
    flashBadge("!", "#dc4b4b", tab?.id);
  }
});

// ---------- messages from the page button and the popup ----------

chrome.runtime.onMessage.addListener((msg, sender, respond) => {
  const tabId = msg.tabId ?? sender.tab?.id;
  const pageUrl = msg.pageUrl || sender.url || sender.tab?.url || "";
  (async () => {
    switch (msg?.type) {
      case "frame-context":
        if (tabId != null && /^https?:/i.test(sender.url || "")) {
          await updateTab(tabId, async () => {
            const frameKey = `media-frames:${tabId}`;
            const { [frameKey]: frames = {} } = await chrome.storage.session.get(frameKey);
            frames[sender.frameId ?? 0] = sender.url;
            await chrome.storage.session.set({ [frameKey]: frames });
          });
        }
        return { ok: true };
      case "ping": return ping();
      case "bypass": bypassUntil = Date.now() + 4000; return { ok: true };
      case "media": {
        const { enabled } = await getSettings();
        const tab = tabId != null ? await chrome.tabs.get(tabId).catch(() => null) : null;
        return { items: await mediaFor(tabId), disabled: !enabled || await siteDisabled(tab?.url || pageUrl) || await siteDisabled(pageUrl) };
      }
      case "grab":
        try {
          const detected = (await mediaFor(tabId)).find((m) => m.url === msg.url);
          const { [`media-frames:${tabId}`]: frames = {} } = await chrome.storage.session.get(`media-frames:${tabId}`);
          const referrer = detected?.pageUrl || frames[detected?.frameId] || pageUrl;
          await sendToApp({ url: msg.url, referrer, filename: msg.filename, streamKind: detected?.kind === "hls" || msg.streamKind === "hls" ? "hls" : undefined, tabId });
          return { ok: true };
        } catch (e) {
          return { ok: false, error: String(e?.message || e) };
        }
      case "magnet": {
        const { enabled } = await getSettings();
        if (!enabled || (await siteDisabled(pageUrl))) return { ok: false };
        try {
          await sendToApp({ url: msg.url, referrer: pageUrl });
          flashBadge("✓", "#2f9e6a", tabId);
          return { ok: true };
        } catch (e) {
          return { ok: false, error: String(e?.message || e) };
        }
      }
      case "show":
        return chrome.runtime.sendNativeMessage(HOST, { type: "show" }).catch((e) => ({ ok: false, error: String(e) }));
      case "toggle-site": {
        const { disabledSites } = await getSettings();
        const host = hostOf(pageUrl);
        const next = disabledSites.includes(host) ? disabledSites.filter((h) => h !== host) : [...disabledSites, host];
        await chrome.storage.sync.set({ disabledSites: next });
        return { disabled: next.includes(host) };
      }
    }
  })().then(respond);
  return true; // async response
});
