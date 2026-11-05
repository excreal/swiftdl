// SwiftDL UI. Talks to the Rust backend through Tauri's invoke/listen;
// opened in a normal browser, mock.js stands in for the backend.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const appWindow = window.__TAURI__.window?.getCurrentWindow?.();

// ---------- icons (1.6px strokes) ----------
const P = {
  search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>',
  compose: '<path d="M12 4H6.5A2.5 2.5 0 0 0 4 6.5v11A2.5 2.5 0 0 0 6.5 20h11a2.5 2.5 0 0 0 2.5-2.5V12"/><path d="M17.6 3.6a2 2 0 0 1 2.8 2.8L12.5 14.3 9 15l.7-3.5z"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  download: '<path d="M12 4v11M7.5 10.5 12 15l4.5-4.5M5 20h14"/>',
  pause: '<path d="M9 6v12M15 6v12"/>',
  play: '<path d="M8 5.5v13l10.5-6.5z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  globe: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5a13 13 0 0 1 0 17M12 3.5a13 13 0 0 0 0 17"/>',
  chevron: '<path d="m9.5 6 6 6-6 6"/>',
  chevDown: '<path d="m6 9.5 6 6 6-6"/>',
  x: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>',
  folder: '<path d="M3.5 7.5A2 2 0 0 1 5.5 5.5H9l2 2h7.5a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  open: '<path d="M14 4.5h5.5V10M19.5 4.5 11 13M18 14v4a1.5 1.5 0 0 1-1.5 1.5h-10A1.5 1.5 0 0 1 5 18V8a1.5 1.5 0 0 1 1.5-1.5h4"/>',
  copy: '<rect x="8.5" y="8.5" width="11" height="11" rx="2"/><path d="M15.5 8.5V6a1.5 1.5 0 0 0-1.5-1.5H6A1.5 1.5 0 0 0 4.5 6v8A1.5 1.5 0 0 0 6 15.5h2.5"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
  retry: '<path d="M4.5 12a7.5 7.5 0 0 1 13.2-4.8L19.5 9M19.5 4.5V9H15M19.5 12a7.5 7.5 0 0 1-13.2 4.8L4.5 15M4.5 19.5V15H9"/>',
  trash: '<path d="M4.5 7h15M10 11v6M14 11v6M6.5 7l.8 11.2A2 2 0 0 0 9.3 20h5.4a2 2 0 0 0 2-1.8L17.5 7M9.5 7V5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v2"/>',
  more: '<circle cx="6" cy="12" r="1.1"/><circle cx="12" cy="12" r="1.1"/><circle cx="18" cy="12" r="1.1"/>',
  gauge: '<path d="m12 14 3.5-3.5"/><path d="M4 18a8.5 8.5 0 1 1 16 0"/>',
  sliders: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>',
  clipboard: '<rect x="6" y="4.5" width="12" height="16" rx="2"/><path d="M9.5 4.5h5v2.5h-5z"/>',
  check: '<path d="m5.5 12.5 4 4 9-9"/>',
  inbox: '<path d="M4 13.5 6.5 6h11l2.5 7.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18z"/><path d="M4 13.5h4.5l1 2h5l1-2H20"/>',
  arrowDown: '<path d="M12 5v14M6.5 13.5 12 19l5.5-5.5"/>',
  doneCircle: '<circle cx="12" cy="12" r="8.5"/><path d="m8.5 12 2.5 2.5 4.5-5"/>',
  alert: '<circle cx="12" cy="12" r="8.5"/><path d="M12 8v5M12 16h.01"/>',
  bolt: '<path d="M13 3.5 5.5 13.5H11l-1 7 7.5-10H12z"/>',
  hash: '<path d="M5 9h14M5 15h14M10 4 8 20M16 4l-2 16"/>',
  keyboard: '<rect x="3" y="6" width="18" height="12" rx="2"/><path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10"/>',
  video: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M7.5 4.5v15M16.5 4.5v15M3.5 9.5h4M3.5 14.5h4M16.5 9.5h4M16.5 14.5h4"/>',
  music: '<path d="M9 17.5V6.3l10-1.8v11"/><circle cx="6.5" cy="17.5" r="2.5"/><circle cx="16.5" cy="15.5" r="2.5"/>',
  app: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M3.5 9h17"/><path d="M6.6 6.8h.01M9 6.8h.01" stroke-width="2.2"/>',
  archive: '<path d="M12 3.3 20 7.4v9.2l-8 4.1-8-4.1V7.4z"/><path d="m4 7.4 8 4.1 8-4.1M12 11.5v9.2M8 5.3l8 4.1"/>',
  doc: '<path d="M14 3.5H7.5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8z"/><path d="M14 3.5V8h4.5M9 12.5h6M9 16h6"/>',
  image: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><circle cx="9" cy="10" r="1.7"/><path d="m20.5 15.5-4.5-4.5-9.5 8.5"/>',
  file: '<path d="M14 3.5H7.5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8z"/><path d="M14 3.5V8h4.5"/>',
};
const icon = (n) => `<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${P[n] || P.file}</svg>`;
const hydrate = () => document.querySelectorAll("i[data-icon]").forEach((el) => {
  const tpl = document.createElement("template");
  tpl.innerHTML = icon(el.dataset.icon);
  const svg = tpl.content.firstElementChild;
  if (el.className) svg.classList.add(...el.className.split(" "));
  el.replaceWith(svg);
});

const CATS = {
  Video: { color: "#8b5cf6", icon: "video" },
  Music: { color: "#ec4899", icon: "music" },
  Programs: { color: "#3b82f6", icon: "app" },
  Compressed: { color: "#f59e0b", icon: "archive" },
  Documents: { color: "#10b981", icon: "doc" },
  Images: { color: "#f97316", icon: "image" },
  Other: { color: "#8b8f98", icon: "file" },
};
const cat = (it) => CATS[it.category] || CATS.Other;
const typeIcon = (name) => `<span class="ni-type" style="--c:${(CATS[name] || CATS.Other).color}">${icon((CATS[name] || CATS.Other).icon)}</span>`;
const ftile = (it, lg = false) => `<div class="ftile ${lg ? "lg" : ""}" style="--c:${cat(it).color}">${icon(cat(it).icon)}</div>`;

// Status glyph: a progress ring that fills clockwise while downloading
// (pause bars inside when paused), a dotted ring when queued, and filled
// badges for done and failed.
function statusIcon(it) {
  const s = it.status;
  if (s === "completed") return `<svg class="status" viewBox="0 0 16 16"><circle cx="8" cy="8" r="7.25" fill="var(--green)"/><path d="m5.1 8.2 2 2 3.8-4" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
  if (s === "error") return `<svg class="status" viewBox="0 0 16 16"><circle cx="8" cy="8" r="7.25" fill="var(--red)"/><path d="M8 4.6v4.2" stroke="#fff" stroke-width="1.7" stroke-linecap="round"/><circle cx="8" cy="11.2" r="1" fill="#fff"/></svg>`;
  if (s === "queued") return `<svg class="status" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6.4" fill="none" stroke="var(--faint)" stroke-width="1.7" stroke-linecap="round" stroke-dasharray="0.1 3.25"/></svg>`;
  const color = s === "paused" ? "var(--amber)" : "var(--accent)";
  const r = 6.4, c = 2 * Math.PI * r, p = Math.max(0.03, Math.min(1, pct(it) / 100));
  const inner = s === "paused"
    ? `<path d="M6.6 6v4M9.4 6v4" stroke="${color}" stroke-width="1.5" stroke-linecap="round"/>`
    : `<path d="M8 5.4v4.6M6.1 8.3 8 10.2l1.9-1.9" fill="none" stroke="${color}" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>`;
  return `<svg class="status" viewBox="0 0 16 16"><circle cx="8" cy="8" r="${r}" fill="none" stroke="var(--track)" stroke-width="1.7"/><circle cx="8" cy="8" r="${r}" fill="none" stroke="${color}" stroke-width="1.7" stroke-linecap="round" stroke-dasharray="${(c * p).toFixed(2)} ${c.toFixed(2)}" transform="rotate(-90 8 8)"/>${inner}</svg>`;
}

// ---------- formatting ----------
const UNITS = ["B", "KB", "MB", "GB", "TB"];
function parts(n) {
  if (n == null || !isFinite(n)) return ["—", ""];
  let i = 0;
  while (n >= 1024 && i < UNITS.length - 1) { n /= 1024; i++; }
  return [i === 0 ? String(Math.round(n)) : n < 10 ? n.toFixed(2) : n < 100 ? n.toFixed(1) : String(Math.round(n)), UNITS[i]];
}
const bytes = (n) => (n == null ? "—" : parts(n).join(" "));
const speed = (b) => (b > 0 ? `${bytes(b)}/s` : "—");
function dur(sec) {
  if (!isFinite(sec) || sec <= 0) return "—";
  sec = Math.ceil(sec);
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return h ? `${h}h ${m}m` : m ? `${m}m ${s}s` : `${s}s`;
}
function when(ms) {
  if (!ms) return "";
  const d = new Date(ms), now = new Date();
  const mins = Math.round((now - d) / 60000);
  if (mins < 1) return "Just now";
  if (mins < 60) return `${mins}m ago`;
  if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const y = new Date(now); y.setDate(now.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return "Yesterday";
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const $ = (id) => document.getElementById(id);
const pct = (it) => (it.size ? Math.min(100, (it.downloaded / it.size) * 100) : it.status === "completed" ? 100 : 0);
const eta = (it) => (it.speed > 0 && it.size ? (it.size - it.downloaded) / it.speed : Infinity);
const isActive = (it) => it.status === "downloading" || it.status === "queued";
const host = (url) => { try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return ""; } };
const ext = (name) => (/\.([a-z0-9]{1,5})$/i.exec(name || "") || [])[1]?.toUpperCase() || "";
const STATUS = { downloading: "Downloading", queued: "Queued", paused: "Paused", completed: "Completed", error: "Failed" };

// ---------- state ----------
const HISTORY = 120;
const LIMITS = [0, 256, 1024, 5120, 10240];
const state = {
  items: [], settings: null, view: "all", search: "", selected: null, pending: [],
  speedHist: new Map(), totalHist: [], typesOpen: true,
};
const VIEWS = {
  all: { name: "All downloads", icon: "inbox", test: () => true },
  active: { name: "In progress", icon: "arrowDown", test: (i) => i.status !== "completed" && i.status !== "error" },
  completed: { name: "Completed", icon: "doneCircle", test: (i) => i.status === "completed" },
  failed: { name: "Failed", icon: "alert", test: (i) => i.status === "error" },
};
const byId = (id) => state.items.find((i) => i.id === id);
function visible() {
  const v = VIEWS[state.view];
  const q = state.search.trim().toLowerCase();
  return state.items.filter((it) => (v ? v.test(it) : it.category === state.view) &&
    (!q || it.filename.toLowerCase().includes(q) || it.url.toLowerCase().includes(q)));
}
const GROUPS = [
  ["downloading", "Downloading"], ["queued", "Queued"], ["paused", "Paused"], ["error", "Failed"], ["completed", "Completed"],
];
function grouped(list) {
  return GROUPS.map(([s, title]) => {
    const items = list.filter((i) => i.status === s);
    items.sort(s === "completed" ? (a, b) => (b.completedAt || 0) - (a.completedAt || 0) : (a, b) => b.addedAt - a.addedAt);
    return { s, title, items };
  }).filter((g) => g.items.length);
}
const orderIds = () => grouped(visible()).flatMap((g) => g.items.map((i) => i.id));

// ---------- charts ----------
function areaChart(canvas, data, { grid = false } = {}) {
  const dpr = devicePixelRatio || 1, w = canvas.clientWidth, h = canvas.clientHeight;
  if (!w || !h) return;
  if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  const css = getComputedStyle(document.documentElement);
  const accent = css.getPropertyValue("--accent").trim();
  const line = css.getPropertyValue("--border").trim();
  const faint = css.getPropertyValue("--faint").trim();
  const max = Math.max(1, ...data) * 1.2;
  // With a scale, labels get their own margin on the right so the graph
  // never draws over them.
  const gutter = grid ? 58 : 0;
  const pw = w - gutter, top = grid ? 6 : 2, bottom = grid ? 6 : 2;
  const yOf = (v) => h - bottom - (v / max) * (h - top - bottom);
  if (grid) {
    ctx.lineWidth = 1;
    ctx.font = `500 10.5px ${css.getPropertyValue("--font")}`;
    ctx.textBaseline = "middle";
    const hasData = data.some((v) => v > 0);
    for (const frac of [1 / 1.2, 0.5 / 1.2, 0]) {
      const y = Math.round(yOf(max * frac)) + 0.5;
      ctx.strokeStyle = line;
      ctx.setLineDash(frac ? [3, 3] : []);
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(pw, y); ctx.stroke();
      ctx.fillStyle = faint;
      if (hasData || !frac) ctx.fillText(frac ? speed(max * frac) : "0", pw + 8, y);
    }
    ctx.setLineDash([]);
  }
  const n = HISTORY, span = Math.max(Math.min(data.length, n), 30) - 1;
  const pts = data.slice(-n).map((v, i, a) => [pw - (a.length - 1 - i) * (pw / span), yOf(v)]);
  if (pts.length < 2) return;
  const path = new Path2D();
  pts.forEach(([x, y], i) => {
    if (!i) path.moveTo(x, y);
    else { const [px, py] = pts[i - 1], cx = (px + x) / 2; path.bezierCurveTo(cx, py, cx, y, x, y); }
  });
  const area = new Path2D(path);
  area.lineTo(pts.at(-1)[0], h - bottom); area.lineTo(pts[0][0], h - bottom); area.closePath();
  const g = ctx.createLinearGradient(0, 0, 0, h);
  g.addColorStop(0, accent + "40"); g.addColorStop(1, accent + "00");
  ctx.fillStyle = g; ctx.fill(area);
  ctx.strokeStyle = accent; ctx.lineWidth = 1.5; ctx.stroke(path);
}

// ---------- sidebar ----------
function renderNav() {
  const count = (fn) => state.items.filter(fn).length;
  const live = state.items.some((i) => i.status === "downloading");
  const item = (view, label, ic, n, extra = "") =>
    `<button class="nav-item ${state.view === view ? "on" : ""}" data-view="${view}"><span class="ni-icon">${ic}</span><span class="ni-label">${label}</span>${extra}<span class="ni-count">${n || ""}</span></button>`;
  let html = item("all", "All downloads", icon("inbox"), state.items.length) +
    item("active", "In progress", icon("arrowDown"), count(VIEWS.active.test), live ? '<span class="ni-live"></span>' : "") +
    item("completed", "Completed", icon("doneCircle"), count(VIEWS.completed.test));
  const failed = count(VIEWS.failed.test);
  if (failed || state.view === "failed") html += item("failed", "Failed", icon("alert"), failed);
  html += `<button class="nav-sec ${state.typesOpen ? "" : "closed"}" data-toggle="types">Types ${icon("chevDown")}</button>`;
  if (state.typesOpen) for (const name of Object.keys(CATS)) html += item(name, name, typeIcon(name), count((i) => i.category === name));
  if ($("nav").innerHTML !== html) $("nav").innerHTML = html;

  const active = state.items.filter((i) => i.status === "downloading");
  const queued = state.items.filter((i) => i.status === "queued").length;
  const total = active.reduce((s, i) => s + (i.speed || 0), 0);
  const [v, u] = total > 0 ? parts(total) : ["0", "B"];
  $("net-speed").textContent = v;
  $("net-unit").textContent = `${u}/s`;
  const left = active.reduce((s, i) => s + Math.max(0, (i.size || 0) - i.downloaded), 0);
  $("net-foot").textContent = active.length ? `${active.length} active${queued ? `, ${queued} queued` : ""} · ${bytes(left)} left` : queued ? `${queued} queued` : "Idle";
  const lim = state.settings?.speedLimitKib || 0;
  $("limit-btn").textContent = lim ? `≤ ${bytes(lim * 1024)}/s` : "No limit";
  $("limit-btn").classList.toggle("on", !!lim);
  areaChart($("net-graph"), state.totalHist);
}

// ---------- header ----------
function renderHeader(list) {
  const v = VIEWS[state.view];
  $("h-icon").innerHTML = v ? icon(v.icon) : typeIcon(state.view);
  $("h-name").textContent = v ? v.name : state.view;
  $("h-count").textContent = list.length || "";
  $("btn-pause-all").hidden = !list.some(isActive);
  $("btn-resume-all").hidden = !list.some((i) => i.status === "paused" || i.status === "error");
}

// ---------- list ----------
const rowEls = new Map(), headEls = new Map();
function makeRow(id) {
  const el = document.createElement("div");
  el.className = "row"; el.dataset.id = id;
  el.innerHTML = `<span class="r-status"></span><div class="r-main"></div><div class="r-meta"></div>`;
  el._c = [...el.children]; el._k = [];
  return el;
}
function set(el, i, html) { if (el._k[i] !== html) { el._k[i] = html; el._c[i].innerHTML = html; } }
function meta(it) {
  const p = Math.floor(pct(it));
  const mini = (cls) => `<span class="mini ${cls}"><i style="width:${pct(it)}%"></i></span><span class="pct">${it.size ? p + "%" : ""}</span>`;
  switch (it.status) {
    case "downloading":
      return `${mini("")}<span class="m-size">${bytes(it.downloaded)} / ${bytes(it.size)}</span><span class="m-speed">${it.speed > 0 ? speed(it.speed) : "Connecting…"}</span><span class="m-eta">${dur(eta(it))}</span>`;
    case "queued": return `${mini("queued")}<span class="m-size">${it.size ? bytes(it.size) : "—"}</span><span class="m-speed" style="color:var(--muted);font-weight:500">Queued</span><span class="m-eta"></span>`;
    case "paused": return `${mini("paused")}<span class="m-size">${bytes(it.downloaded)} / ${bytes(it.size)}</span><span class="m-speed" style="color:var(--amber);font-weight:500">Paused</span><span class="m-eta"></span>`;
    case "error": return `<span class="m-size">${bytes(it.size)}</span><span class="m-when">${when(it.addedAt)}</span>`;
    default: return `<span class="cat"><i style="background:${cat(it).color}"></i>${esc(it.category)}</span><span class="m-size">${bytes(it.size)}</span><span class="m-when">${when(it.completedAt)}</span>`;
  }
}
function updateRow(el, it) {
  el.classList.toggle("sel", state.selected === it.id);
  set(el, 0, statusIcon(it));
  const e = ext(it.filename);
  set(el, 1, `<span class="r-name" title="${esc(it.filename)}">${esc(it.filename.replace(/\.[^.]+$/, "") || it.filename)}</span>${e ? `<span class="r-ext">${e}</span>` : ""}${it.status === "error" ? `<span class="r-err">${esc(it.error || "Failed")}</span>` : ""}`);
  set(el, 2, meta(it));
}
function renderList() {
  const list = visible();
  const nodes = [];
  for (const g of grouped(list)) {
    let h = headEls.get(g.s);
    if (!h) { h = document.createElement("div"); h.className = "ghead"; headEls.set(g.s, h); }
    const right = g.s === "downloading" ? speed(g.items.reduce((s, i) => s + (i.speed || 0), 0)) : g.s === "completed" ? bytes(g.items.reduce((s, i) => s + (i.size || 0), 0)) : "";
    const html = `${statusIcon({ status: g.s, size: 4, downloaded: 3 })}<span>${g.title}</span><span class="gc">${g.items.length}</span><span class="gr">${right}</span>`;
    if (h.innerHTML !== html) h.innerHTML = html;
    nodes.push(h);
    for (const it of g.items) {
      let el = rowEls.get(it.id);
      if (!el) { el = makeRow(it.id); rowEls.set(it.id, el); }
      updateRow(el, it);
      nodes.push(el);
    }
  }
  for (const id of rowEls.keys()) if (!byId(id)) rowEls.delete(id);
  const root = $("rows");
  if (root.children.length !== nodes.length || nodes.some((n, i) => root.children[i] !== n)) root.replaceChildren(...nodes);

  $("empty").hidden = list.length > 0;
  if (!list.length) {
    const q = state.search.trim();
    $("empty-title").textContent = q ? "No results" : state.items.length ? "Nothing here" : "No downloads yet";
    $("empty-text").textContent = q ? `No downloads match “${q}”.` : state.items.length ? "Downloads in this view will show up here." : "Paste a link, drop one onto this window, or start a download in your browser.";
    $("empty-actions").hidden = !!q || state.items.length > 0;
  }
  return list;
}

// ---------- connection slots ----------
// Same view as the download window: each connection's part of the file is
// a rounded slot, filled as far as that connection has got. When nothing is
// running (no live segments) it falls back to a single bar.
function renderLanes(el, it) {
  const size = it.size || 1;
  const segs = (it.segments || []).filter(([s, , e]) => e > s);
  const paused = it.status !== "downloading";
  if (!segs.length) {
    const html = `<div class="lanes-empty ${paused ? "paused" : ""}"><i style="width:${pct(it)}%"></i></div>`;
    if (el.innerHTML !== html) el.innerHTML = html;
    return 0;
  }
  if (el.firstElementChild?.className !== `lanes ${paused ? "paused" : ""}` || el.firstElementChild.children.length !== segs.length) {
    el.innerHTML = `<div class="lanes ${paused ? "paused" : ""}">${segs.map(() => `<span class="slot"><i></i></span>`).join("")}</div>`;
  }
  segs.forEach(([s, p, e], i) => {
    const slot = el.firstElementChild.children[i];
    slot.style.left = `calc(${(s / size) * 100}% + 1px)`;
    slot.style.width = `calc(${((e - s) / size) * 100}% - 2px)`;
    slot.firstElementChild.style.width = `${((p - s) / Math.max(1, e - s)) * 100}%`;
  });
  return segs.filter(([, p, e]) => p < e).length;
}

// ---------- inspector ----------
let insKey = null;
function renderInspector() {
  const panel = $("inspector");
  const it = state.selected != null ? byId(state.selected) : null;
  if (!it) { panel.hidden = true; insKey = null; return; }
  panel.hidden = false;
  const key = `${it.id}:${it.status}`;
  const dl = it.status === "downloading";
  if (insKey !== key) {
    insKey = key;
    const hasMap = it.status !== "completed" && it.size;
    panel.innerHTML = `<div class="ins-scroll">
        <div class="ins-top"><span class="ins-glyph">${icon(cat(it).icon)}</span><div class="ins-head"><div class="ins-title">${esc(it.filename)}</div><div class="ins-meta" id="ins-chips"></div></div><button class="sq-btn" data-ins="close" title="Close (Esc)">${icon("x")}</button></div>
        <div class="ins-sec"><div class="ins-sec-head">Progress<span id="ins-pct"></span></div><div class="ins-bar" id="ins-bar"><i></i></div><dl class="props" id="ins-stats"></dl></div>
        ${hasMap ? `<div class="ins-sec"><div class="ins-sec-head">Connections<span id="ins-conn"></span></div><div id="ins-map"></div></div>` : ""}
        <div class="ins-sec" id="ins-graph-sec"><div class="ins-sec-head">Speed<span id="ins-peak"></span></div><canvas id="ins-graph"></canvas></div>
        <div class="ins-sec"><div class="ins-sec-head">Details</div><dl class="props" id="ins-props"></dl></div>
      </div>
      <div class="ins-foot" id="ins-foot"></div>`;
  }
  const dotColor = { downloading: "var(--accent)", queued: "var(--faint)", paused: "var(--amber)", completed: "var(--green)", error: "var(--red)" }[it.status];
  const meta = `<span class="dot" style="background:${dotColor}"></span><span>${STATUS[it.status]}</span><span>·</span><span>${esc(it.category)}</span><span>·</span><span class="trunc">${esc(host(it.url))}</span>`;
  if ($("ins-chips").innerHTML !== meta) $("ins-chips").innerHTML = meta;

  $("ins-pct").textContent = it.size ? `${Math.floor(pct(it))}%` : "";
  const bar = $("ins-bar");
  bar.className = `ins-bar ${it.status}`;
  bar.firstElementChild.style.width = `${pct(it)}%`;
  const row = (k, v) => `<dt>${k}</dt><dd><span>${v}</span></dd>`;
  const stats = it.status === "completed"
    ? row("Size", bytes(it.size)) + row("Finished", when(it.completedAt) || "—")
    : row("Downloaded", it.size ? `${bytes(it.downloaded)} of ${bytes(it.size)}` : bytes(it.downloaded)) +
      (dl ? row("Speed", speed(it.speed)) + row("Time left", dur(eta(it))) : "");
  if ($("ins-stats").innerHTML !== stats) $("ins-stats").innerHTML = stats;

  if ($("ins-map")) {
    const active = renderLanes($("ins-map"), it);
    $("ins-conn").textContent = dl ? `${active || it.connections} active` : it.status === "paused" ? "Paused" : "";
  }
  const hist = state.speedHist.get(it.id) || [];
  $("ins-graph-sec").hidden = !hist.some((v) => v > 0);
  if (!$("ins-graph-sec").hidden) { areaChart($("ins-graph"), hist, { grid: true }); $("ins-peak").textContent = `Peak ${speed(Math.max(...hist))}`; }

  const dir = it.path.replace(/[\\/][^\\/]*$/, "");
  const props = `
    <dt>Location</dt><dd><span class="link" data-ins="folder" title="${esc(it.path)}">${esc(dir)}</span><button class="copy" data-ins="copy-path" title="Copy path">${icon("copy")}</button></dd>
    <dt>Source</dt><dd><span title="${esc(it.url)}">${esc(it.url)}</span><button class="copy" data-ins="copy" title="Copy link">${icon("copy")}</button></dd>
    <dt>Resumable</dt><dd><span>${it.format ? "Yes" : it.resumable ? "Yes" : "No, the server doesn't support it"}</span></dd>
    <dt>Added</dt><dd><span>${new Date(it.addedAt).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })}</span></dd>
    ${it.error ? `<dt>Error</dt><dd><span style="color:var(--red)" title="${esc(it.error)}">${esc(it.error)}</span></dd>` : ""}`;
  if ($("ins-props").innerHTML !== props) $("ins-props").innerHTML = props;

  const primary = it.status === "completed" ? `<button class="btn primary" data-ins="open">Open</button>`
    : it.status === "error" ? `<button class="btn primary" data-ins="resume">Retry</button>`
    : isActive(it) ? `<button class="btn" data-ins="pause">Pause</button>`
    : `<button class="btn primary" data-ins="resume">Resume</button>`;
  const foot = `<button class="btn" data-ins="folder">Show in folder</button>${primary}<button class="btn icon" data-ins="more" title="More">${icon("more")}</button>`;
  if ($("ins-foot").innerHTML !== foot) $("ins-foot").innerHTML = foot;
}

// ---------- browser integration ----------
async function loadIntegration() {
  try { state.integration = await invoke("integration_status"); } catch { state.integration = null; }
  renderOnboard();
  if ($("dlg-settings").open) renderBrowserPanel();
}
const installedBrowsers = () => (state.integration?.browsers || []).filter((b) => b.installed);
function renderOnboard() {
  const card = $("onboard");
  const i = state.integration;
  const show = i && !i.lastSeen && !state.settings?.onboardingDismissed && installedBrowsers().length > 0;
  card.hidden = !show;
  if (!show) return;
  // Until the extension is on the Chrome Web Store, store buttons would lead
  // to a missing page, so offer the guided manual install instead.
  const actions = i.storeAvailable
    ? installedBrowsers().slice(0, 3).map((b) => `<button class="link-btn" data-store="${b.id}">Add to ${b.name}</button>`).join("")
    : `<button class="link-btn" data-guide>Add the extension</button>`;
  const html = `<span class="ob-icon">${icon("globe")}</span>
    <span class="ob-text"><b>Connect your browser</b>Catch downloads automatically and get a download button on videos</span>
    <span class="ob-actions">${actions}<button class="sq-btn" data-dismiss title="Dismiss">${icon("x")}</button></span>`;
  if (card.innerHTML !== html) card.innerHTML = html;
}
async function openExtensionGuide() {
  const dir = await invoke("extension_dir").catch(() => null);
  $("ext-path").textContent = dir || "%LOCALAPPDATA%\\SwiftDL\\extension";
  $("ext-path").title = $("ext-path").textContent;
  // The Firefox add-on is installed next to it, in extension-firefox.
  $("ext-path-ff").textContent = $("ext-path").textContent.replace(/extension$/, "extension-firefox");
  $("dlg-ext").showModal();
}
$("ext-copy").innerHTML = icon("copy");
$("ext-copy").onclick = () => copy($("ext-path").textContent, "Folder path copied");
$("ext-open").onclick = () => call("open_extension_folder");
$("onboard").addEventListener("click", async (e) => {
  if (e.target.closest("[data-guide]")) return openExtensionGuide();
  const s = e.target.closest("[data-store]");
  if (s) return openStore(s.dataset.store);
  if (e.target.closest("[data-dismiss]")) {
    await saveSettings({ ...state.settings, onboardingDismissed: true });
    renderOnboard();
  }
});
async function openStore(browser) {
  await call("open_store", { browser });
  toast("Opened the extension page. Click “Add to browser” there.", "ok");
}
function renderBrowserPanel() {
  const i = state.integration;
  if (!i) return;
  const store = i.storeAvailable;
  const hint = store ? "Add it to a browser below, then open that browser once" : "Follow the steps below, then open the browser once";
  const connected = i.browsers.filter((b) => b.lastSeen).sort((a, b) => b.lastSeen - a.lastSeen);
  const logo = (b, cls = "") => `<img class="blogo ${cls}" src="browsers/${b.id}.svg" alt="" width="28" height="28">`;
  $("ext-status").innerHTML = connected.length
    ? `${logo(connected[0])}<div class="sr-text"><b>Connected in ${connected.map((b) => b.name).join(", ").replace(/, ([^,]*)$/, " and $1")}</b><span>Last active ${when(connected[0].lastSeen).toLowerCase()}</span></div>`
    : `<div class="sr-text"><b>Not connected yet</b><span>${hint}</span></div>`;
  const rank = (b) => (b.lastSeen ? 2 : b.installed ? 1 : 0);
  $("browser-list").innerHTML = [...i.browsers].sort((a, b) => rank(b) - rank(a)).map((b) => {
    const sub = b.lastSeen ? `Connected · last active ${when(b.lastSeen).toLowerCase()}`
      : !b.installed ? "Not installed"
      : store && b.autoOffer ? "Installed · asks to enable the extension on next start" : "Installed";
    return `<div class="set-row">
      ${logo(b, b.installed || b.lastSeen ? "" : "off")}
      <div class="sr-text"><b>${b.name}</b><span>${sub}</span></div>
      ${!b.lastSeen && b.installed && store ? `<button type="button" class="btn sm" data-store="${b.id}">Add extension</button>` : ""}</div>`;
  }).join("");
  // No store listing yet: show the manual steps up front, and hide the
  // automatic offer, which only works once the extension is on the store.
  const manual = document.querySelector(".manual");
  manual.open = !store;
  manual.querySelector("summary").textContent = store ? "Install manually" : "How to add it";
  $("set-offer").closest(".set-group").hidden = !store;
}
$("browser-list").addEventListener("click", (e) => { const b = e.target.closest("[data-store]"); if (b) openStore(b.dataset.store); });

function render() {
  if (state.selected != null && !byId(state.selected)) state.selected = null;
  const list = renderList();
  renderNav();
  renderHeader(list);
  renderInspector();
}

// ---------- toasts ----------
function toast(msg, kind = "info") {
  const el = document.createElement("div");
  el.className = "toast";
  const ic = kind === "ok" ? "check" : kind === "err" ? "alert" : kind === "dl" ? "download" : "info";
  el.innerHTML = `<span class="t-ic ${kind}">${icon(ic)}</span><span>${msg}</span>`;
  $("toasts").append(el);
  setTimeout(() => { el.classList.add("out"); setTimeout(() => el.remove(), 200); }, 2800);
  while ($("toasts").children.length > 3) $("toasts").firstElementChild.remove();
}
async function call(cmd, args) {
  try { return await invoke(cmd, args); } catch (e) { toast(esc(String(e)), "err"); throw e; }
}

// ---------- actions ----------
const resume = (ids) => ids.length && call("resume", { ids });
const pause = (ids) => ids.length && call("pause", { ids });
const copy = (text, msg) => navigator.clipboard.writeText(text).then(() => toast(msg, "ok"));
function toggle(it) {
  if (!it || it.status === "completed") return;
  isActive(it) ? pause([it.id]) : resume([it.id]);
}
function askDelete(it) {
  if (!it) return;
  const dlg = $("dlg-delete");
  const live = it.status !== "completed";
  $("del-title").textContent = live ? "Cancel this download?" : "Remove from list?";
  $("del-text").textContent = live ? `“${it.filename}” will stop and its partial data will be deleted.` : `“${it.filename}” will be removed from SwiftDL.`;
  $("del-ok").textContent = live ? "Cancel download" : "Remove";
  $("del-files").checked = false;
  $("del-files-row").hidden = live;
  dlg.returnValue = "";
  dlg.onclose = () => {
    if (dlg.returnValue !== "ok") return;
    call("remove", { ids: [it.id], deleteFiles: $("del-files").checked });
    toast(live ? "Download cancelled" : "Removed from list", "ok");
  };
  dlg.showModal();
}
function act(name, it, anchor) {
  if (!it) return;
  switch (name) {
    case "open": return call("open_file", { id: it.id });
    case "folder": return call("open_folder", { id: it.id });
    case "pause": return pause([it.id]);
    case "resume": return resume([it.id]);
    case "copy": return copy(it.url, "Link copied");
    case "copy-path": return copy(it.path, "Path copied");
    case "delete": return askDelete(it);
    case "more": { const r = anchor.getBoundingClientRect(); return itemMenu(it, r.right - 220, r.top - 8, true); }
  }
}
function select(id, scroll = true) {
  state.selected = id;
  render();
  if (scroll) rowEls.get(id)?.scrollIntoView({ block: "nearest" });
}
function move(d) {
  const ids = orderIds();
  if (!ids.length) return;
  const i = ids.indexOf(state.selected);
  select(ids[i < 0 ? (d > 0 ? 0 : ids.length - 1) : Math.max(0, Math.min(ids.length - 1, i + d))]);
}

// ---------- menus ----------
function showMenu(x, y, entries, above = false) {
  const menu = $("menu");
  menu.innerHTML = entries.map((e, i) => e === "-" ? `<div class="msep"></div>` : e.title ? `<div class="mtitle">${e.title}</div>` :
    `<button data-i="${i}" class="${e.danger ? "danger" : ""}" ${e.enabled === false ? "disabled" : ""}>${e.icon ? icon(e.icon) : e.dot ? `<span class="ni-dot" style="background:${e.dot};margin:0 3px"></span>` : ""}<span>${e.label}</span>${e.checked ? `<span class="mcheck">${icon("check")}</span>` : e.keys ? `<span class="mh">${e.keys.map((k) => `<kbd>${k}</kbd>`).join("")}</span>` : ""}</button>`).join("");
  menu.onclick = (ev) => { const b = ev.target.closest("button[data-i]"); if (b) { menu.hidden = true; entries[b.dataset.i].run(); } };
  menu.hidden = false;
  const r = menu.getBoundingClientRect();
  if (above) y = y - r.height;
  menu.style.left = `${Math.max(8, Math.min(x, innerWidth - r.width - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, innerHeight - r.height - 8))}px`;
}
function itemMenu(it, x, y, above = false) {
  showMenu(x, y, [
    { icon: "open", label: "Open", keys: ["Enter"], enabled: it.status === "completed", run: () => act("open", it) },
    { icon: "folder", label: "Show in folder", keys: ["O"], run: () => act("folder", it) },
    "-",
    isActive(it) ? { icon: "pause", label: "Pause", keys: ["Space"], run: () => pause([it.id]) }
      : { icon: it.status === "error" ? "retry" : "play", label: it.status === "error" ? "Retry" : "Resume", keys: ["Space"], enabled: it.status !== "completed", run: () => resume([it.id]) },
    { icon: "link", label: "Copy link", keys: ["C"], run: () => act("copy", it) },
    "-",
    { icon: "trash", label: it.status === "completed" ? "Remove…" : "Cancel download…", keys: ["Del"], danger: true, run: () => askDelete(it) },
  ], above);
}
document.addEventListener("mousedown", (e) => { if (!e.target.closest("#menu") && !e.target.closest("#limit-btn") && !e.target.closest("[data-ins=more]")) $("menu").hidden = true; });
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input")) e.preventDefault(); });

// ---------- events ----------
$("rows").addEventListener("mousedown", (e) => { const r = e.target.closest(".row"); if (r) select(Number(r.dataset.id), false); });
$("rows").addEventListener("dblclick", (e) => {
  const r = e.target.closest(".row"), it = r && byId(Number(r.dataset.id));
  if (it) it.status === "completed" ? act("open", it) : toggle(it);
});
$("rows").addEventListener("contextmenu", (e) => { const r = e.target.closest(".row"); if (r) { e.preventDefault(); itemMenu(byId(Number(r.dataset.id)), e.clientX, e.clientY); } });
$("list").addEventListener("mousedown", (e) => { if (!e.target.closest(".row") && !e.target.closest(".ghead") && !e.target.closest(".empty") && state.selected != null) { state.selected = null; render(); } });
$("inspector").addEventListener("click", (e) => {
  const b = e.target.closest("[data-ins]");
  if (!b) return;
  if (b.dataset.ins === "close") { state.selected = null; render(); return; }
  act(b.dataset.ins, byId(state.selected), b);
});
$("nav").addEventListener("click", (e) => {
  if (e.target.closest("[data-toggle]")) { state.typesOpen = !state.typesOpen; render(); return; }
  const b = e.target.closest("[data-view]");
  if (b) { state.view = b.dataset.view; state.search = ""; render(); }
});
$("btn-pause-all").onclick = () => pause(visible().filter(isActive).map((i) => i.id));
$("btn-resume-all").onclick = () => resume(visible().filter((i) => i.status === "paused" || i.status === "error").map((i) => i.id));
$("btn-add").onclick = $("empty-add").onclick = () => openComposer();
$("btn-palette").onclick = $("btn-search").onclick = () => openPalette();
$("btn-settings").onclick = () => openSettings();
$("empty-ext").onclick = () => openSettings("browser");
$("w-min").onclick = () => appWindow?.minimize();
$("w-max").onclick = () => appWindow?.toggleMaximize();
$("w-close").onclick = () => appWindow?.close();
if (!appWindow) $("win-ctrls").hidden = true;

async function setLimit(k) {
  await saveSettings({ ...state.settings, speedLimitKib: k });
  toast(k ? `Speed limited to ${bytes(k * 1024)}/s` : "Speed limit removed", "ok");
}
const limitEntries = () => [
  { title: "Speed limit" },
  ...LIMITS.map((k) => ({ label: k ? `${bytes(k * 1024)}/s` : "No limit", checked: k === state.settings.speedLimitKib, run: () => setLimit(k) })),
  "-",
  { icon: "sliders", label: "Custom…", run: () => openSettings("speed") },
];
$("limit-btn").onclick = (e) => { const r = e.currentTarget.getBoundingClientRect(); showMenu(r.left, r.top - 6, limitEntries(), true); };

document.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey, k = e.key;
  if (mod && k.toLowerCase() === "k") { e.preventDefault(); if (!$("dlg-palette").open) openPalette(); return; }
  if (document.querySelector("dialog[open]") || e.target.matches?.("input")) return;
  const it = state.selected != null ? byId(state.selected) : null;
  if (mod && k.toLowerCase() === "n") { e.preventDefault(); openComposer(); }
  else if (mod && k === ",") { e.preventDefault(); openSettings(); }
  else if (mod) return;
  else if (k === "ArrowDown" || k === "j") { e.preventDefault(); move(1); }
  else if (k === "ArrowUp" || k === "k") { e.preventDefault(); move(-1); }
  else if (k === " ") { e.preventDefault(); toggle(it); }
  else if (k === "Enter" && it) { e.preventDefault(); it.status === "completed" ? act("open", it) : toggle(it); }
  else if ((k === "Delete" || k === "Backspace") && it) { e.preventDefault(); askDelete(it); }
  else if (k === "o" && it) act("folder", it);
  else if (k === "c" && it) act("copy", it);
  else if (k === "n") openComposer();
  else if (k === "/") { e.preventDefault(); openPalette(); }
  else if (k === "Escape") { state.selected = null; render(); }
});

const firstUrl = (t) => (t || "").split(/\s+/).find((s) => /^(https?:\/\/|magnet:\?)\S+$/i.test(s));
document.addEventListener("paste", (e) => {
  if (document.querySelector("dialog[open]") || e.target.matches?.("input")) return;
  const url = firstUrl(e.clipboardData.getData("text"));
  if (url) openComposer({ url });
});
let drag = 0;
document.addEventListener("dragenter", (e) => { if ([...e.dataTransfer.types].some((t) => t === "text/uri-list" || t === "text/plain")) { drag++; $("drop").hidden = false; } });
document.addEventListener("dragleave", () => { if (--drag <= 0) { drag = 0; $("drop").hidden = true; } });
document.addEventListener("dragover", (e) => e.preventDefault());
document.addEventListener("drop", (e) => {
  e.preventDefault(); drag = 0; $("drop").hidden = true;
  const url = firstUrl(e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain"));
  if (url) openComposer({ url });
});

// ---------- command palette ----------
let pal = { items: [], sel: 0 };
function paletteItems(q) {
  q = q.trim().toLowerCase();
  const match = (s) => !q || s.toLowerCase().includes(q);
  const out = [];
  const actions = [
    { icon: "plus", label: "New download", keys: ["Ctrl", "N"], run: () => openComposer() },
    { icon: "pause", label: "Pause all downloads", run: () => pause(state.items.filter(isActive).map((i) => i.id)) },
    { icon: "play", label: "Resume all downloads", run: () => resume(state.items.filter((i) => i.status === "paused" || i.status === "error").map((i) => i.id)) },
    ...LIMITS.map((k) => ({ icon: "gauge", label: k ? `Limit speed to ${bytes(k * 1024)}/s` : "Remove speed limit", run: () => setLimit(k) })),
    { icon: "settings", label: "Open settings", keys: ["Ctrl", ","], run: () => openSettings() },
    { icon: "globe", label: "Set up browser extension", run: () => openSettings("browser") },
  ].filter((a) => match(a.label));
  const views = [
    ...Object.entries(VIEWS).map(([k, v]) => ({ icon: v.icon, label: `Go to ${v.name}`, run: () => { state.view = k; render(); } })),
    ...Object.keys(CATS).map((c) => ({ type: c, label: `Go to ${c}`, run: () => { state.view = c; render(); } })),
  ].filter((a) => match(a.label));
  const dls = state.items.filter((i) => q && (i.filename.toLowerCase().includes(q) || i.url.toLowerCase().includes(q))).slice(0, 8)
    .map((i) => ({ status: i, label: i.filename, sub: STATUS[i.status], run: () => { state.view = "all"; select(i.id); } }));
  if (dls.length) out.push({ title: "Downloads" }, ...dls);
  if (actions.length) out.push({ title: "Actions" }, ...actions);
  if (views.length && q) out.push({ title: "Navigation" }, ...views);
  else if (!q) out.push({ title: "Navigation" }, ...views.slice(0, 4));
  return out;
}
function renderPalette() {
  const q = $("pal-q").value;
  pal.items = paletteItems(q);
  const selectable = pal.items.filter((i) => !i.title);
  pal.sel = Math.min(pal.sel, Math.max(0, selectable.length - 1));
  let n = -1;
  $("pal-list").innerHTML = selectable.length ? pal.items.map((i) => {
    if (i.title) return `<div class="pal-sec">${i.title}</div>`;
    n++;
    const ic = i.status ? statusIcon(i.status) : i.type ? typeIcon(i.type) : icon(i.icon);
    return `<button class="pal-item ${n === pal.sel ? "on" : ""}" data-n="${n}"><span class="pi-ic">${ic}</span><span class="pi-label">${esc(i.label)}${i.sub ? `<span class="pi-sub">${i.sub}</span>` : ""}</span>${i.keys ? `<span class="pi-right">${i.keys.map((k) => `<kbd>${k}</kbd>`).join("")}</span>` : ""}</button>`;
  }).join("") : `<div class="pal-empty">No results for “${esc(q)}”</div>`;
  $("pal-list").querySelector(".pal-item.on")?.scrollIntoView({ block: "nearest" });
}
function openPalette() {
  pal.sel = 0;
  $("pal-q").value = "";
  renderPalette();
  $("dlg-palette").showModal();
  $("pal-q").focus();
}
function runPalette(n) {
  const item = pal.items.filter((i) => !i.title)[n];
  if (!item) return;
  $("dlg-palette").close();
  item.run();
}
$("pal-q").addEventListener("input", () => { pal.sel = 0; renderPalette(); });
$("pal-q").addEventListener("keydown", (e) => {
  const count = pal.items.filter((i) => !i.title).length;
  if (e.key === "ArrowDown") { e.preventDefault(); pal.sel = (pal.sel + 1) % Math.max(1, count); renderPalette(); }
  else if (e.key === "ArrowUp") { e.preventDefault(); pal.sel = (pal.sel - 1 + count) % Math.max(1, count); renderPalette(); }
  else if (e.key === "Enter") { e.preventDefault(); runPalette(pal.sel); }
});
$("pal-list").addEventListener("mousemove", (e) => {
  const b = e.target.closest(".pal-item");
  if (b && Number(b.dataset.n) !== pal.sel) { pal.sel = Number(b.dataset.n); $("pal-list").querySelectorAll(".pal-item").forEach((x) => x.classList.toggle("on", x === b)); }
});
$("pal-list").addEventListener("click", (e) => { const b = e.target.closest(".pal-item"); if (b) runPalette(Number(b.dataset.n)); });
$("dlg-palette").addEventListener("click", (e) => { if (e.target === $("dlg-palette")) $("dlg-palette").close(); });

// ---------- composer (new download) ----------
const add = { headers: [], seq: 0, timer: null, dirTouched: false, dir: "", info: null };
function openComposer(incoming = null) {
  const dlg = $("dlg-add");
  if (dlg.open) return;
  add.headers = incoming?.headers || [];
  add.suggest = incoming?.filename || "";
  add.dirTouched = false;
  add.info = null;
  add.dir = state.settings?.downloadDir || "";
  $("add-url").value = incoming?.url || "";
  $("add-file").hidden = true;
  renderChips();
  setState("idle");
  dlg.showModal();
  $("add-url").focus();
  if (incoming?.url) probe();
  else readClip().then((u) => { if (u && !$("add-url").value) { $("add-url").value = u; $("add-url").select(); probe(); } });
}
const readClip = () => (navigator.clipboard?.readText?.() || Promise.resolve("")).then(firstUrl).catch(() => null);
function setState(kind, msg = "") {
  const el = $("add-state");
  el.className = `comp-state ${kind === "error" ? "err" : ""}`;
  el.innerHTML = kind === "loading" ? `<span class="spinner"></span>Fetching file details…` : kind === "error" ? `${icon("alert")}${esc(msg)}` : kind === "idle" ? `SwiftDL checks the file's name and size before downloading.` : "";
  el.hidden = kind === "done";
  $("add-start").disabled = $("add-later").disabled = kind !== "done";
}
function renderChips() {
  const i = add.info;
  const folder = add.dir.split(/[\\/]/).filter(Boolean).slice(-2).join(" / ") || add.dir;
  $("add-chips").innerHTML = i ? [
    `<span class="chip">${icon("hash")}<span>${i.size != null ? bytes(i.size) : "Unknown size"}</span></span>`,
    `<span class="chip"><i class="dot" style="background:${(CATS[i.category] || CATS.Other).color}"></i><span>${esc(i.category)}</span></span>`,
    i.resumable ? `<span class="chip ok">${icon("bolt")}<span>${state.settings?.connections || 8} connections</span></span>` : `<span class="chip warn">${icon("info")}<span>Single connection, no resume</span></span>`,
    `<button type="button" class="chip" id="add-folder" title="${esc(add.dir)}">${icon("folder")}<span>${esc(folder)}</span>${icon("chevDown")}</button>`,
  ].join("") : "";
}
async function probe() {
  const url = $("add-url").value.trim();
  if (!/^(https?:\/\/|magnet:\?)\S+/i.test(url)) { add.info = null; $("add-file").hidden = true; renderChips(); url ? setState("error", "Paste a web link (http:// or https://) or a magnet link") : setState("idle"); return; }
  const seq = ++add.seq;
  // Video sites need a quality and torrents a file choice: their own window.
  const special = await invoke("is_media_url", { url }).catch(() => false) || await invoke("is_torrent_url", { url, filename: null }).catch(() => false);
  if (seq !== add.seq) return;
  if (special) {
    $("dlg-add").close();
    invoke("open_new_download", { url, headers: add.headers });
    return;
  }
  setState("loading");
  try {
    const info = await invoke("probe_url", { url, headers: add.headers });
    if (seq !== add.seq) return;
    if (info.streamKind === "hls") {
      $("dlg-add").close();
      await call("open_new_download", { url, streamKind: "hls", headers: add.headers });
      return;
    }
    add.info = info;
    if (!add.dirTouched) add.dir = info.dir;
    // A name from the extension (e.g. the video page's title) beats a
    // generic server name like "videoplayback".
    $("add-filename").value = /\.[a-z0-9]{2,5}$/i.test(add.suggest) ? add.suggest : info.filename;
    $("add-tile").innerHTML = ftile(info);
    $("add-sub").textContent = host(url);
    $("add-file").hidden = false;
    renderChips();
    setState("done");
  } catch (e) {
    if (seq !== add.seq) return;
    add.info = null; $("add-file").hidden = true; renderChips();
    setState("error", `Couldn't reach that link: ${e}`);
  }
}
$("add-url").addEventListener("input", () => { clearTimeout(add.timer); add.timer = setTimeout(probe, 400); });
$("add-chips").addEventListener("click", async (e) => {
  if (!e.target.closest("#add-folder")) return;
  const dir = await call("pick_folder", { start: add.dir || null });
  if (dir) { add.dir = dir; add.dirTouched = true; renderChips(); }
});
$("add-paste").onclick = async () => { const u = await readClip(); if (u) { $("add-url").value = u; probe(); } else toast("No link on the clipboard"); };
$("add-x").onclick = () => $("dlg-add").close();
$("add-later").onclick = () => submitAdd(false);
$("form-add").addEventListener("submit", (e) => { e.preventDefault(); if (!$("add-start").disabled) submitAdd(true); });
$("dlg-add").addEventListener("keydown", (e) => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && !$("add-start").disabled) { e.preventDefault(); submitAdd(true); } });
$("dlg-add").addEventListener("close", () => { add.seq++; setTimeout(nextPending, 150); });
async function submitAdd(start) {
  const filename = $("add-filename").value.trim();
  if (!filename) return;
  const id = await call("add_download", { req: { url: $("add-url").value.trim(), filename, dir: add.dir || state.settings.downloadDir, headers: add.headers, start } });
  $("dlg-add").close();
  if (state.view !== "all" && state.view !== "active") state.view = "all";
  select(id);
  toast(start ? `Downloading ${esc(filename)}` : `${esc(filename)} added for later`, start ? "dl" : "ok");
}
function nextPending() {
  if ($("dlg-add").open || !state.pending.length) return;
  const inc = state.pending.shift(), headers = [];
  if (inc.cookies) headers.push(["Cookie", inc.cookies]);
  if (inc.referrer) headers.push(["Referer", inc.referrer]);
  if (inc.userAgent) headers.push(["User-Agent", inc.userAgent]);
  openComposer({ url: inc.url, headers, filename: inc.filename });
}

// ---------- settings ----------
let setDir = "", setLimitKib = 0, setThemeChoice = "system", setSeed = "stop";
function renderSeed() {
  $("set-seed").querySelectorAll("[data-v]").forEach((b) => b.classList.toggle("on", b.dataset.v === setSeed));
}
$("set-seed").addEventListener("click", (e) => { const b = e.target.closest("[data-v]"); if (b) { setSeed = b.dataset.v; renderSeed(); } });
function renderThemeChoice() {
  $("set-theme").querySelectorAll("[data-v]").forEach((b) => b.classList.toggle("on", b.dataset.v === setThemeChoice));
}
// Picking a theme previews it at once; Cancel puts the saved one back.
$("set-theme").addEventListener("click", (e) => {
  const b = e.target.closest("[data-v]");
  if (!b) return;
  setThemeChoice = b.dataset.v;
  renderThemeChoice();
  window.setTheme(setThemeChoice);
});
const restoreTheme = () => window.setTheme(state.settings.theme);
$("dlg-settings").addEventListener("cancel", restoreTheme); // Escape
$("dlg-settings").addEventListener("close", restoreTheme);
function openSettings(tab = "general") {
  const s = state.settings;
  setThemeChoice = s.theme || "system";
  renderThemeChoice();
  setSeed = s.seedUntil || "stop";
  renderSeed();
  setDir = s.downloadDir; $("set-dir").textContent = setDir;
  $("set-sort").checked = s.sortIntoCategories;
  $("set-ask").checked = s.askBeforeDownload;
  $("set-notify").checked = s.notifyOnComplete;
  $("set-offer").checked = s.offerExtension;
  $("set-progress-win").checked = s.showProgressWindow;
  $("set-tray").checked = s.closeToTray;
  loadIntegration();
  $("set-conns").value = s.connections;
  $("set-concurrent").value = s.maxConcurrent;
  setLimitKib = s.speedLimitKib;
  renderLimits();
  showTab(tab);
  $("dlg-settings").showModal();
}
function renderLimits() {
  $("set-limits").innerHTML = LIMITS.map((k) => `<button type="button" data-k="${k}" class="${k === setLimitKib ? "on" : ""}">${k ? bytes(k * 1024) + "/s" : "Off"}</button>`).join("");
  const c = $("set-limit-custom");
  if (document.activeElement !== c) c.value = LIMITS.includes(setLimitKib) ? "" : setLimitKib;
}
$("set-limits").addEventListener("click", (e) => { const b = e.target.closest("[data-k]"); if (b) { setLimitKib = Number(b.dataset.k); renderLimits(); } });
$("set-limit-custom").addEventListener("input", (e) => { setLimitKib = Math.max(0, Number.parseInt(e.target.value, 10) || 0); renderLimits(); });
function showTab(tab) {
  $("set-nav").querySelectorAll("[data-tab]").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  document.querySelectorAll("[data-panel]").forEach((p) => (p.hidden = p.dataset.panel !== tab));
}
$("set-nav").addEventListener("click", (e) => { const b = e.target.closest("[data-tab]"); if (b) showTab(b.dataset.tab); });
document.querySelectorAll(".stepper").forEach((st) => st.addEventListener("click", (e) => {
  const b = e.target.closest("[data-d]");
  if (!b) return;
  const input = $(st.dataset.for);
  input.value = Math.min(+input.max, Math.max(+input.min, (+input.value || 0) + Number(b.dataset.d)));
}));
$("set-browse").onclick = async () => { const d = await call("pick_folder", { start: setDir || null }); if (d) { setDir = d; $("set-dir").textContent = d; } };
$("set-cancel").onclick = () => { restoreTheme(); $("dlg-settings").close(); };
$("set-ext-folder").onclick = () => call("open_extension_folder");
async function saveSettings(s) { await call("set_settings", { settings: s }); state.settings = s; render(); }
$("form-settings").addEventListener("submit", async (e) => {
  e.preventDefault();
  const clamp = (v, lo, hi, d) => Math.min(hi, Math.max(lo, Number.parseInt(v, 10) || d));
  await saveSettings({
    downloadDir: setDir || state.settings.downloadDir,
    sortIntoCategories: $("set-sort").checked,
    connections: clamp($("set-conns").value, 1, 32, 8),
    maxConcurrent: clamp($("set-concurrent").value, 1, 10, 3),
    speedLimitKib: setLimitKib,
    askBeforeDownload: $("set-ask").checked,
    notifyOnComplete: $("set-notify").checked,
    offerExtension: $("set-offer").checked,
    showProgressWindow: $("set-progress-win").checked,
    closeToTray: $("set-tray").checked,
    onboardingDismissed: state.settings.onboardingDismissed,
    theme: setThemeChoice,
    seedUntil: setSeed,
  });
  $("dlg-settings").close();
  toast("Settings saved", "ok");
});

// ---------- data ----------
function ingest(items) {
  state.items = items;
  let total = 0;
  for (const it of items) {
    if (it.status !== "downloading") continue;
    const h = state.speedHist.get(it.id) || [];
    h.push(it.speed || 0);
    if (h.length > HISTORY) h.shift();
    state.speedHist.set(it.id, h);
    total += it.speed || 0;
  }
  state.totalHist.push(total);
  if (state.totalHist.length > HISTORY) state.totalHist.shift();
  render();
}
async function boot() {
  hydrate();
  const s = await invoke("get_state");
  state.settings = s.settings;
  window.setTheme(s.settings.theme);
  state.pending = s.pending;
  $("version").textContent = `Version ${s.version}`;
  ingest(s.items);
  nextPending();
  loadIntegration();
  setInterval(loadIntegration, 15000);
  await listen("downloads", (e) => ingest(e.payload));
  await listen("incoming", () => invoke("get_state").then((s) => { state.pending.push(...s.pending); nextPending(); }));
  setInterval(() => {
    if (!state.items.some((i) => i.status === "downloading")) {
      state.totalHist.push(0);
      if (state.totalHist.length > HISTORY) state.totalHist.shift();
      areaChart($("net-graph"), state.totalHist);
    }
  }, 500);
  addEventListener("resize", () => render());
}
boot();
