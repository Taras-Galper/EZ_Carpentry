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
// Room geometry (pure JS).
const { resizeWall } = await import("../src/room.js");
{
  const room = { walls: [[0, 0], [3200, 0], [3200, 2600], [0, 2600]], openings: [{ wall: 0, offset: 1300, width: 700 }] };
  const ok = (a, L, walls, off) => {
    const r = resizeWall(room, 0, L, a);
    const good = JSON.stringify(r.walls) === JSON.stringify(walls) && r.openings[0].offset === off;
    console.log((good ? "ok   " : "FAIL ") + `resize wall, ${a} fixed`);
    if (!good) process.exitCode = 1;
  };
  ok("start", 4000, [[0, 0], [4000, 0], [4000, 2600], [0, 2600]], 1300);
  ok("end", 4000, [[-800, 0], [3200, 0], [3200, 2600], [-800, 2600]], 2100);
  ok("centre", 4000, [[-400, 0], [3600, 0], [3600, 2600], [-400, 2600]], 1700);
}

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
  await page.click('[data-tab="drawings"]');
  await page.waitForTimeout(300);
  const figs = await page.locator("#v-drawings figure").count();
  check(figs > 0, `${engine}: drawings tab shows ${figs} drawings`);
  check(errors.length === 0, `${engine}: no page errors ${errors.join(" | ")}`);
  await page.close();
}

// Snap: drag a cabinet near the back wall in Plan; it lands flush, beside its neighbour,
// and the gap chip shows while dragging.
{
  const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
  await page.goto(url);
  await page.waitForFunction(() => window.ezCarpentry?.project);
  await page.click("text=New").catch(() => {});
  await page.waitForTimeout(300);
  const n0 = await page.evaluate(() => window.ezCarpentry.project.cabinets.length);
  if (n0 === 0) {
    await page.click('[data-add="Base"]'); await page.click('[data-add="Base"]');
    await page.click('[data-view="plan"]'); await page.waitForTimeout(400);
    const p0 = await page.evaluate(() => { const c = window.ezCarpentry.project.cabinets[1]; return window.ezCarpentry.worldToScreen(c.x + 450, 700, c.z + 400); });
    const room = await page.evaluate(() => window.ezCarpentry.project.room.walls.length);
    const p1 = await page.evaluate(() => window.ezCarpentry.worldToScreen(1700, 400, 450));
    await page.mouse.move(p0.x, p0.y); await page.mouse.down();
    await page.mouse.move(p1.x, p1.y, { steps: 15 });
    const chip = await page.locator(".snapchip").count();
    await page.mouse.up(); await page.waitForTimeout(300);
    const cabs = await page.evaluate(() => window.ezCarpentry.project.cabinets.map((c) => [c.x, c.z]).sort((a, b) => a[0] - b[0]));
    if (room >= 3) {
      check(chip === 1, "snap chip while dragging");
      check(cabs[1][1] === 0 && cabs[1][0] === cabs[0][0] + 600, `dragged cabinet flush and beside its neighbour ${JSON.stringify(cabs)}`);
    } else console.log("skip snap check: new project has no room");
  } else console.log("skip snap check: New did not empty the project");
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
process.exit(failed || process.exitCode ? 1 : 0);
