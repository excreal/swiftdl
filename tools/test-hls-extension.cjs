// Run with: node tools/test-hls-extension.cjs
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

async function main() {
  const state = {};
  const messages = [];
  const cookieLookups = [];
  let response, message, badge, contextMenu, downloadCreated, requestHeaders, tabUpdated;
  let extraTabs = [];
  const downloadEvents = [];
  let tab = { id: 7, url: "https://page.example.test/watch", title: "Fixture" };
  const event = () => ({ addListener() {} });
  const storage = {
    get: async (key) => key === null ? { ...state } : Array.isArray(key) ? Object.fromEntries(key.map((k) => [k, state[k]])) : typeof key === "object" ? { ...key, ...state } : { [key]: state[key] },
    set: async (value) => Object.assign(state, structuredClone(value)),
    remove: async (keys) => { for (const key of [].concat(keys)) delete state[key]; },
  };
  const chrome = {
    storage: { sync: storage, local: storage, session: storage },
    downloads: {
      onCreated: { addListener: (fn) => { downloadCreated = fn; } },
      pause: async () => downloadEvents.push("pause"),
      cancel: async () => downloadEvents.push("cancel"),
      erase: async () => downloadEvents.push("erase"),
      resume: async () => downloadEvents.push("resume"),
    },
    webRequest: {
      onResponseStarted: { addListener: (fn) => { response = fn; } },
      onBeforeSendHeaders: { addListener: (fn) => { requestHeaders = fn; } },
    },
    tabs: { onUpdated: { addListener: (fn) => { tabUpdated = fn; } }, onRemoved: event(), get: async (id) => [tab, ...extraTabs].find((t) => t.id === id), query: async () => [tab, ...extraTabs] },
    action: { setBadgeText: (value) => { badge = value; }, setBadgeBackgroundColor() {} },
    contextMenus: { onClicked: { addListener: (fn) => { contextMenu = fn; } }, removeAll() {}, create() {} },
    cookies: { getAll: async ({ url }) => {
      cookieLookups.push(url);
      return [{ name: "session", value: "fixture-only", domain: new URL(url).hostname, path: "/", secure: true, hostOnly: true }];
    } },
    runtime: {
      onInstalled: event(), onStartup: event(),
      onMessage: { addListener: (fn) => { message = fn; } },
      sendNativeMessage: async (_, payload) => { messages.push(payload); return { ok: true }; },
    },
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../extension/background.js"), "utf8"), {
    chrome, console, URL, navigator: { userAgent: "fixture-browser" }, setTimeout() {},
  });
  const signed = "https://cdn.example.test/manifest?sig=a%2Fb%26c&rn=keep&range=keep&not=entity";
  const detected = (url, mime) => response({ tabId: 7, url, statusCode: 200, responseHeaders: [{ name: "Content-Type", value: mime }] });
  await Promise.all([
    detected(signed, "application/vnd.apple.mpegurl"),
    detected("https://other-cdn.example.test/video/segment.m4s", "video/mp4"),
    detected("https://cdn.example.test/other.m3u8?sig=other", "application/x-mpegurl"),
  ]);
  assert.equal(state["media:7"].filter((m) => m.kind === "hls").length, 2, "concurrent detections must not overwrite each other");
  assert(state["media:7"].some((m) => m.url === signed));
  assert.equal(badge.text, "2");
  const reply = await new Promise((resolve) => message({ type: "grab", url: signed, streamKind: "hls", filename: "fixture" }, { tab: { id: 7, url: "https://page.example.test/watch" }, url: "https://player.example.test/embed" }, resolve));
  assert.equal(reply.ok, true);
  const payload = messages.at(-1);
  assert.equal(payload.url, signed);
  assert.equal(payload.streamKind, "hls");
  assert.equal(payload.referrer, "https://player.example.test/embed", "use iframe context, not always the top-level page");
  assert.equal(payload.userAgent, "fixture-browser");
  assert.equal(payload.cookies, "", "HLS cookies must not be sent as a global raw header");
  assert(payload.mediaCookies.some((c) => c.domain === "other-cdn.example.test" && c.hostOnly));
  assert(cookieLookups.every((url) => url.startsWith("https://")));
  await new Promise((resolve) => message({ type: "frame-context" }, { tab: { id: 7 }, frameId: 3, url: "https://iframe.example.test/player" }, resolve));
  await response({ tabId: 7, frameId: 3, url: "https://cdn.example.test/frame.m3u8", statusCode: 200, responseHeaders: [] });
  await new Promise((resolve) => message({ type: "grab", url: "https://cdn.example.test/frame.m3u8", pageUrl: "https://top.example.test/watch" }, { tab: { id: 7 } }, resolve));
  assert.equal(messages.at(-1).referrer, "https://iframe.example.test/player", "popup downloads must recover the detected stream's iframe context");
  const page = "https://hqporner.com/hdporn/128499-you_understand_now.html";
  const player = "https://mydaddy.cc/video/fixture/";
  const file = "https://s41.bigcdn.cc/pubs/fixture/video.mp4?token=keep%2Fexact";
  tab = { id: 7, url: page, title: "You Understand Now? - HQporner.com" };
  const grab = (url, filename) => new Promise((resolve) => message({ type: "grab", url, filename }, { tab, url: player }, resolve));
  assert.equal((await grab(file, "video.mp4")).ok, true);
  assert.equal(messages.at(-1).filename, "You Understand Now.mp4", "iframe downloads use the parent title");
  assert.equal(messages.at(-1).url, file, "keep signed CDN URLs intact");
  assert.equal(messages.at(-1).referrer, player, "keep the CDN's player referrer");
   await contextMenu({ srcUrl: file, pageUrl: page, frameUrl: player }, tab);
   assert.equal(messages.at(-1).filename, "You Understand Now.mp4", "right-click downloads also use the parent title");
   assert.equal(messages.at(-1).referrer, player, "right-click must prefer frameUrl to pageUrl");
   await new Promise((resolve) => message({ type: "frame-context" }, { tab, frameId: 4, url: player }, resolve));
   const automatic = { id: 1, url: file, referrer: player, filename: "video.mp4", state: "in_progress" };
   await downloadCreated(automatic);
   assert.equal(messages.at(-1).filename, "You Understand Now.mp4", "Chromium DownloadItem without tabId must recover its owning tab");
   assert.equal(downloadEvents.at(-1), "erase");
   extraTabs = [{ id: 8, url: page, title: "Other tab" }];
   state["media-frames:8"] = { 4: player };
   await downloadCreated(automatic);
   assert.equal(messages.at(-1).filename, "video.mp4", "ambiguous tabs must not supply an arbitrary title");
   state.disabledSites = ["hqporner.com"];
   let beforeAmbiguous = messages.length;
   await downloadCreated(automatic);
   assert.equal(messages.length, beforeAmbiguous, "ambiguous tab matching must not bypass a disabled parent site");
   assert.equal(downloadEvents.at(-1), "resume");
   state.disabledSites = [];
   extraTabs = [];
   state.enabled = false;
   let count = messages.length;
   assert.equal((await grab(file, "video.mp4")).ok, false);
   await contextMenu({ srcUrl: file, pageUrl: page, frameUrl: player }, tab);
   await downloadCreated(automatic);
   assert.equal(messages.length, count, "global disable must block every download entry point");
   state.enabled = true;
   state.disabledSites = ["hqporner.com"];
   count = messages.length;
   assert.equal((await grab(file, "video.mp4")).ok, false);
   await downloadCreated(automatic);
   assert.equal(messages.length, count, "disabled parent sites must also block iframe downloads");
   assert.equal(downloadEvents.at(-1), "resume", "a rejected automatic handoff must resume the browser download");
   state.disabledSites = [];
   await downloadCreated({ id: 2, url: signed, filename: "playlist", state: "in_progress", referrer: "" });
   assert.equal(messages.at(-1).streamKind, "hls", "automatic interception must keep detected extensionless HLS classification");
   assert.equal(messages.at(-1).cookies, "", "automatic HLS downloads must also use scoped cookies");
   const signedMp4 = "https://cdn.example.test/movie.mp4?sig=keep%2Fexact&rn=123&expires=456";
   await detected(signedMp4, "video/mp4");
   assert(state["media:7"].some((m) => m.url === signedMp4), "MP4 query parameters must also remain intact");
  await grab("https://ads.example.test/ad.mp4", "ad.mp4");
  assert.equal(messages.at(-1).filename, "ad.mp4", "do not rename unrelated files");
  tab = { id: 7, url: "https://nothqporner.com/hdporn/128499-video.html", title: "Unrelated" };
  await grab(file, "original.mp4");
  assert.equal(messages.at(-1).filename, "original.mp4", "do not match lookalike sites");
  tab = { id: 7, url: page, title: "" };
  await grab(file, "original.mp4");
  assert.equal(messages.at(-1).filename, "original.mp4", "keep existing names when no title is available");
  const ui = fs.readFileSync(path.join(__dirname, "../app/ui/download.js"), "utf8");
  const uiContext = {};
  vm.runInNewContext(`${ui.match(/function categoryDir\([\s\S]*?\n\}/)[0]}\nglobalThis.dirFor = categoryDir;`, uiContext);
  assert.equal(uiContext.dirFor({ downloadDir: "/home/user/Downloads", sortIntoCategories: true }, "Video"), "/home/user/Downloads/Video");
  assert.equal(uiContext.dirFor({ downloadDir: "C:\\Downloads", sortIntoCategories: true }, "Video"), "C:\\Downloads\\Video");
  assert.equal(uiContext.dirFor({ downloadDir: "/", sortIntoCategories: true }, "Video"), "/Video");
  assert.equal(decodeURIComponent(encodeURIComponent(signed)), signed, "overlay attribute encoding must preserve signed URLs");
  const protectedUrl = "https://vault-99.owocdn.top/stream/fixture/index.m3u8?token=exact%2Fvalue";
  await requestHeaders({ tabId: 7, requestId: "protected", url: protectedUrl, requestHeaders: [
    { name: "Referer", value: "https://kwik.cx/" },
    { name: "Origin", value: "https://kwik.cx" },
    { name: "User-Agent", value: "observed-browser-agent" },
    { name: "Accept", value: "*/*" },
    { name: "Accept-Language", value: "en-US,en;q=0.9" },
    { name: "Cookie", value: "must-not-be-copied" },
    { name: "Authorization", value: "must-not-be-copied" },
  ] });
  await response({ tabId: 7, requestId: "protected", frameId: 3, documentUrl: "https://kwik.cx/e/fixture", url: protectedUrl, statusCode: 200, responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }] });
  assert.equal((await grab(protectedUrl, "episode")).ok, true);
  assert.equal(messages.at(-1).referrer, "https://kwik.cx/", "use observed wire Referer, not the frame URL");
  assert.equal(messages.at(-1).userAgent, "observed-browser-agent");
  assert(messages.at(-1).mediaHeaders.some(([key, value]) => key === "Origin" && value === "https://kwik.cx"));
  assert(messages.at(-1).mediaHeaders.some(([key, value]) => key === "Accept-Language" && value === "en-US,en;q=0.9"));
  assert(!JSON.stringify(messages.at(-1)).includes("must-not-be-copied"), "never copy raw credentials from observed request headers");
  const rotated = protectedUrl.replace("token=exact%2Fvalue", "token=new%2Fsignature");
  await detected(rotated, "application/vnd.apple.mpegurl");
  assert.equal(state["media:7"].filter((m) => m.url.startsWith("https://vault-99.owocdn.top/")).length, 1, "rotating tokens should replace the same playlist entry");
  assert(state["media:7"].some((m) => m.url === rotated), "keep the complete newest signed URL");
  await detected(`${rotated}&quality=720`, "application/vnd.apple.mpegurl");
  assert.equal(state["media:7"].filter((m) => m.url.startsWith("https://vault-99.owocdn.top/")).length, 2, "do not merge distinct media/quality selectors");
  await requestHeaders({ tabId: 7, requestId: "no-referrer", url: rotated, requestHeaders: [] });
  await response({ tabId: 7, requestId: "no-referrer", url: rotated, frameId: 3, statusCode: 200, responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }] });
  await grab(rotated, "episode");
  assert.equal(messages.at(-1).referrer, "", "respect browser no-referrer policy rather than inventing a Referer");
  await tabUpdated(7, { status: "loading" });
  // Tab listeners schedule storage changes; a message joins the per-tab queue.
  await new Promise((resolve) => message({ type: "frame-context" }, { tab, frameId: 0, url: tab.url }, resolve));
  assert.equal(state["media:7"], undefined, "same-URL reloads must clear stale media");
  assert.equal(state["media-requests:7"], undefined, "reloads must clear captured request headers");
  let firefoxFallback = 0;
  chrome.webRequest.onBeforeSendHeaders.addListener = (_, __, options) => {
    if (options.includes("extraHeaders")) throw new Error("Unsupported Firefox option");
    firefoxFallback++;
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../extension/background.js"), "utf8"), {
    chrome, console, URL, navigator: { userAgent: "fixture-browser" }, setTimeout() {},
  });
  assert.equal(firefoxFallback, 1, "Firefox must register the header listener without Chromium-only options");
  console.log("HLS extension detection, signed URL, badge and scoped-cookie handoff checks passed");
  console.log("HQPorner embedded-player filename and direct-download handoff checks passed");
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
