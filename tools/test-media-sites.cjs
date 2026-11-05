// Run with: node tools/test-media-sites.cjs
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.resolve(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");

async function main() {
  const rust = read("app/src-tauri/src/media.rs");
  const sites = [...rust.match(/const SITES: &[\s\S]*?= &\[([\s\S]*?)\];/)[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  const preview = { window: {}, location: { search: "?video" }, URL };
  vm.runInNewContext(read("app/ui/mock.js"), preview);
  const invoke = preview.window.__TAURI__.core.invoke;
  for (const site of sites) {
    for (const host of [site, `www.${site}`, `m.${site}`, site.toUpperCase()]) {
      assert.equal(await invoke("is_media_url", { url: `https://${host}/video/123` }), true, host);
    }
    for (const url of [`https://not${site}/video`, `https://${site}.example.org/video`, `https://example.org/path/.${site}/video`, `https://example.org/?url=https://www.${site}/video`]) {
      assert.equal(await invoke("is_media_url", { url }), false, url);
    }
  }
  assert.equal(await invoke("is_media_url", { url: "not a URL" }), false);
  for (const file of ["extension/content.js", "extension/popup.js"]) {
    const context = {};
    const declaration = read(file).match(/const VIDEO_SITE = .*;/)[0];
    vm.runInNewContext(`${declaration}\nglobalThis.pattern = VIDEO_SITE;`, context);
    for (const site of ["xvideos.com", "xnxx.com", "hqporner.com", "dailymotion.com"]) {
      for (const host of [site, `www.${site}`, `geo.${site}`, site.toUpperCase()]) {
        assert(context.pattern.test(host), `${file}: ${host}`);
      }
      for (const host of [`not${site}`, `${site}.example.org`]) {
        assert(!context.pattern.test(host), `${file}: ${host}`);
      }
    }
  }
  console.log("Media-site routing checks passed (preview, content script, popup)");
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
