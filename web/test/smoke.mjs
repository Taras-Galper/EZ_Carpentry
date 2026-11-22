// Browser smoke test for web/dist/index.html on both engines (WebAssembly
// and the pure-JS fallback). Usage: node web/test/smoke.mjs [outdir]
// Needs Playwright (npm i -g playwright) and a Chrome/Chromium.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const require = createRequire(import.meta.url);
let pw;
try { pw = require("playwright"); } catch { pw = require(path.join(process.env.PW_HOME || "/opt/npm-tools/node_modules", "playwright")); }
const here = path.dirname(fileURLToPath(import.meta.url));
const url = "file://" + path.resolve(here, "../dist/index.html");
const out = process.argv[2] || "/tmp/ezc-smoke";
fs.mkdirSync(out, { recursive: true });
const exe = ["/usr/bin/google-chrome", "/usr/bin/chromium"].find((p) => fs.existsSync(p));
const browser = await pw.chromium.launch({ executablePath: exe, args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
let failed = 0;
const check = (ok, msg) => { console.log((ok ? "ok   " : "FAIL ") + msg); if (!ok) failed++; };

for (const engine of ["wasm", "js"]) {
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  if (engine === "js") await page.addInitScript(() => { window.__forceJs = true; });
  await page.goto(url);
  await page.waitForFunction(() => window.ezCarpentry?.project);
  for (const type of ["L corner", "Wall L"]) {
    await page.click(`[data-add="${type}"]`);
    await page.waitForTimeout(300);
  }
  const st = await page.evaluate(() => {
    const e = window.ezCarpentry, sel = e.selected;
    return { n: e.project.cabinets.length, sel, corner: e.project.cabinets.find((c) => c.id === sel)?.corner };
  });
  check(st.corner === "l", `${engine}: Wall L added and selected (${st.sel})`);
  await page.screenshot({ path: `${out}/${engine}-desk.png` });
  check(errors.length === 0, `${engine}: no page errors ${errors.join(" | ")}`);
  await page.close();
}

const phone = await browser.newContext({ ...pw.devices["iPhone 13"] });
const pp = await phone.newPage();
await pp.goto(url);
await pp.waitForFunction(() => window.ezCarpentry?.project);
await pp.screenshot({ path: `${out}/phone.png` });
check(true, "phone loads");
await browser.close();
console.log(failed ? `${failed} failed` : "all passed");
process.exit(failed ? 1 : 0);
