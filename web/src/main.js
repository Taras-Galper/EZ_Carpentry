import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";
import init, * as cwWasm from "../pkg/casework_wasm.js";
import wasmBytes from "../pkg/casework_wasm_bg.wasm";
import { wallOf, walls as roomWalls, local as wallLocal, point as wallPoint, wallPieces, snapCorner, resizeWall, moveWithWalls } from "./room.js";

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const h = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (k === "class") e.className = v;
    else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
  }
  for (const k of kids.flat()) if (k != null) e.append(k.nodeType ? k : String(k));
  return e;
};

const STORE = "casework.project.v1"; // single-project key from earlier versions
// Projects kept in this browser: index + one entry per project.
const P_INDEX = "ezc.projects", P_CUR = "ezc.current", P_KEY = (id) => "ezc.p." + id;
const DEFAULTS = "ezc.defaults", PRESETS = "ezc.presets";
let projectId = null;
let project, report, selected = null, tab = "3d", showFronts = true;
// Multi-selection: Shift-click adds cabinets to `selected`. selSet holds the
// others; it is dropped whenever `selected` changes outside multi-select.
const selSet = new Set();
let selAnchor = null;
const isSel = (id) => id === selected || selSet.has(id);
const selection = () => project.cabinets.filter((c) => isSel(c.id));
let selWall = null;   // index of the selected wall, or null
let lenAnchor = "start"; // which point of a wall stays put when its length is typed: start | centre | end
let drawing = null;   // { pts: [[x, z]...], cursor: [x, z] } while drawing a room
let downloads = null;
let cw = cwWasm, engine = "wasm";

// WebAssembly first; pure-JS build of the same Rust code if the host blocks it.
async function loadEngine() {
  if (!window.__forceJs) {
    try { await init({ module_or_path: wasmBytes }); cw = cwWasm; return; } catch (e) { console.warn("wasm unavailable, using JS build", e); }
  }
  cw = await import("../pkgjs/casework_wasm.js");
  engine = "js";
}

// ---------- phone: bottom sheets ----------
const phone = matchMedia("(max-width: 760px)");
function openSheet(name) {
  // name: "left" | "right" | null; tapping the open one closes it
  const cur = document.body.classList.contains("sheet-left") ? "left" : document.body.classList.contains("sheet-right") ? "right" : null;
  const next = name === cur ? null : name;
  document.body.classList.toggle("sheet-left", next === "left");
  document.body.classList.toggle("sheet-right", next === "right");
  $$("#mbar button").forEach((b) => b.classList.toggle("on", b.dataset.sheet === next));
}
const closeSheetOnPhone = () => { if (phone.matches) openSheet(null); };

// ---------- persistence ----------
const lsGet = (k, d = null) => { try { const v = localStorage.getItem(k); return v == null ? d : JSON.parse(v); } catch { return d; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, JSON.stringify(v)); return true; } catch { return false; } };
const lsDel = (k) => { try { localStorage.removeItem(k); } catch {} };
const newId = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
function projectIndex() { return lsGet(P_INDEX, []); }
function saveLocal() {
  if (!projectId) projectId = newId();
  lsSet(P_KEY(projectId), project);
  const idx = projectIndex().filter((e) => e.id !== projectId);
  idx.unshift({ id: projectId, name: project.name, updated: Date.now(), cabinets: project.cabinets.length });
  lsSet(P_INDEX, idx);
  lsSet(P_CUR, projectId);
}
function loadLocal() {
  // move a project saved by an earlier version into the list
  const old = lsGet(STORE);
  if (old && !projectIndex().length) { projectId = newId(); project = old; saveLocal(); lsDel(STORE); }
  const id = lsGet(P_CUR);
  const p = id && lsGet(P_KEY(id));
  if (p) { projectId = id; return p; }
  return null;
}
// Switch to another project: fresh id unless opening a stored one, history cleared.
function useProject(p, id = null) {
  project = migrate(p);
  projectId = id || newId();
  undoStack.length = 0; redoStack.length = 0; lastSnap = null;
  selected = null; selWall = null; framed = false;
  recompute();
}
// Shop defaults (materials, prices, CNC, construction) applied to new projects.
function freshProject() {
  const p = JSON.parse(cw.sample_project());
  const d = lsGet(DEFAULTS);
  p.cabinets = []; p.name = "Untitled";
  if (d) Object.assign(p, { materials: d.materials, pricing: d.pricing, settings: d.settings, cnc: d.cnc }, d.room ? { room: { ...p.room, ...d.room } } : {});
  return p;
}
function saveDefaults() {
  const ok = lsSet(DEFAULTS, { materials: project.materials, pricing: project.pricing, settings: project.settings, cnc: project.cnc, room: { thickness: project.room.thickness, height: project.room.height } });
  toast(ok ? "Saved as shop defaults" : "Browser storage unavailable");
}
function openProjects() {
  const dlg = $("#projects"); dlg.replaceChildren();
  const fmt = (t) => new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  const close = () => { dlg.hidden = true; };
  const rows = projectIndex().map((e) => {
    const del = h("button", { class: "danger" }, "Delete");
    del.onclick = () => {
      if (del.dataset.armed) {
        lsDel(P_KEY(e.id)); lsSet(P_INDEX, projectIndex().filter((x) => x.id !== e.id));
        if (e.id === projectId) useProject(freshProject());
        openProjects();
      } else { del.dataset.armed = 1; del.textContent = "Delete?"; }
    };
    return h("tr", { class: e.id === projectId ? "sel" : "" },
      h("td", {}, h("b", {}, e.name || "Untitled")), h("td", { class: "mute" }, `${e.cabinets ?? ""} cabinets`), h("td", { class: "mute" }, fmt(e.updated)),
      h("td", { class: "n" },
        h("button", { onclick: () => { const p = lsGet(P_KEY(e.id)); if (p) useProject(p, e.id); close(); } }, "Open"), " ",
        h("button", { onclick: () => { const p = lsGet(P_KEY(e.id)); if (p) { p.name = (p.name || "Untitled") + " copy"; useProject(p); } close(); } }, "Copy"), " ",
        del));
  });
  dlg.append(h("div", { class: "dlg" },
    h("div", { class: "row" }, h("h2", { class: "grow" }, "Projects"), h("button", { onclick: close }, "Close")),
    rows.length ? h("table", {}, h("tbody", {}, rows)) : h("p", { class: "mute" }, "—")));
  dlg.hidden = false;
}

// ---------- files ----------
const isHosted = () => !!(window.claude && window.claude.use);
const ALLOWED = ["txt", "json", "csv", "svg", "zip", "html"];
async function saveFile(name, data) {
  let ext = name.split(".").pop().toLowerCase();
  if (downloads) {
    if (!ALLOWED.includes(ext)) {
      const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
      data = cw.zip_one(name, bytes);
      name += ".zip";
    }
    try { await downloads.save({ filename: name, data: data instanceof Uint8Array ? data.slice().buffer : data }); }
    catch (e) { if (e?.code && e.code !== "declined") toast(e.message || e.code); }
    return;
  }
  const blob = new Blob([data], { type: "application/octet-stream" });
  const a = h("a", { href: URL.createObjectURL(blob), download: name });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}
const slug = (s) => (s || "project").replace(/[^\w-]+/g, "_");

function toast(msg) {
  const t = $("#toast"); t.textContent = msg; t.classList.add("on");
  clearTimeout(toast.t); toast.t = setTimeout(() => t.classList.remove("on"), 2600);
}

// ---------- history ----------
// Every recompute compares the project to the last snapshot, so any edit is undoable
// and selection-only changes are not.
const undoStack = [], redoStack = [];
let lastSnap = null;
function track(json) {
  if (lastSnap !== null && json !== lastSnap) {
    undoStack.push(lastSnap);
    if (undoStack.length > 200) undoStack.shift();
    redoStack.length = 0;
  }
  lastSnap = json;
}
function stepHistory(from, to) {
  if (!from.length) return;
  to.push(lastSnap);
  lastSnap = from.pop();
  project = JSON.parse(lastSnap);
  if (selected && !project.cabinets.some((c) => c.id === selected)) selected = null;
  recompute();
}
const undo = () => stepHistory(undoStack, redoStack);
const redo = () => stepHistory(redoStack, undoStack);

// Bring any loaded project up to date: add newer default materials (with
// their prices), then let the Rust parser fill every other missing field.
function migrate(p) {
  const defaults = JSON.parse(cw.sample_project()).materials;
  p.materials = p.materials || [];
  for (const m of p.materials) if (m.price == null) m.price = defaults.find((d) => d.id === m.id)?.price ?? 0;
  for (const m of defaults) if (!p.materials.some((x) => x.id === m.id)) p.materials.push(m);
  return JSON.parse(cw.normalize(JSON.stringify(p)));
}

// ---------- compute ----------
function recompute() {
  if (selected !== selAnchor) selSet.clear();
  for (const id of [...selSet]) if (id === selected || !project.cabinets.some((c) => c.id === id)) selSet.delete(id);
  selAnchor = selected;
  const json = JSON.stringify(project);
  try {
    report = JSON.parse(cw.compute(json));
  } catch (e) {
    toast(String(e.message || e)); return;
  }
  track(json);
  $("#undo").disabled = !undoStack.length;
  $("#redo").disabled = !redoStack.length;
  saveLocal();
  renderPlanTools();
  $("#mbsel").textContent = selWall != null ? `wall ${selWall + 1}` : selected || "";
  renderList();
  renderProps();
  renderStatus();
  renderTab();
  renderDial();
}

// ---------- cabinet presets ----------
const uid = (p) => { let i = 1; while (project.cabinets.some((c) => c.id === p + i)) i++; return p + i; };
const gid = (p) => { let i = 1; while (project.cabinets.some((c) => c.group === p + i)) i++; return p + i; };
const CAB_PRESETS = {
  Base: () => ({ kind: "base", name: "Base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [160], shelves: 1 }),
  Drawers: () => ({ kind: "base", name: "Drawer base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [180, 300, 290], shelves: 0 }),
  Wall: () => ({ kind: "wall", name: "Wall", width: 600, height: 720, depth: 320, elevation: 1450, toe_kick_height: 0, doors: 1, drawers: [], shelves: 2 }),
  Sink: () => ({ kind: "base", name: "Sink base", width: 800, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 2, drawers: [], shelves: 0, cutout: "sink" }),
  Hob: () => ({ kind: "base", name: "Hob base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [180, 300, 290], shelves: 0, cutout: "hob" }),
  Dishwasher: () => ({ kind: "dishwasher", name: "Dishwasher", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [], shelves: 0 }),
  Fridge: () => ({ kind: "appliance", name: "Fridge", width: 600, height: 2000, depth: 650, toe_kick_height: 0, doors: 0, drawers: [], shelves: 0 }),
  Filler: () => ({ kind: "filler", name: "Filler", width: 50, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [], shelves: 0 }),
  Wardrobe: () => ({ kind: "tall", name: "Wardrobe", width: 1000, height: 2300, depth: 600, toe_kick_height: 100, toe_kick_depth: 60, doors: 2, drawers: [], shelves: 0, hanging_rail: true }),
  Open: () => ({ kind: "wall", name: "Open shelf", width: 600, height: 720, depth: 320, elevation: 1450, toe_kick_height: 0, doors: 0, drawers: [], shelves: 2 }),
  "L corner": () => ({ kind: "base", name: "L corner", width: 1000, width2: 1000, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [], shelves: 1, corner: "l" }),
  "Wall L": () => ({ kind: "wall", name: "Wall L corner", width: 600, width2: 600, height: 720, depth: 320, elevation: 1450, toe_kick_height: 0, doors: 1, drawers: [], shelves: 2, corner: "l" }),
  Blind: () => ({ kind: "base", name: "Blind corner", width: 1200, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [], shelves: 1, blind_side: "right", blind_width: 600 }),
  Oven: () => ({ kind: "tall", name: "Oven housing", width: 600, height: 2200, depth: 580, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [375, 375], shelves: 1, housing_height: 590, housing_elevation: 850 }),
  Tall: () => ({ kind: "tall", name: "Tall", width: 600, height: 2200, depth: 580, toe_kick_height: 100, toe_kick_depth: 60, doors: 2, drawers: [], shelves: 5 }),
};
// ---------- islands and peninsulas: cabinets that move together ----------
const GROUPS = { Island: true, Peninsula: true };
// A multi-selection moves as one; otherwise a cabinet's group does.
const groupOf = (c) => {
  if (selSet.size && isSel(c.id)) {
    const ids = new Set(selection().flatMap((m) => (m.group ? project.cabinets.filter((o) => o.group === m.group) : [m])).map((m) => m.id));
    return project.cabinets.filter((o) => ids.has(o.id));
  }
  return c.group ? project.cabinets.filter((o) => o.group === c.group) : [c];
};
// Move/rotate c to (x, z, elevation, rotation), carrying its group along rigidly.
function placeCabinet(c, x, z, elev = c.elevation, rot = c.rotation) {
  const dr = rot - c.rotation, de = elev - c.elevation;
  const r = THREE.MathUtils.degToRad(dr), sn = Math.sin(r), cs = Math.cos(r);
  const [cx0, cz0] = [c.x, c.z]; // c moves inside the loop
  for (const m of groupOf(c)) {
    const vx = m.x - cx0, vz = m.z - cz0;
    m.x = Math.round((x + vx * cs + vz * sn) * 10) / 10;
    m.z = Math.round((z - vx * sn + vz * cs) * 10) / 10;
    m.elevation = Math.max(0, m.elevation + de);
    m.rotation = ((m.rotation + dr) % 360 + 540) % 360 - 180;
  }
}
// Show c's group at a trial placement without committing it.
function previewGroup(c, x, z, y, rot = c.rotation) {
  const dr = rot - c.rotation, r = THREE.MathUtils.degToRad(dr), sn = Math.sin(r), cs = Math.cos(r);
  for (const m of groupOf(c)) {
    const g = groups.get(m.id); if (!g) continue;
    const vx = m.x - c.x, vz = m.z - c.z;
    g.position.set(x + vx * cs + vz * sn, m.elevation + (y - c.elevation), z - vx * sn + vz * cs);
    g.rotation.y = THREE.MathUtils.degToRad(m.rotation + dr);
  }
}
function addGroup(type) {
  const n = 3, w = 600, d = 560, seat = 300;
  const groupId = gid(type.toLowerCase());
  const style = project.cabinets[0]?.door_style || "slab";
  let origin, rot, firstFree = false;
  // Peninsula: from the middle of the first wall with room for it, into the room.
  if (type === "Peninsula") {
    search: for (const wl of roomWalls(project.room)) {
      const r = (Math.atan2(-wl.normal[1], wl.normal[0]) * 180) / Math.PI;
      const fd = [Math.sin(THREE.MathUtils.degToRad(r)), Math.cos(THREE.MathUtils.degToRad(r))];
      // fd runs back along the wall (-t), so the seating side is toward +t: positions from the middle outwards
      const ts = [];
      for (let k = 0; k * 100 < wl.len; k++) ts.push(wl.len / 2 + (k % 2 ? 1 : -1) * Math.ceil(k / 2) * 100);
      const ops = project.room.openings.filter((op) => op.wall === wl.i);
      for (const tc of ts) {
        const p = wallPoint(wl, tc, 0);
        const o = [p[0] - fd[0] * (d / 2), p[1] - fd[1] * (d / 2)];
        const probe = { kind: "base", width: n * w, depth: d + seat, x: o[0] - fd[0] * seat, z: o[1] - fd[1] * seat, rotation: r };
        const f = footprint(probe);
        const [ta] = wallLocal(wl, f.x0, f.z0), [tb] = wallLocal(wl, f.x1, f.z1);
        const lo = Math.min(ta, tb), hi = Math.max(ta, tb);
        if (lo < 50 || hi > wl.len - 50) continue;
        if (ops.some((op) => op.offset < hi + 100 && op.offset + op.width > lo - 100)) continue;
        const hit = project.cabinets.some((q) => { const g = footprint(q); return g.x0 < f.x1 - 2 && g.x1 > f.x0 + 2 && g.z0 < f.z1 - 2 && g.z1 > f.z0 + 2; });
        if (!hit) { origin = o; rot = Math.round(r * 1000) / 1000; break search; }
      }
    }
  }
  if (!origin) {
    // Island (or no wall free): centred in the room.
    const ws = project.room.walls;
    const cx = ws.length ? (Math.min(...ws.map((q) => q[0])) + Math.max(...ws.map((q) => q[0]))) / 2 : 1500;
    const cz = ws.length ? (Math.min(...ws.map((q) => q[1])) + Math.max(...ws.map((q) => q[1]))) / 2 : 1500;
    origin = [cx - (n * w) / 2, cz - d / 2]; rot = 0; firstFree = true;
    // nearest free spot to the centre (with the seating strip behind), 100 mm grid
    const clear = 900; // walkway kept to walls and other cabinets
    const free = (ox, oz) => {
      const f = { x0: ox - clear, x1: ox + n * w + clear, z0: oz - seat - clear, z1: oz + d + clear };
      const inRoom = !ws.length || (ox - clear >= Math.min(...ws.map((q) => q[0])) && ox + n * w + clear <= Math.max(...ws.map((q) => q[0]))
        && oz - seat - clear >= Math.min(...ws.map((q) => q[1])) && oz + d + clear <= Math.max(...ws.map((q) => q[1])));
      return inRoom && !project.cabinets.some((q) => { const g = footprint(q); return g.x0 < f.x1 && g.x1 > f.x0 && g.z0 < f.z1 && g.z1 > f.z0; });
    };
    let best = null;
    for (let i = -20; i <= 20; i++) for (let j = -20; j <= 20; j++) {
      const ox = origin[0] + i * 100, oz = origin[1] + j * 100;
      if (free(ox, oz) && (!best || i * i + j * j < best[2])) best = [ox, oz, i * i + j * j];
    }
    if (best) origin = [best[0], best[1]];
  }
  const r = THREE.MathUtils.degToRad(rot), ux = [Math.cos(r), -Math.sin(r)];
  for (let k = 0; k < n; k++) {
    const c = {
      ...CAB_PRESETS.Base(), name: type, id: uid("b"), group: groupId,
      x: Math.round((origin[0] + ux[0] * w * k) * 10) / 10, z: Math.round((origin[1] + ux[1] * w * k) * 10) / 10,
      rotation: rot, elevation: 0, door_style: style, handles: true, back_finished: true, worktop_back: seat,
      hinge_side: k === 0 && !firstFree ? "right" : "left",
      end_left: k === 0 && firstFree, end_right: k === n - 1,
      carcass_material: "carcass", back_material: "back", front_material: "front", drawer_material: "drawer",
    };
    project.cabinets.push(c);
    if (k === 0) selected = c.id;
  }
  project = JSON.parse(cw.normalize(JSON.stringify(project)));
  recompute();
}

function addCabinet(type, preset = null) {
  const p = preset ? structuredClone(preset) : CAB_PRESETS[type]();
  const prefix = { base: "b", wall: "w", tall: "t", dishwasher: "d", appliance: "a", filler: "f" }[p.kind] || "c";
  // Append to the back-wall run after anything at the same height (a tall
  // cabinet blocks both base and wall units); match the project's front style.
  const elev = p.elevation || 0, top = elev + p.height;
  const same = project.cabinets.filter((c) => c.elevation < top && c.elevation + c.height > elev);
  const run = same.filter((c) => !c.rotation && Math.abs(c.z) < 1);
  let x = run.reduce((m, c) => Math.max(m, c.x + c.width), 0), z = 0, rotation = 0;
  // Run already reaches the right-hand wall: start on its left instead.
  const right = project.room.walls.length ? Math.max(...project.room.walls.map((q) => q[0])) : Infinity;
  if (x + p.width > right + 1 && run.length) x = Math.max(0, Math.min(...run.map((c) => c.x)) - p.width);
  // An L corner goes into the first free right-angle room corner.
  const spot = isL(p) ? freeCorner(p, same) : null;
  if (spot) [x, z, rotation] = spot;
  const style = p.door_style || project.cabinets.find((c) => c.id === selected)?.door_style || project.cabinets[0]?.door_style || "slab";
  const c = {
    id: uid(prefix), x, z, elevation: 0, rotation, hinge_side: "left", handles: true, door_style: style,
    carcass_material: "carcass", back_material: "back", front_material: "front", drawer_material: "drawer",
    toe_kick_depth: 60, ...p,
  };
  project.cabinets.push(c);
  // Fill every field the preset leaves out with the Rust defaults.
  project = JSON.parse(cw.normalize(JSON.stringify(project)));
  selected = c.id;
  recompute();
}

// Room corner (end of wall i, start of wall i + 1, at 90°) where an L
// cabinet of preset `p` overlaps none of `others`: [x, z, rotation] or null.
function freeCorner(p, others) {
  const ws = roomWalls(project.room);
  for (let i = 0; i < ws.length; i++) {
    const a = ws[i], b = ws[(i + 1) % ws.length];
    if (b.dir[0] * a.normal[0] + b.dir[1] * a.normal[1] < 0.98) continue; // inside right-angle corner only
    if (a.len < p.width || b.len < p.width2) continue;
    const [ox, oz] = wallPoint(a, a.len - p.width, 0);
    const c = { ...p, x: ox, z: oz, rotation: a.rotation };
    const f = footprint(c);
    const hit = others.some((o) => { const g = footprint(o); return g.x0 < f.x1 - 2 && g.x1 > f.x0 + 2 && g.z0 < f.z1 - 2 && g.z1 > f.z0 + 2; });
    if (!hit) return [Math.round(ox * 10) / 10, Math.round(oz * 10) / 10, a.rotation];
  }
  return null;
}

// ---------- left: add palette ----------
// Small front-elevation icons, 28 x 34 viewBox.
const ICONS = {
  Base: '<rect x="3" y="8" width="22" height="22"/><line x1="20" y1="12" x2="20" y2="17"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Drawers: '<rect x="3" y="8" width="22" height="22"/><line x1="3" y1="14" x2="25" y2="14"/><line x1="3" y1="22" x2="25" y2="22"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Sink: '<rect x="1" y="6" width="26" height="2.5" class="k"/><rect x="3" y="8.5" width="22" height="21.5"/><line x1="14" y1="8.5" x2="14" y2="30"/><path d="M10 6 v-3 h4"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Hob: '<rect x="1" y="6" width="26" height="2.5" class="k"/><rect x="3" y="8.5" width="22" height="21.5"/><line x1="3" y1="15" x2="25" y2="15"/><line x1="3" y1="22" x2="25" y2="22"/><line x1="7" y1="4" x2="21" y2="4" class="t"/><rect x="3" y="30" width="22" height="3" class="k"/>',
"L corner": '<path d="M2 8 h24 v22 h-9 v-13 h-15 z"/><line x1="17" y1="17" x2="17" y2="30" class="t"/>',
  "Wall L": '<path d="M2 3 h24 v18 h-9 v-10 h-15 z"/><line x1="0" y1="1.5" x2="28" y2="1.5" class="t"/>',
  Blind: '<rect x="2" y="8" width="24" height="22"/><rect x="15" y="8" width="11" height="22" class="k"/><line x1="12" y1="12" x2="12" y2="17"/><rect x="2" y="30" width="24" height="3" class="k"/>',
  Wall: '<rect x="3" y="3" width="22" height="16"/><line x1="20" y1="13" x2="20" y2="17"/><line x1="0" y1="2" x2="28" y2="2" class="t"/>',
  Oven: '<rect x="7" y="1" width="14" height="30"/><rect x="8.5" y="11" width="11" height="9" class="k"/><line x1="10" y1="13" x2="18" y2="13" class="t"/><line x1="7" y1="25" x2="21" y2="25"/><rect x="7" y="31" width="14" height="2" class="k"/>',
  Tall: '<rect x="7" y="1" width="14" height="30"/><line x1="7" y1="13" x2="21" y2="13"/><rect x="7" y="31" width="14" height="2" class="k"/>',
  Dishwasher: '<rect x="3" y="8" width="22" height="22"/><line x1="10" y1="11.5" x2="18" y2="11.5" class="t"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Fridge: '<rect x="6" y="3" width="16" height="30" stroke-dasharray="2.2 1.6"/><line x1="9" y1="9" x2="9" y2="14"/><line x1="9" y1="18" x2="9" y2="24"/>',
  Filler: '<rect x="11" y="8" width="6" height="22"/><rect x="11" y="30" width="6" height="3" class="k"/>',
  Island: '<rect x="2" y="9" width="24" height="16" class="k"/><rect x="4" y="11" width="20" height="8"/><circle cx="8" cy="29" r="2"/><circle cx="14" cy="29" r="2"/><circle cx="20" cy="29" r="2"/>',
  Peninsula: '<line x1="3" y1="2" x2="3" y2="32" class="t"/><rect x="3" y="10" width="22" height="14" class="k"/><rect x="3" y="12" width="20" height="8"/><circle cx="10" cy="28" r="2"/><circle cx="17" cy="28" r="2"/>',
  Wardrobe: '<rect x="3" y="1" width="22" height="30"/><line x1="3" y1="7" x2="25" y2="7"/><line x1="6" y1="10" x2="22" y2="10" class="t"/><path d="M14 10 v2 l-4 4 h8 l-4 -4" fill="none"/><rect x="3" y="31" width="22" height="2" class="k"/>',
  Open: '<rect x="3" y="3" width="22" height="16"/><line x1="3" y1="8.5" x2="25" y2="8.5"/><line x1="3" y1="14" x2="25" y2="14"/><line x1="0" y1="2" x2="28" y2="2" class="t"/>',
};
// Presets: whole cabinets saved by the user, shown under "My cabinets".
const PLACEMENT = ["id", "x", "z", "rotation"];
function savePreset(c) {
  const cab = Object.fromEntries(Object.entries(c).filter(([k]) => !PLACEMENT.includes(k)));
  const list = lsGet(PRESETS, []).filter((p) => p.name !== c.name);
  list.push({ name: c.name, cab });
  toast(lsSet(PRESETS, list) ? `Preset "${c.name}" saved` : "Browser storage unavailable");
  renderPresets();
}
function renderPresets() {
  const el = $("#presets"); el.replaceChildren();
  const list = lsGet(PRESETS, []);
  if (!list.length) return;
  el.append(h("h3", {}, "My cabinets"));
  for (const pr of list) {
    el.append(h("div", { class: "preset" },
      h("button", { class: "grow", onclick: () => { addCabinet(null, pr.cab); closeSheetOnPhone(); } }, pr.name),
      h("button", { title: "Remove preset", onclick: () => { lsSet(PRESETS, lsGet(PRESETS, []).filter((x) => x.name !== pr.name)); renderPresets(); } }, "×")));
  }
}
function renderPalette() {
  renderPresets();
  const el = $("#palette");
  for (const [type, body] of Object.entries(ICONS)) {
    const grp = GROUPS[type];
    const b = h("button", { title: `Add ${type.toLowerCase()}${grp ? "" : " cabinet"}`, "data-add": type, onclick: () => { grp ? addGroup(type) : addCabinet(type); closeSheetOnPhone(); } });
    b.innerHTML = `<svg width="28" height="34" viewBox="0 0 28 34" fill="#fff" stroke="currentColor" stroke-width="1.3"><style>.k{fill:currentColor;opacity:.35;stroke:none}.t{stroke-width:2.2}</style>${body}</svg>`;
    b.append(type);
    el.append(b);
  }
}

// ---------- left: list ----------
function renderList() {
  const ul = $("#list"); ul.replaceChildren();
  for (const c of project.cabinets) {
    ul.append(h("li", { class: isSel(c.id) ? "sel" : "", onclick: (e) => { if (e.shiftKey && selected) toggleSel(c.id); else { selected = c.id; selWall = null; closeSheetOnPhone(); } recompute(); } },
      h("b", {}, c.id), h("span", {}, c.name), h("em", {}, `${c.width}${isL(c) ? "/" + c.width2 : ""}×${c.height}×${c.depth}`)));
  }
}

// ---------- right: properties ----------
const KINDS = [["base", "Base"], ["wall", "Wall"], ["tall", "Tall"], ["dishwasher", "Dishwasher"], ["appliance", "Appliance space"], ["filler", "Filler"]];
function field(label, obj, key, type = "number", opts) {
  let input;
  const commit = () => {
    let v = input.value;
    if (type === "number") { v = parseFloat(v); if (!isFinite(v)) return; }
    if (type === "check") v = input.checked;
    if (type === "list") v = input.value.split(/[ ,;]+/).map(Number).filter((n) => n > 0);
    if (type === "int") v = Math.max(0, parseInt(v) || 0);
    if (type === "select" && typeof opts[0][0] === "number") v = Number(v);
    obj[key] = v; recompute();
  };
  if (type === "select") {
    input = h("select", { onchange: commit }, opts.map(([v, l]) => h("option", { value: v, selected: obj[key] === v }, l)));
  } else if (type === "check") {
    input = h("input", { type: "checkbox", checked: !!obj[key], onchange: commit });
  } else {
    const val = type === "list" ? (obj[key] || []).join(", ") : obj[key] ?? "";
    input = h("input", { type: type === "number" || type === "int" ? "number" : "text", value: val, step: "any", onchange: commit });
  }
  return h("label", { class: type === "check" ? "chk" : "" }, h("span", {}, label), input);
}

function renderProps() {
  const el = $("#props"); el.replaceChildren();
  // Conditional fields pass null; leave them out.
  const put = (...kids) => el.append(...kids.flat().filter((k) => k != null && k !== false));
  if (selWall != null && wallOf(project.room, selWall)) return renderWallPanel(put);
  const c = project.cabinets.find((c) => c.id === selected);
  const mats = project.materials.map((m) => [m.id, m.name]);
  if (c && selSet.size) return renderMultiPanel(put);
  if (!c) {
    const s = project.settings;
    put(
      h("h3", {}, "Project"),
      field("Name", project, "name", "text"),
      h("h3", {}, "Client"),
      field("Name", project.client, "name", "text"),
      h("div", { class: "grid2" }, field("Phone", project.client, "phone", "text"), field("Email", project.client, "email", "text")),
      field("Address", project.client, "address", "text"),
      h("h3", {}, "Machining"),
      h("div", { class: "grid2" },
        field("Kerf", s, "kerf"), field("Sheet trim", s, "sheet_trim"),
        field("Front gap", s, "front_gap"), field("Edge band", s, "edge_band"),
        field("System line", s, "system_line"), field("Hinge cup inset", s, "hinge_cup_inset"),
        field("Slide clearance", s, "slide_clearance"), field("Handle c/c", s, "handle_spacing"),
        field("Shelf setback", s, "shelf_setback"),
        field("Joinery", s, "joinery", "select", [["confirmat", "Confirmat"], ["dowel", "Dowel"]]),
        field("Back", s, "back_mode", "select", [["applied", "Applied"], ["grooved", "In groove"]]),
        field("Plinth", s, "plinth", "select", [["board", "Toe-kick board"], ["legs", "Legs + plinth"]]),
        field("Plinth per run", s, "plinth_runs", "check"),
        s.back_mode === "grooved" ? field("Back inset", s, "back_inset") : null,
        field("Groove depth", s, "groove_depth")),
      h("h3", {}, "Room"),
      h("div", { class: "grid2" }, field("Wall thickness", project.room, "thickness"), field("Height", project.room, "height")),
      h("div", { class: "row" },
        h("button", { onclick: startDrawing }, "Draw room"),
        h("button", { onclick: rectangleRoom }, "Rectangle"),
        project.room.walls.length ? h("button", { class: "danger", onclick: () => { project.room.walls = []; project.room.openings = []; selWall = null; recompute(); } }, "Remove") : null),
      h("h3", {}, "CNC"),
      h("div", { class: "grid2" },
        field("Pass depth", project.cnc, "pass_depth"), field("Into spoilboard", project.cnc, "through"),
        field("Safe Z", project.cnc, "safe_z"), field("Tab height", project.cnc, "tab_height"),
        field("Tab length", project.cnc, "tab_length"), field("Tabs below m²", project.cnc, "tab_area")),
      ...[["Router", project.cnc.router], ["Groove cutter", project.cnc.groove_router], ...project.cnc.drills.map((t) => [t.name, t])].map(([label, t]) =>
        h("div", { class: "tool" },
          h("div", { class: "row" }, h("b", { class: "grow" }, label)),
          h("div", { class: "grid4" },
            field("T", t, "number", "int"), field("Ø", t, "dia"), field("rpm", t, "rpm"),
            t.feed > 0 || label === "Router" || label === "Groove cutter" ? field("Feed", t, "feed") : field("Plunge", t, "plunge")))),
      h("h3", {}, "Worktop"),
      h("div", { class: "grid2" }, field("Thickness", s, "worktop_thickness"), field("Overhang", s, "worktop_overhang"), field("End overhang", s, "worktop_end_overhang")),
      h("h3", {}, "Shaker"),
      h("div", { class: "grid2" }, field("Frame width", s, "shaker_frame"), field("Groove", s, "shaker_groove"), field("Routed depth", s, "routed_depth")),
      h("div", { class: "row" }, h("h3", { class: "grow" }, "Materials"), h("button", { onclick: addMaterial }, "+ Material")),
      ...project.materials.map((m) => {
        const used = materialUsed(m.id);
        return h("div", { class: "mat" },
          h("div", { class: "grid2" },
            field("Name", m, "name", "text"), field("Colour", m, "color", "text"),
            field("Thickness", m, "thickness"), field("Grain", m, "grain", "check"),
            field("Sheet L", m, "sheet_length"), field("Sheet W", m, "sheet_width")),
          offcutField(m),
          h("div", { class: "row" }, h("span", { class: "grow mute" }, m.id),
            h("button", { class: "danger", disabled: used, title: used ? "In use" : null, onclick: () => { project.materials = project.materials.filter((x) => x !== m); recompute(); } }, "Delete")));
      }),
      h("div", { class: "row defaults" }, h("button", { onclick: saveDefaults }, "Save as shop defaults")),
    );
    return;
  }
  const carcass = ["base", "wall", "tall"].includes(c.kind);
  const lc = isL(c);
  const blind = !lc && c.blind_side && c.blind_side !== "none";
  const cut = c.cutout && c.cutout !== "none";
  put(
    h("div", { class: "row" },
      h("h3", {}, c.id),
      h("button", { onclick: () => {
        const d = structuredClone(c); d.id = uid(c.id.replace(/\d+$/, "")); d.x = c.x + c.width;
        project.cabinets.push(d); selected = d.id; recompute();
      } }, "Duplicate"),
      h("button", { title: "Rotate 90° (R)", onclick: () => rotateCabinet(c) }, "⟳"),
      h("button", { title: "Save as preset", onclick: () => savePreset(c) }, "★"),
      h("button", { class: "danger", onclick: () => {
        project.cabinets = project.cabinets.filter((x) => x !== c); selected = null; recompute();
      } }, "Delete")),
    c.group ? h("div", { class: "row" }, h("span", { class: "grow mute" }, `${c.group} · ${groupOf(c).length} cabinets`),
      h("button", { onclick: () => { for (const m of groupOf(c)) m.group = ""; recompute(); } }, "Ungroup")) : null,
    field("Name", c, "name", "text"),
    field("Type", c, "kind", "select", KINDS),
    h("div", { class: "grid3" }, field("Width", c, "width"), field("Height", c, "height"), field("Depth", c, "depth")),
    h("div", { class: "grid2" },
      field("X", c, "x"), field("Z", c, "z"),
      field("Elevation", c, "elevation"), field("Rotation°", c, "rotation"),
      c.kind !== "appliance" && c.kind !== "wall" ? [field("Toe kick H", c, "toe_kick_height"), field("Toe kick D", c, "toe_kick_depth")] : null),
    carcass ? [
      h("h3", {}, "Interior"),
      h("div", { class: "grid2" },
        field("Corner", c, "corner", "select", [["none", "None"], ["l", "L shape"]]),
        lc ? field("Return width", c, "width2") : field("Blind corner", c, "blind_side", "select", [["none", "None"], ["left", "Left"], ["right", "Right"]]),
        blind ? [field("Blind width", c, "blind_width"), h("span")] : null,
        lc ? field("Doors", c, "doors", "select", [[0, "None"], [1, "Bi-fold"]]) : field("Doors", c, "doors", "select", [[0, "None"], [1, "1"], [2, "2"]]),
        lc || c.lift_up ? null : field("Hinge", c, "hinge_side", "select", [["left", "Left"], ["right", "Right"]]),
        c.kind === "wall" && !lc ? field("Lift-up", c, "lift_up", "check") : null,
        field("Glass", c, "glass", "check"),
        field("Front style", c, "door_style", "select", [["slab", "Slab"], ["shaker", "Shaker"], ["shaker_routed", "Shaker routed"]]),
        field("Shelves", c, "shelves", "int"),
        field("Handles", c, "handles", "check"),
        field("End panel L", c, "end_left", "check"),
        field("End panel R", c, "end_right", "check"),
        lc ? null : field("Finished back", c, "back_finished", "check"),
        c.kind === "tall" ? field("Hanging rail", c, "hanging_rail", "check") : null,
        c.kind === "tall" && c.hanging_rail ? field("Hat shelf gap", c, "hat_space") : null),
      lc ? null : field("Drawers (front heights, top first)", c, "drawers", "list"),
      !lc && c.kind !== "wall" ? h("h3", {}, "Appliance opening") : null,
      !lc && c.kind !== "wall" ? h("div", { class: "grid2" }, field("Height", c, "housing_height"), c.housing_height > 0 ? field("From floor", c, "housing_elevation") : h("span")) : null,
    ] : null,
    c.kind === "dishwasher" ? h("div", { class: "grid2" },
      field("Front style", c, "door_style", "select", [["slab", "Slab"], ["shaker", "Shaker"], ["shaker_routed", "Shaker routed"]]),
      field("Handles", c, "handles", "check")) : null,
    c.kind === "base" ? [
      h("h3", {}, "Worktop"),
      h("div", { class: "grid2" },
        field("Seating overhang", c, "worktop_back"), h("span"),
        field("Cut-out", c, "cutout", "select", [["none", "None"], ["sink", "Sink"], ["hob", "Hob"]]),
        cut ? field("Front setback", c, "cutout_setback") : h("span"),
        cut ? [field("Width (0 = std)", c, "cutout_width"), field("Depth (0 = std)", c, "cutout_depth"),
          field("Corner radius", c, "cutout_radius")] : null),
    ] : null,
    c.kind !== "appliance" ? h("h3", {}, "Materials") : null,
    carcass ? [field("Carcass", c, "carcass_material", "select", mats), field("Back", c, "back_material", "select", mats)] : null,
    c.kind !== "appliance" ? field("Fronts", c, "front_material", "select", mats) : null,
    carcass ? field("Drawer box", c, "drawer_material", "select", mats) : null,
    c.kind !== "appliance" && c.kind !== "filler" && c.door_style === "shaker" ? field("Shaker panel", c, "panel_material", "select", mats) : null,
  );
}

// Stock offcuts as "L×W, L×W" (x or × or *).
function offcutField(m) {
  const show = (m.offcuts || []).map(([l, w]) => `${l}×${w}`).join(", ");
  const input = h("input", { type: "text", value: show, placeholder: "e.g. 1200×600, 800×400", onchange: () => {
    const list = input.value.split(/[,;]+/).map((t) => t.trim().split(/\s*[x×*]\s*/i).map(Number)).filter((p) => p.length === 2 && p.every((v) => v > 0));
    m.offcuts = list; recompute();
  } });
  return h("label", {}, h("span", {}, "Offcuts in stock"), input);
}
function toggleSel(id) {
  if (id === selected) {
    const next = [...selSet][0];
    if (next == null) return;
    selSet.delete(next); selected = next;
  } else if (selSet.has(id)) selSet.delete(id);
  else selSet.add(id);
  selAnchor = selected;
}
// Several cabinets selected: act on them together.
function renderMultiPanel(put) {
  const cs = selection();
  const style = new Set(cs.map((c) => c.door_style)).size === 1 ? cs[0].door_style : "";
  const grouped = cs.some((c) => c.group);
  put(
    h("div", { class: "row" }, h("h3", {}, `${cs.length} cabinets`),
      h("button", { title: "Rotate 90° (R)", onclick: () => rotateCabinet(cs.find((c) => c.id === selected)) }, "⟳"),
      h("button", { class: "danger", onclick: () => { project.cabinets = project.cabinets.filter((x) => !isSel(x.id)); selected = null; recompute(); } }, "Delete")),
    h("div", { class: "mute" }, cs.map((c) => c.id).join(" · ")),
    h("div", { class: "row" },
      h("button", { onclick: () => { const g = gid("group"); for (const c of cs) c.group = g; recompute(); } }, "Group"),
      grouped ? h("button", { onclick: () => { for (const c of cs) c.group = ""; recompute(); } }, "Ungroup") : null,
      h("button", { onclick: () => duplicateSelection(cs) }, "Duplicate")),
    h("h3", {}, "Set for all"),
    h("div", { class: "grid2" },
      h("label", {}, h("span", {}, "Front style"), h("select", { onchange: (e) => { for (const c of cs) c.door_style = e.target.value; recompute(); } },
        ...[["", "—"], ["slab", "Slab"], ["shaker", "Shaker"], ["shaker_routed", "Shaker routed"]].map(([v, t]) => h("option", { value: v, selected: v === style, disabled: v === "" }, t)))),
      h("label", {}, h("span", {}, "Fronts"), h("select", { onchange: (e) => { for (const c of cs) c.front_material = e.target.value; recompute(); } },
        h("option", { value: "", disabled: true, selected: new Set(cs.map((c) => c.front_material)).size > 1 }, "—"),
        ...project.materials.map((m) => h("option", { value: m.id, selected: cs.every((c) => c.front_material === m.id) }, m.name))))),
  );
}
// Copies placed beside the selection (to its right along x), selected.
function duplicateSelection(cs) {
  const fs = cs.map((c) => footprint(c));
  const dx = Math.max(...fs.map((f) => f.x1)) - Math.min(...fs.map((f) => f.x0));
  const groups = new Map();
  const copies = cs.map((c) => {
    const d = structuredClone(c);
    d.id = uid(c.id.replace(/\d+$/, "")); project.cabinets.push(d);
    d.x = Math.round((c.x + dx) * 10) / 10;
    if (c.group) { if (!groups.has(c.group)) groups.set(c.group, gid("group")); d.group = groups.get(c.group); }
    return d;
  });
  selected = copies[0].id; selSet.clear(); for (const d of copies.slice(1)) selSet.add(d.id); selAnchor = selected;
  recompute();
}

function renderWallPanel(put) {
  const w = wallOf(project.room, selWall), i = selWall;
  const fill = (tier) => {
    const before = project.cabinets.length;
    try { project = JSON.parse(cw.fill_wall(JSON.stringify(project), i, tier)); }
    catch (e) { toast(String(e.message || e)); return; }
    const n = project.cabinets.length - before;
    toast(n ? `${n} added` : "No free space on this wall");
    closeSheetOnPhone();
    recompute();
  };
  const lenInput = h("input", { type: "number", step: "any", value: Math.round(w.len), onchange: () => {
    const L = parseFloat(lenInput.value); if (!(L > 50)) return;
    const r = resizeWall(project.room, i, L, lenAnchor);
    moveWithWalls(project.room.walls, r.walls, project.cabinets);
    project.room.walls = r.walls; project.room.openings = r.openings;
    commitRoom();
  } });
  const ops = project.room.openings.map((o, k) => [o, k]).filter(([o]) => o.wall === i);
  const addOpening = (kind) => {
    const width = kind === "door" ? 850 : 1000;
    project.room.openings.push(kind === "door"
      ? { wall: i, kind, offset: Math.max(0, Math.round((w.len - width) / 2)), width, height: 2050, sill: 0 }
      : { wall: i, kind, offset: Math.max(0, Math.round((w.len - width) / 2)), width, height: 1200, sill: 950 });
    recompute();
  };
  put(
    h("div", { class: "row" }, h("h3", {}, `Wall ${i + 1}`),
      h("button", { onclick: () => { selWall = null; recompute(); } }, "Done")),
    h("label", {}, h("span", {}, "Length"), lenInput),
    h("label", {}, h("span", {}, "Fixed"),
      h("div", { class: "seg", id: "anchor" }, ...[["start", "Start"], ["centre", "Centre"], ["end", "End"]].map(([k, t]) =>
        h("button", { class: lenAnchor === k ? "on" : "", "data-anchor": k, onclick: () => { lenAnchor = k; recompute(); } }, t)))),
    h("h3", {}, "Fill with cabinets"),
    h("div", { class: "fill" },
      h("button", { class: "pri", onclick: () => fill("both") }, "Base + wall"),
      h("button", { onclick: () => fill("base") }, "Base"),
      h("button", { onclick: () => fill("wall") }, "Wall")),
    h("h3", {}, "Doors & windows"),
    ...ops.map(([o, k]) => h("div", { class: "mat" },
      h("div", { class: "row" },
        h("b", { class: "grow" }, o.kind === "window" ? "Window" : "Door"),
        h("button", { class: "danger", onclick: () => { project.room.openings.splice(k, 1); recompute(); } }, "Delete")),
      h("div", { class: "grid2" },
        field("From corner", o, "offset"), field("Width", o, "width"),
        field("Height", o, "height"), o.kind === "window" ? field("Sill", o, "sill") : null))),
    h("div", { class: "row" },
      h("button", { onclick: () => addOpening("door") }, "+ Door"),
      h("button", { onclick: () => addOpening("window") }, "+ Window")),
  );
}

// Room edits can flip the corner order; the Rust parser re-orients it.
function commitRoom() {
  project = JSON.parse(cw.normalize(JSON.stringify(project)));
  recompute();
}
function rectangleRoom() {
  closeSheetOnPhone();
  // Big enough for what is already placed, 3600 x 3000 at least.
  let W = 3600, D = 3000;
  for (const c of project.cabinets) { const f = footprint(c); W = Math.max(W, Math.ceil(f.x1 / 100) * 100); D = Math.max(D, Math.ceil(f.z1 / 100) * 100); }
  project.room.walls = [[0, 0], [W, 0], [W, D], [0, D]];
  project.room.openings = [];
  selWall = null; selected = null;
  if (view !== "plan") setView("plan");
  commitRoom();
}
function startDrawing() {
  closeSheetOnPhone();
  drawing = { pts: [], cursor: null };
  selected = null; selWall = null;
  if (view !== "plan") setView("plan");
  recompute();
}
function finishDrawing() {
  const d = drawing; drawing = null;
  if (d && d.pts.length >= 3) {
    project.room.walls = d.pts;
    project.room.openings = [];
    commitRoom();
  } else recompute();
}

// ---------- coordinate dial ----------
// Type a number to place the selected cabinet exactly, or +n / -n to move it
// by n. "=-n" sets a negative coordinate.
function readDial(input, current) {
  const t = input.value.trim().replace(",", ".");
  if (/^[+-]/.test(t)) { const d = parseFloat(t); return isFinite(d) ? current + d : null; }
  const v = parseFloat(t.replace(/^=/, ""));
  return isFinite(v) ? v : null;
}
function renderDial() {
  const el = $("#dial");
  const c = tab === "3d" && !drawing && project.cabinets.find((c) => c.id === selected);
  el.hidden = !c;
  if (!c) return;
  const keys = [["x", "x"], ["y", "elevation"], ["z", "z"], ["r", "rotation"]];
  for (const [k, prop] of keys) {
    const input = $(`#dial input[data-k=${k}]`);
    input.onchange = () => {
      const v = readDial(input, c[prop]);
      if (v == null) { input.value = fmtDial(c[prop]); return; }
      const next = { x: c.x, z: c.z, elevation: c.elevation, rotation: c.rotation };
      next[prop] = prop === "elevation" ? Math.max(0, v) : prop === "rotation" ? ((v % 360) + 540) % 360 - 180 : v;
      placeCabinet(c, next.x, next.z, next.elevation, next.rotation);
      recompute();
    };
  }
  showDial({ x: c.x, y: c.elevation, z: c.z, r: c.rotation });
}
const fmtDial = (v) => String(Math.round(v * 10) / 10);
function showDial(v) {
  for (const [k, val] of Object.entries(v)) {
    const input = $(`#dial input[data-k=${k}]`);
    if (input && document.activeElement !== input) input.value = fmtDial(val);
  }
}

function materialUsed(id) {
  if (id === "top" && project.settings.worktop_thickness > 0) return true;
  return project.cabinets.some((c) => [c.carcass_material, c.back_material, c.front_material, c.drawer_material, c.panel_material].includes(id));
}
function addMaterial() {
  let i = 1; while (project.materials.some((m) => m.id === "m" + i)) i++;
  project.materials.push({ id: "m" + i, name: "New material", thickness: 18, sheet_length: 2800, sheet_width: 2070, grain: false, color: "#dddddd", price: 0 });
  recompute();
}

function renderStatus() {
  const s = report.summary;
  const sheets = s.sheets.map(([n, q]) => `${q}× ${n}`).join(" · ");
  $("#status").replaceChildren(
    h("span", {}, `${s.cabinets} cabinets`), h("span", {}, `${s.parts} parts`),
    h("span", {}, `${s.holes} bores`), h("span", {}, `${s.edge_band_m} m band`), h("span", {}, sheets),
    ...report.build.warnings.map((w) => h("span", { class: "warn" }, w)),
  );
}

// ---------- tabs ----------
function renderTab() {
  $$("#tabs button[data-tab]").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  $$(".view").forEach((v) => (v.hidden = v.id !== "v-" + tab));
  if (tab === "3d") updateScene();
  if (tab === "parts") renderParts();
  if (tab === "sheets") renderSheets();
  if (tab === "hw") renderHardware();
  if (tab === "drawings") renderDrawings();
  if (tab === "quote") renderQuote();
}

function renderQuote() {
  const v = $("#v-quote"); v.replaceChildren();
  const json = JSON.stringify(project);
  const q = JSON.parse(cw.export_one(json, "quote", 0));
  const pr = project.pricing;
  const fmt = (n) => q.currency + n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  const doc = h("div", { class: "qdoc" },
    h("div", { class: "row" }, h("h2", {}, project.name),
      h("button", { class: "pri", onclick: () => {
        const pics = project.cabinets.length ? [snapshot(1600, 1000, "overview")] : [];
        saveFile(slug(project.name) + "_quote.html", cw.quote_html(json, new Date().toISOString().slice(0, 10), pics));
      } }, "Customer quote"),
      h("button", { onclick: () => saveFile(slug(project.name) + "_quote.csv", cw.export_one(json, "quote_csv", 0)) }, "CSV")),
    h("table", {},
      h("thead", {}, h("tr", {}, h("th", {}, "Item"), h("th", { class: "n" }, "Cost"), h("th", { class: "n" }, "Price"))),
      h("tbody", {}, q.items.map((it) => h("tr", { class: it.id === selected ? "sel" : "", onclick: () => {
        const cab = project.cabinets.find((c) => c.id === it.id || it.id.startsWith(c.id + "-"));
        if (cab) { selected = cab.id; recompute(); }
      } },
        h("td", {}, h("b", {}, it.name), " ", h("span", { class: "mute" }, it.description)),
        h("td", { class: "n mute" }, fmt(it.cost)), h("td", { class: "n" }, fmt(it.price))))),
      h("tfoot", {},
        h("tr", {}, h("td", { class: "n" }, "Subtotal"), h("td", { class: "n mute" }, fmt(q.cost)), h("td", { class: "n" }, fmt(q.net))),
        h("tr", {}, h("td", { class: "n" }, `VAT ${pr.vat_pct}%`), h("td"), h("td", { class: "n" }, fmt(q.vat))),
        h("tr", { class: "total" }, h("td", { class: "n" }, "Total"), h("td"), h("td", { class: "n" }, fmt(q.total))))),
    h("h3", {}, "Cost breakdown"),
    h("table", {},
      h("thead", {}, h("tr", {}, ["", "Item", "Qty", "Unit", "Total"].map((t, i) => h("th", { class: i > 1 ? "n" : "" }, t)))),
      h("tbody", {}, q.lines.map((l) => h("tr", {},
        h("td", { class: "mute" }, l.group), h("td", {}, l.item), h("td", { class: "n" }, l.qty + " " + l.unit),
        h("td", { class: "n mute" }, fmt(l.unit_price)), h("td", { class: "n" }, fmt(l.total))))),
      h("tfoot", {},
        h("tr", {}, h("td"), h("td", {}, "Cost"), h("td"), h("td"), h("td", { class: "n" }, fmt(q.cost))),
        h("tr", {}, h("td"), h("td", {}, `Markup ${pr.markup_pct}%`), h("td"), h("td"), h("td", { class: "n" }, fmt(q.markup))))),
  );
  // Prices
  const hwUnit = new Map(q.lines.filter((l) => l.group === "Hardware").map((l) => [l.item, l.unit_price]));
  const hwField = (name) => {
    const input = h("input", { type: "number", step: "any", value: pr.hardware[name] ?? hwUnit.get(name) ?? 0, onchange: () => {
      const n = parseFloat(input.value);
      if (isFinite(n)) pr.hardware[name] = n; else delete pr.hardware[name];
      recompute();
    } });
    return h("label", {}, h("span", {}, name), input);
  };
  const co = pr.company;
  const logoIn = h("input", { type: "file", accept: "image/png,image/jpeg,image/svg+xml", hidden: true, onchange: () => {
    const f = logoIn.files[0]; if (!f) return;
    if (f.size > 400000) { toast("Logo over 400 kB"); return; }
    const rd = new FileReader(); rd.onload = () => { co.logo = rd.result; recompute(); }; rd.readAsDataURL(f);
  } });
  const terms = h("textarea", { rows: 4, onchange: () => { co.terms = terms.value; recompute(); } }, co.terms);
  const prices = h("div", { class: "qprices" },
    h("h3", {}, "Company"),
    field("Name", co, "name", "text"),
    h("div", { class: "grid2" }, field("Phone", co, "phone", "text"), field("Email", co, "email", "text")),
    field("Address", co, "address", "text"),
    h("div", { class: "row logo" }, co.logo ? h("img", { src: co.logo, alt: "" }) : h("span", { class: "grow mute" }, "No logo"),
      h("button", { onclick: () => logoIn.click() }, co.logo ? "Change logo" : "Add logo"),
      co.logo ? h("button", { onclick: () => { co.logo = ""; recompute(); } }, "×") : null, logoIn),
    h("label", {}, h("span", {}, "Terms"), terms),
    field("Valid days", co, "valid_days", "int"),
    h("h3", {}, "Rates"),
    h("div", { class: "grid2" },
      field("Currency", pr, "currency", "text"), field("Markup %", pr, "markup_pct"),
      field("VAT %", pr, "vat_pct"), field("Labour / h", pr, "labor_rate"),
      field("CNC min / sheet", pr, "cnc_minutes_per_sheet"), field("Edge band / m", pr, "edge_band"),
      field("Assembly h / cab", pr, "assembly_hours"), field("Install h / cab", pr, "install_hours")),
    h("h3", {}, "Sheet prices"),
    ...project.materials.map((m) => field(m.name, m, "price")),
    h("h3", {}, "Hardware"),
    ...[...hwUnit.keys()].map(hwField));
  v.append(doc, prices);
}

function renderDrawings() {
  const v = $("#v-drawings"); v.replaceChildren();
  const json = JSON.stringify(project);
  // Plan and wall elevations first (or only the selected wall's); none when a cabinet is selected.
  if (!selected && selWall == null && project.room.walls.length >= 3) {
    const svg = cw.export_one(json, "plan", 0);
    v.append(h("figure", { class: "wide" },
      h("img", { src: "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg) }),
      h("figcaption", {}, h("span", {}, "Plan"), h("button", { onclick: () => saveFile("plan.svg", svg) }, "SVG"))));
  }
  if (!selected) project.room.walls.forEach((_, i) => {
    if (selWall != null && selWall !== i) return;
    const svg = cw.export_one(json, "wall", i);
    v.append(h("figure", { class: "wide" },
      h("img", { src: "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg) }),
      h("figcaption", {}, h("span", {}, `Wall ${i + 1}`),
        h("button", { onclick: () => saveFile(`wall_${i + 1}.svg`, svg) }, "SVG"))));
  });
  if (selWall != null) return;
  project.cabinets.forEach((c, i) => {
    if (selected && c.id !== selected) return;
    if (!["base", "wall", "tall"].includes(c.kind)) return;
    const svg = cw.export_one(json, "drawing", i);
    v.append(h("figure", {},
      h("img", { src: "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg) }),
      h("figcaption", {}, h("span", {}, `${c.id} · ${c.name}`),
        h("button", { onclick: () => saveFile(`${slug(c.id)}_drawing.svg`, svg) }, "SVG"))));
  });
}

const matOf = (id) => project.materials.find((m) => m.id === id) || { name: id, color: "#ccc", grain: false };

function partSvg(p, size = 260) {
  const pad = 12, s = (size - 2 * pad) / Math.max(p.w, p.h);
  const W = p.w * s + 2 * pad, H = p.h * s + 2 * pad;
  const X = (x) => pad + x * s, Y = (y) => H - pad - y * s;
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`); svg.setAttribute("width", W); svg.setAttribute("height", H);
  const poly = document.createElementNS(ns, "polygon");
  poly.setAttribute("points", p.outline.map(([x, y]) => `${X(x)},${Y(y)}`).join(" "));
  poly.setAttribute("fill", matOf(p.material).color); poly.setAttribute("stroke", "#222");
  svg.append(poly);
  const e = p.edges, seg = [[0, 0, p.w, 0], [p.w, 0, p.w, p.h], [0, p.h, p.w, p.h], [0, 0, 0, p.h]];
  seg.forEach((g, i) => { if (!e[i]) return;
    const l = document.createElementNS(ns, "line");
    l.setAttribute("x1", X(g[0])); l.setAttribute("y1", Y(g[1])); l.setAttribute("x2", X(g[2])); l.setAttribute("y2", Y(g[3]));
    l.setAttribute("stroke", "#e0662a"); l.setAttribute("stroke-width", 3); svg.append(l); });
  for (const i of p.band_segs || []) {
    const [a, b] = [p.outline[i], p.outline[(i + 1) % p.outline.length]];
    const l = document.createElementNS(ns, "line");
    l.setAttribute("x1", X(a[0])); l.setAttribute("y1", Y(a[1])); l.setAttribute("x2", X(b[0])); l.setAttribute("y2", Y(b[1]));
    l.setAttribute("stroke", "#e0662a"); l.setAttribute("stroke-width", 3); svg.append(l);
  }
  for (const c of p.cutouts || []) {
    const r = document.createElementNS(ns, "rect");
    r.setAttribute("x", X(c.x)); r.setAttribute("y", Y(c.y + c.h));
    r.setAttribute("width", c.w * s); r.setAttribute("height", c.h * s);
    r.setAttribute("rx", c.r * s); r.setAttribute("fill", "#fff"); r.setAttribute("stroke", "#222");
    svg.append(r);
  }
  for (const k of p.pockets || []) {
    const r = document.createElementNS(ns, "rect");
    r.setAttribute("x", X(k.x)); r.setAttribute("y", Y(k.y + k.h));
    r.setAttribute("width", Math.max(1, k.w * s)); r.setAttribute("height", Math.max(1, k.h * s));
    r.setAttribute("rx", k.r * s); r.setAttribute("fill", "rgba(0,0,0,.12)"); r.setAttribute("stroke", "#555");
    if (k.face === 2) r.setAttribute("stroke-dasharray", "4 3");
    svg.append(r);
  }
  for (const o of p.holes) {
    if (o.edge != null) {
      const [dx, dy] = [[0, 1], [-1, 0], [0, -1], [1, 0]][o.edge];
      const l = document.createElementNS(ns, "line");
      l.setAttribute("x1", X(o.x)); l.setAttribute("y1", Y(o.y));
      l.setAttribute("x2", X(o.x + dx * o.depth)); l.setAttribute("y2", Y(o.y + dy * o.depth));
      l.setAttribute("stroke", "#d07a00"); l.setAttribute("stroke-width", Math.max(2, o.dia * s));
      svg.append(l);
      continue;
    }
    const c = document.createElementNS(ns, "circle");
    c.setAttribute("cx", X(o.x)); c.setAttribute("cy", Y(o.y)); c.setAttribute("r", Math.max(1.2, (o.dia / 2) * s));
    c.setAttribute("fill", "none"); c.setAttribute("stroke", o.depth > 0 ? "#1667c4" : "#c42020");
    svg.append(c);
  }
  return svg;
}

function renderParts() {
  const v = $("#v-parts"); v.replaceChildren();
  const parts = report.build.parts.map((p, i) => ({ ...p, i }));
  const list = selected ? parts.filter((p) => p.cabinet === selected) : parts;
  const preview = h("div", { class: "preview" });
  const show = (p) => {
    $$("tr.sel", v).forEach((r) => r.classList.remove("sel"));
    $(`tr[data-i="${p.i}"]`, v)?.classList.add("sel");
    preview.replaceChildren(
      h("div", { class: "row" }, h("h3", {}, `${p.cabinet} · ${p.name}`),
        h("button", { onclick: () => saveFile(`${slug(p.id)}.dxf`, cw.export_one(JSON.stringify(project), "part_dxf", p.i)) }, "DXF")),
      partSvg(p, 300),
      h("dl", {},
        h("dt", {}, "Finished"), h("dd", {}, `${p.w} × ${p.h} × ${p.thickness}`),
        h("dt", {}, "Material"), h("dd", {}, matOf(p.material).name),
        h("dt", {}, "Bores"), h("dd", {}, summarizeHoles(p.holes)),
      ...((p.pockets || []).length ? [h("dt", {}, "Pockets"), h("dd", {}, p.pockets.map((k) => `${k.w}×${k.h}×${k.depth}${k.face === 2 ? " (face 2)" : ""}`).join(", "))] : []),
      ...(p.cutouts || []).flatMap((c) => [h("dt", {}, c.kind === "hob" ? "Hob" : "Sink"),
        h("dd", {}, `${c.w} × ${c.h} R${c.r} at ${c.x}, ${c.y}`)])));
  };
  const tbody = h("tbody", {}, list.map((p) => h("tr", { "data-i": p.i, onclick: () => show(p) },
    h("td", {}, p.cabinet), h("td", {}, p.name), h("td", {}, matOf(p.material).name),
    h("td", { class: "n" }, p.w), h("td", { class: "n" }, p.h), h("td", { class: "n" }, p.thickness),
    h("td", {}, ["B", "R", "T", "L"].filter((_, k) => p.edges[k]).join("")),
    h("td", { class: "n" }, p.holes.length || ""))));
  v.append(
    h("div", { class: "tablewrap" }, h("table", {},
      h("thead", {}, h("tr", {}, ["Cab", "Part", "Material", "W", "H", "T", "Band", "Bores"].map((t) => h("th", {}, t)))),
      tbody)),
    preview);
  if (list[0]) show(list[0]);
}

function summarizeHoles(holes) {
  const m = new Map();
  for (const o of holes) { const k = `Ø${o.dia} ${o.edge != null ? "edge " + o.depth : o.depth > 0 ? o.depth + " deep" : "thru"}`; m.set(k, (m.get(k) || 0) + 1); }
  return [...m].map(([k, n]) => `${n}× ${k}`).join(", ") || "—";
}

function renderSheets() {
  const v = $("#v-sheets"); v.replaceChildren();
  const json = JSON.stringify(project);
  report.nesting.sheets.forEach((s, i) => {
    const svg = cw.export_one(json, "sheet_svg", i);
    const img = h("img", { src: "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg), style: `aspect-ratio:${s.length + 40}/${s.width + 80}` });
    const name = `${String(i + 1).padStart(2, "0")}_${slug(s.material)}`;
    v.append(h("figure", {}, img,
      h("figcaption", {},
        h("span", {}, `#${i + 1} ${matOf(s.material).name}${s.offcut ? ` · offcut ${s.length}×${s.width}` : ""} · ${s.placements.length} parts · ${s.yield_pct}%`),
        h("button", { onclick: () => saveFile(name + ".dxf", cw.export_one(json, "sheet_dxf", i)) }, "DXF"),
        h("button", { onclick: () => saveFile(name + ".nc", cw.export_one(json, "sheet_nc", i)) }, "NC"),
        h("button", { onclick: () => saveFile(name + ".svg", svg) }, "SVG"))));
  });
  if (report.nesting.oversize.length)
    v.append(h("p", { class: "warn" }, "Oversize: " + report.nesting.oversize.map((i) => report.build.parts[i].id).join(", ")));
}

function renderHardware() {
  const v = $("#v-hw"); v.replaceChildren();
  const tot = new Map();
  for (const x of report.build.hardware) tot.set(x.name, (tot.get(x.name) || 0) + x.qty);
  v.append(
    h("table", { class: "hw" },
      h("thead", {}, h("tr", {}, h("th", {}, "Item"), h("th", {}, "Order code"), h("th", {}, "Qty"))),
      h("tbody", {}, [...tot].map(([n, q]) => {
        const codes = project.pricing.codes;
        const input = h("input", { class: "code", value: codes[n] || "", placeholder: "—", onchange: () => {
          const v = input.value.trim(); if (v) codes[n] = v; else delete codes[n]; recompute();
        } });
        return h("tr", {}, h("td", {}, n), h("td", {}, input), h("td", { class: "n" }, q));
      }))),
    h("table", { class: "hw" },
      h("thead", {}, h("tr", {}, h("th", {}, "Cab"), h("th", {}, "Item"), h("th", {}, "Qty"))),
      h("tbody", {}, report.build.hardware.map((x) =>
        h("tr", {}, h("td", {}, x.cabinet), h("td", {}, x.name), h("td", { class: "n" }, x.qty))))));
}

// ---------- 3D ----------
let renderer, scene, camera, controls, root, framed = false;
let persp, ortho, view = "3d"; // "3d" | "plan" | "front"
let roomGroup, previewLine, aimAt, floorAt, vdrag = null;
let grid;
let gizmo, gizmoProxy; // TransformControls on a handle at the selected cabinet's bottom-front centre
const overlayItems = []; // { el, pos: Vector3, kind: "label" | "handle" }
const groups = new Map();
let drag = null;
const matCache = new Map();
function setup3d() {
  const host = $("#v-3d");
  renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  host.append(renderer.domElement);
  scene = new THREE.Scene();
  scene.background = new THREE.Color("#f3f1ec");
  persp = camera = new THREE.PerspectiveCamera(40, 1, 10, 50000);
  ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, -100000, 100000);
  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  scene.add(new THREE.HemisphereLight(0xffffff, 0x8a8478, 1.6));
  const sun = new THREE.DirectionalLight(0xffffff, 1.6); sun.position.set(-2000, 4000, 5000); scene.add(sun);
  grid = new THREE.GridHelper(10000, 100, 0xcfcac0, 0xe2ded6); scene.add(grid);
  root = new THREE.Group(); scene.add(root);
  roomGroup = new THREE.Group(); scene.add(roomGroup);
  previewLine = new THREE.Line(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: 0x2f6fe4 }));
  previewLine.frustumCulled = false;
  scene.add(previewLine);
  // Move gizmo: along the cabinet's own width / height / depth, 10 mm steps.
  gizmo = new TransformControls(camera, renderer.domElement);
  gizmo.setSpace("local");
  gizmo.setTranslationSnap(10);
  gizmo.setSize(0.8);
  scene.add(gizmo.getHelper());
  gizmoProxy = new THREE.Object3D();
  scene.add(gizmoProxy);
  // Cabinet origin (back-left) from the handle's position.
  const originFromProxy = (c) => {
    const r = THREE.MathUtils.degToRad(c.rotation), sn = Math.sin(r), cs = Math.cos(r);
    const lx = c.width / 2, lz = c.depth; // handle in cabinet-local coordinates
    return [gizmoProxy.position.x - (lx * cs + lz * sn), gizmoProxy.position.z - (-lx * sn + lz * cs)];
  };
  gizmo.addEventListener("dragging-changed", (e) => {
    controls.enabled = !e.value;
    if (e.value) return;
    const c = project.cabinets.find((c) => c.id === selected);
    if (!c) return;
    let [x, z] = originFromProxy(c);
    // magnet: a back left within 40 mm of a wall it faces away from goes flush
    for (const w of roomWalls(project.room)) {
      if (Math.abs(((c.rotation - w.rotation + 540) % 360) - 180) > 0.5) continue;
      const [t, sd] = wallLocal(w, x, z);
      if (Math.abs(sd) > 0.05 && Math.abs(sd) < 40 && t > -c.width && t < w.len) { x -= w.normal[0] * sd; z -= w.normal[1] * sd; toast(`Snapped to wall ${w.i + 1}`); break; }
    }
    placeCabinet(c, Math.round(x * 10) / 10, Math.round(z * 10) / 10, Math.max(0, Math.round(gizmoProxy.position.y * 10) / 10));
    recompute();
  });
  gizmo.addEventListener("objectChange", () => {
    const c = project.cabinets.find((c) => c.id === selected), g = c && groups.get(c.id);
    if (!g) return;
    if (gizmoProxy.position.y < 0) gizmoProxy.position.y = 0;
    const [x, z] = originFromProxy(c);
    previewGroup(c, x, z, gizmoProxy.position.y);
    showDial({ x, y: gizmoProxy.position.y, z });
  });
  new ResizeObserver(resize).observe(host);
  const ray = new THREE.Raycaster(), ptr = new THREE.Vector2();
  const floor = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  const wallPlane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  const el = renderer.domElement;
  const aim = (e) => {
    const r = el.getBoundingClientRect();
    ptr.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ptr, camera);
  };
  // In the front view the floor is edge-on, so drag along a vertical plane (x only).
  const floorPoint = () => ray.ray.intersectPlane(view === "front" ? wallPlane : floor, new THREE.Vector3());
  aimAt = aim;
  // Snapped corner under the pointer (ray already aimed); near the first corner it closes the room.
  const drawCursor = () => {
    const p = floorAt(); if (!p) return;
    const first = drawing.pts[0], last = drawing.pts.at(-1);
    let q = snapCorner(last, p.x, p.z);
    // a finger is less precise than a mouse: bigger closing radius on touch screens
    const close = phone.matches ? 350 : 150;
    if (first && drawing.pts.length >= 3 && Math.hypot(q[0] - first[0], q[1] - first[1]) < close) q = first;
    drawing.cursor = q;
    updatePreview();
    $("#coords").textContent = last ? `${Math.round(Math.hypot(q[0] - last[0], q[1] - last[1]))}` : `${q[0]}, ${q[1]}`;
  };
  floorAt = () => ray.ray.intersectPlane(floor, new THREE.Vector3());
  const shown = (o) => { for (; o; o = o.parent) if (!o.visible) return false; return true; };
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    if (gizmo.axis) { drag = null; return; } // the gizmo handles this drag
    aim(e);
    if (drawing) {
      // Touch has no hover: take the corner from where the finger lands.
      drawCursor();
      drag = { sx: e.clientX, sy: e.clientY, drawing: true };
      return;
    }
    const hit = ray.intersectObjects([...root.children, ...roomGroup.children], true)
      .find((x) => shown(x.object) && (x.object.userData.cab || x.object.userData.wall != null));
    const c = hit?.object.userData.cab && project.cabinets.find((c) => c.id === hit.object.userData.cab);
    const p = c && floorPoint();
    drag = { sx: e.clientX, sy: e.clientY, c, wall: hit?.object.userData.wall, moved: false, off: p ? [c.x - p.x, c.z - p.z] : null };
  });
  el.addEventListener("dblclick", () => { if (drawing && drawing.pts.length >= 3) finishDrawing(); });
  el.addEventListener("pointermove", (e) => {
    if (drawing) { aim(e); drawCursor(); return; }
    if (!drag?.c || !drag.off) return;
    if (!drag.moved && Math.hypot(e.clientX - drag.sx, e.clientY - drag.sy) < 5) return;
    if (!drag.moved) { drag.moved = true; controls.enabled = false; el.setPointerCapture(e.pointerId); }
    aim(e);
    const p = floorPoint(); if (!p) return;
    let [x, z, rot] = drag.c.group
      ? [Math.round((p.x + drag.off[0]) / 10) * 10, Math.round((p.z + drag.off[1]) / 10) * 10, drag.c.rotation]
      : snap(drag.c, p.x + drag.off[0], view === "front" ? drag.c.z : p.z + drag.off[1]);
    if (view === "front") { z = drag.c.z; rot = drag.c.rotation; }
    drag.x = x; drag.z = z; drag.rot = rot;
    previewGroup(drag.c, x, z, drag.c.elevation, rot);
    showSnapCue(drag.c.group ? null : wallSnap);
    $("#coords").textContent = `${drag.c.id}  x ${x}  z ${z}`;
  });
  el.addEventListener("pointerup", (e) => {
    const d = drag; drag = null;
    controls.enabled = true;
    if (d?.moved) showSnapCue(null);
    $("#coords").textContent = "";
    if (!d) return;
    if (d.drawing) {
      if (Math.hypot(e.clientX - d.sx, e.clientY - d.sy) > 4 || !drawing?.cursor) return;
      const q = drawing.cursor, first = drawing.pts[0];
      if (first && drawing.pts.length >= 3 && q === first) { finishDrawing(); return; }
      const last = drawing.pts.at(-1);
      if (!last || Math.hypot(q[0] - last[0], q[1] - last[1]) > 5) drawing.pts.push(q);
      renderPlanTools(); updatePreview();
      return;
    }
    if (d.moved && d.x != null) {
      placeCabinet(d.c, d.x, d.z, d.c.elevation, d.rot ?? d.c.rotation);
      if (!isSel(d.c.id)) selected = d.c.id;
      selWall = null; recompute(); return;
    }
    if (Math.hypot(e.clientX - d.sx, e.clientY - d.sy) > 4) return;
    if (!d.c && d.wall == null) closeSheetOnPhone();
    if (d.c && e.shiftKey && selected) { toggleSel(d.c.id); selWall = null; recompute(); return; }
    selected = d.c ? d.c.id : null;
    selWall = !d.c && d.wall != null ? d.wall : null;
    recompute();
  });
  // Corner handles: drag to reshape the room (plan view).
  window.addEventListener("pointermove", (e) => {
    if (!vdrag) return;
    aim(e);
    const p = floorAt(); if (!p) return;
    const pts = project.room.walls, n = pts.length, i = vdrag.i;
    let x = Math.round(p.x / 10) * 10, z = Math.round(p.z / 10) * 10;
    for (const q of [pts[(i + n - 1) % n], pts[(i + 1) % n]]) {
      if (Math.abs(x - q[0]) < 60) x = q[0];
      if (Math.abs(z - q[1]) < 60) z = q[1];
    }
    pts[i] = [x, z];
    buildRoom();
  });
  window.addEventListener("pointerup", () => {
    if (!vdrag) return;
    moveWithWalls(vdrag.old, project.room.walls, project.cabinets);
    vdrag = null; controls.enabled = true; commitRoom();
  });
  (function loop() {
    requestAnimationFrame(loop);
    controls.update();
    hideNearWalls();
    placeOverlay();
    renderer.render(scene, camera);
  })();
}

// In 3D, walls between the camera and the room are hidden so the room stays visible.
function hideNearWalls() {
  for (const g of roomGroup.children) {
    const w = g.userData.w; if (!w) continue;
    if (view !== "3d") { g.visible = true; continue; }
    const mx = w.start[0] + (w.dir[0] * w.len) / 2, mz = w.start[1] + (w.dir[1] * w.len) / 2;
    g.visible = (camera.position.x - mx) * w.normal[0] + (camera.position.z - mz) * w.normal[1] > 0;
  }
}

// The room being drawn is an SVG over the canvas, so it is never hidden by cabinets or walls.
function updatePreview() {
  previewLine.visible = false;
  placeOverlay();
}
function drawPreview(toScreen) {
  const svg = $("#drawsvg");
  if (!svg) return;
  if (!drawing) { if (svg.innerHTML) svg.innerHTML = ""; return; }
  const pts = drawing.pts.map(([x, z]) => toScreen(x, z));
  const cur = drawing.cursor && toScreen(drawing.cursor[0], drawing.cursor[1]);
  const all = cur ? [...pts, cur] : pts;
  const closing = drawing.cursor && drawing.pts.length >= 3 && drawing.cursor === drawing.pts[0];
  let out = "";
  if (all.length > 1) out += `<polyline points="${all.map((p) => `${p.x},${p.y}`).join(" ")}" fill="none" stroke="#2f6fe4" stroke-width="2.5"/>`;
  pts.forEach((p, i) => { out += `<circle cx="${p.x}" cy="${p.y}" r="${i === 0 ? (closing ? 9 : 6) : 4}" fill="${i === 0 ? "#fff" : "#2f6fe4"}" stroke="#2f6fe4" stroke-width="2"/>`; });
  if (cur && pts.length) {
    const last = drawing.pts.at(-1), L = Math.round(Math.hypot(drawing.cursor[0] - last[0], drawing.cursor[1] - last[1]));
    out += `<g transform="translate(${cur.x + 12},${cur.y - 12})"><rect x="0" y="-15" width="${String(L).length * 8 + 12}" height="20" rx="4" fill="#2f6fe4"/><text x="6" y="0" fill="#fff" font-size="12" font-family="system-ui">${L}</text></g>`;
  } else if (cur) {
    out += `<circle cx="${cur.x}" cy="${cur.y}" r="4" fill="#2f6fe4"/>`;
  }
  svg.innerHTML = out;
}

// Wall-length labels and corner handles, positioned over the canvas each frame.
function placeOverlay() {
  const host = $("#v-3d"), W = host.clientWidth, H = host.clientHeight, v = new THREE.Vector3();
  drawPreview((x, z) => { v.set(x, 0, z).project(camera); return { x: ((v.x + 1) / 2) * W, y: ((1 - v.y) / 2) * H }; });
  for (const it of overlayItems) {
    v.copy(it.pos).project(camera);
    const on = (it.always || view === "plan") && !drawing && v.z < 1;
    it.el.hidden = !on;
    if (!on) continue;
    it.el.style.transform = `translate(${((v.x + 1) / 2) * W}px, ${((1 - v.y) / 2) * H}px) translate(-50%, -50%)`;
  }
}

function buildRoom() {
  roomGroup.clear();
  const ov = $("#overlay"); ov.replaceChildren(); overlayItems.length = 0;
  const room = project.room;
  const box = new THREE.BoxGeometry(1, 1, 1);
  const th = room.thickness || 100;
  for (const w of roomWalls(room)) {
    const g = new THREE.Group();
    g.userData.w = w;
    for (const pc of wallPieces(room, w)) {
      // extend solid pieces at the wall ends so corners close
      const t0 = pc.t0 === 0 && !pc.glass ? -th : pc.t0, t1 = pc.t1 === w.len && !pc.glass ? w.len + th : pc.t1;
      const len = t1 - t0, depth = pc.glass ? 8 : th, h = pc.y1 - pc.y0;
      if (len <= 0 || h <= 0) continue;
      const m = new THREE.Mesh(box, pc.glass ? glassMat : w.i === selWall ? wallSelMat : wallMat);
      const [cx, cz] = wallPoint(w, (t0 + t1) / 2, -depth / 2);
      m.scale.set(len, h, depth);
      m.position.set(cx, pc.y0 + h / 2, cz);
      m.rotation.y = THREE.MathUtils.degToRad(w.rotation);
      m.userData.wall = w.i;
      g.add(m);
    }
    roomGroup.add(g);
    // length label just outside the wall
    const [lx, lz] = wallPoint(w, w.len / 2, -th - 160);
    const lab = h("span", { class: "wlabel" + (w.i === selWall ? " on" : ""), onclick: () => {
      selWall = w.i; selected = null; recompute();
      if (phone.matches && !document.body.classList.contains("sheet-right")) openSheet("right");
    } }, String(Math.round(w.len)));
    ov.append(lab); overlayItems.push({ el: lab, pos: new THREE.Vector3(lx, 0, lz) });
    if (w.i === selWall && lenAnchor === "centre") {
      const [mx, mz] = wallPoint(w, w.len / 2, 0);
      const dot = h("span", { class: "vhandle fixed mid" });
      ov.append(dot); overlayItems.push({ el: dot, pos: new THREE.Vector3(mx, 0, mz) });
    }
  }
  room.walls.forEach(([x, z], i) => {
    // the selected wall's fixed corner(s) for a typed length are filled
    const n = room.walls.length;
    const fixed = selWall != null && ((lenAnchor === "start" && i === selWall) || (lenAnchor === "end" && i === (selWall + 1) % n));
    const hd = h("span", { class: "vhandle" + (fixed ? " fixed" : ""), title: "Drag corner" });
    hd.addEventListener("pointerdown", (e) => { e.stopPropagation(); e.preventDefault(); vdrag = { i, old: structuredClone(room.walls) }; controls.enabled = false; });
    ov.append(hd); overlayItems.push({ el: hd, pos: new THREE.Vector3(x, 0, z) });
  });
}
// Cabinet-local (x, z) -> world, same convention as the Rust core and three.js.
function toWorld(c, x, z, ox = c.x, oz = c.z, rot = c.rotation) {
  const r = THREE.MathUtils.degToRad(rot), sn = Math.sin(r), cs = Math.cos(r);
  return [ox + x * cs + z * sn, oz - x * sn + z * cs];
}
const isL = (c) => c.corner === "l" && ["base", "wall", "tall"].includes(c.kind);
function footprint(c, ox = c.x, oz = c.z, rot = c.rotation) {
  const d2 = isL(c) ? Math.max(c.width2, c.depth) : c.depth;
  const pts = [[0, 0], [c.width, 0], [0, d2], [c.width, d2]].map(([x, z]) => toWorld(c, x, z, ox, oz, rot));
  const xs = pts.map((p) => p[0]), zs = pts.map((p) => p[1]);
  return { x0: Math.min(...xs), x1: Math.max(...xs), z0: Math.min(...zs), z1: Math.max(...zs) };
}
// Near a wall: turn to face the room, back against the wall, and slide
// along it snapping to the wall ends and to cabinets on the same wall.
// Otherwise: 10 mm grid and footprint edges to other cabinets within 40 mm.
// Returns [x, z, rotation].
let wallSnap = null; // { w, t0, c, hits: [t...] } from the last snap onto a wall, for the on-screen cue
const SNAP_REACH = 400; // how far (beyond half the depth) a dragged cabinet is pulled onto a wall
function snap(c, x, z) {
  const ws = roomWalls(project.room);
  wallSnap = null;
  if (ws.length) {
    const ctr = toWorld(c, c.width / 2, c.depth / 2, x, z);
    let best = null;
    for (const w of ws) {
      const [t, s] = wallLocal(w, ctr[0], ctr[1]);
      if (s < 0 || s > c.depth / 2 + SNAP_REACH || t < -c.width / 2 || t > w.len + c.width / 2) continue;
      if (!best || s < best.s) best = { w, t, s };
    }
    if (best) {
      const w = best.w;
      let t0 = Math.round((best.t - c.width / 2) / 10) * 10;
      const ends = [0, w.len], solid = []; // solid: same-height neighbours on this wall
      const group = new Set(groupOf(c));
      for (const o of project.cabinets) {
        if (group.has(o)) continue;
        const [ot, os] = wallLocal(w, o.x, o.z);
        if (Math.abs(os) < 2 && Math.abs(((o.rotation - w.rotation + 540) % 360) - 180) < 0.5) {
          ends.push(ot, ot + o.width);
          if (o.elevation < c.elevation + c.height && o.elevation + o.height > c.elevation) solid.push([ot, ot + o.width]);
        }
      }
      let d = null;
      for (const e of ends) for (const edge of [t0, t0 + c.width]) {
        const k = e - edge; if (Math.abs(k) < 80 && (d === null || Math.abs(k) < Math.abs(d))) d = k;
      }
      t0 += d ?? 0;
      // never land inside a neighbour: slide to whichever side is nearer
      for (let pass = 0; pass < 4; pass++) {
        const hit = solid.find(([a, b]) => t0 < b - 0.5 && t0 + c.width > a + 0.5);
        if (!hit) break;
        t0 = Math.abs(hit[1] - t0) <= Math.abs(t0 + c.width - hit[0]) ? hit[1] : hit[0] - c.width;
      }
      const hits = ends.filter((e) => Math.abs(e - t0) < 0.5 || Math.abs(e - t0 - c.width) < 0.5);
      const left = Math.max(...ends.filter((e) => e <= t0 + 0.5)), right = Math.min(...ends.filter((e) => e >= t0 + c.width - 0.5));
      wallSnap = { w, t0, c, hits, left: t0 - left, right: right - t0 - c.width };
      const [px, pz] = wallPoint(w, t0, 0);
      return [Math.round(px * 10) / 10, Math.round(pz * 10) / 10, w.rotation];
    }
  }
  x = Math.round(x / 10) * 10; z = Math.round(z / 10) * 10;
  const f = footprint(c, x, z);
  const xs = [0], zs = [0];
  for (const o of project.cabinets) {
    if (o === c) continue;
    const g = footprint(o);
    xs.push(g.x0, g.x1); zs.push(g.z0, g.z1);
  }
  const best = (edges, lines) => {
    let d = null;
    for (const e of edges) for (const l of lines) if (Math.abs(l - e) < 40 && (d === null || Math.abs(l - e) < Math.abs(d))) d = l - e;
    return d ?? 0;
  };
  return [Math.round(x + best([f.x0, f.x1], xs)), Math.round(z + best([f.z0, f.z1], zs)), c.rotation];
}
// Snap cue: the wall lights up, an accent strip runs along its face under
// the cabinet, edges that locked onto a corner or a neighbour get an upright
// line, and a chip shows the gaps either side.
let snapCue = null, snapChip = null;
const snapMat = new THREE.MeshBasicMaterial({ color: 0x2f6fe4, transparent: true, opacity: 0.55, depthWrite: false });
const snapWallMat = new THREE.MeshStandardMaterial({ color: 0x2f6fe4, roughness: 1, transparent: true, opacity: 0.35, depthWrite: false });
function showSnapCue(info) {
  if (snapCue) { scene.remove(snapCue); snapCue = null; }
  if (snapChip) { snapChip.remove(); const k = overlayItems.findIndex((o) => o.el === snapChip); if (k >= 0) overlayItems.splice(k, 1); snapChip = null; }
  for (const g of roomGroup.children) for (const m of g.children) {
    if (m.userData.wall == null || m.material === glassMat) continue;
    m.material = info && m.userData.wall === info.w.i ? snapWallMat : m.userData.wall === selWall ? wallSelMat : wallMat;
  }
  if (!info) return;
  const { w, t0, c } = info, box = new THREE.BoxGeometry(1, 1, 1);
  snapCue = new THREE.Group();
  const strip = new THREE.Mesh(box, snapMat), sw = 40;
  const [sx, sz] = wallPoint(w, t0 + c.width / 2, sw / 2);
  strip.scale.set(c.width, 4, sw); strip.position.set(sx, 2 + c.elevation, sz);
  strip.rotation.y = THREE.MathUtils.degToRad(w.rotation);
  snapCue.add(strip);
  for (const t of info.hits) {
    const post = new THREE.Mesh(box, snapMat);
    const [px, pz] = wallPoint(w, t, 20);
    post.scale.set(12, c.height + 120, 12); post.position.set(px, c.elevation + (c.height + 120) / 2, pz);
    snapCue.add(post);
  }
  scene.add(snapCue);
  const fmt = (v) => (Math.abs(v) < 0.5 ? "0" : String(Math.round(v)));
  snapChip = h("span", { class: "snapchip" }, `◀ ${fmt(info.left)}`, h("b", {}, `Wall ${w.i + 1}`), `${fmt(info.right)} ▶`);
  $("#overlay").append(snapChip);
  const [cx, cz] = wallPoint(w, t0 + c.width / 2, c.depth + 80);
  overlayItems.push({ el: snapChip, pos: new THREE.Vector3(cx, c.elevation + c.height + 60, cz), always: true });
}

// Rotate 90° clockwise (seen from above) about the footprint centre.
function rotateCabinet(c) {
  const fs = groupOf(c).map((m) => footprint(m));
  const cx = (Math.min(...fs.map((f) => f.x0)) + Math.max(...fs.map((f) => f.x1))) / 2;
  const cz = (Math.min(...fs.map((f) => f.z0)) + Math.max(...fs.map((f) => f.z1))) / 2;
  // turning by -90° about (cx, cz): world v -> (vx cos + vz sin, -vx sin + vz cos) with sin -1, cos 0
  const vx = c.x - cx, vz = c.z - cz;
  placeCabinet(c, Math.round(cx - vz), Math.round(cz + vx), c.elevation, c.rotation - 90);
  recompute();
}
function resize() {
  const host = $("#v-3d"), w = host.clientWidth, hh = host.clientHeight;
  if (!w || !hh) return;
  renderer.setSize(w, hh);
  persp.aspect = w / hh; persp.updateProjectionMatrix();
  const half = ortho.userData.half || 3000, a = w / hh;
  Object.assign(ortho, { left: -half * a, right: half * a, top: half, bottom: -half });
  ortho.updateProjectionMatrix();
}
function mat(color, sel) {
  const k = color + sel;
  if (!matCache.has(k)) {
    const c = new THREE.Color(color);
    if (sel) c.lerp(new THREE.Color("#3d7cf5"), 0.35);
    matCache.set(k, new THREE.MeshStandardMaterial({ color: c, roughness: 0.75 }));
  }
  return matCache.get(k);
}
const applianceMat = new THREE.MeshStandardMaterial({ color: 0xb9bcc0, roughness: 0.6, metalness: 0.3, transparent: true, opacity: 0.55 });
const applianceSel = new THREE.MeshStandardMaterial({ color: 0x8fa8d8, roughness: 0.6, transparent: true, opacity: 0.6 });
const wallSelMat = new THREE.MeshStandardMaterial({ color: 0x8fb0ea, roughness: 1, transparent: true, opacity: 0.6, depthWrite: false });
const glassMat = new THREE.MeshStandardMaterial({ color: 0xa9cbe6, roughness: 0.1, transparent: true, opacity: 0.35, depthWrite: false });
const wallMat = new THREE.MeshStandardMaterial({ color: 0xdedad2, roughness: 1, transparent: true, opacity: 0.45, depthWrite: false });
const recessMat = new THREE.MeshStandardMaterial({ color: 0x000000, transparent: true, opacity: 0.08, depthWrite: false });
const hobMat = new THREE.MeshStandardMaterial({ color: 0x151617, roughness: 0.25, metalness: 0.2 });
const steelMat = new THREE.MeshStandardMaterial({ color: 0xc9ccd0, roughness: 0.35, metalness: 0.7 });
const bowlMat = new THREE.MeshStandardMaterial({ color: 0x6d7277, roughness: 0.5, metalness: 0.6 });
const edgeMat = new THREE.LineBasicMaterial({ color: 0x5a554c, transparent: true, opacity: 0.55 });
function updateScene() {
  if (!renderer) return;
  root.clear();
  groups.clear();
  for (const c of project.cabinets) {
    const g = new THREE.Group();
    g.position.set(c.x, c.elevation, c.z);
    g.rotation.y = THREE.MathUtils.degToRad(c.rotation);
    root.add(g); groups.set(c.id, g);
  }
  const box = new THREE.BoxGeometry(1, 1, 1), edges = new THREE.EdgesGeometry(box);
  buildRoom();
  for (const ph of report.build.placeholders || []) {
    const g = groups.get(ph.cabinet); if (!g) continue;
    const [x, y, z] = ph.box3d.pos, [sx, sy, sz] = ph.box3d.size;
    const m = new THREE.Mesh(box, ph.label === "Glass" ? glassMat : ph.cabinet === selected ? applianceSel : applianceMat);
    m.scale.set(sx, sy, sz); m.position.set(x + sx / 2, y + sy / 2, z + sz / 2);
    m.userData.cab = ph.cabinet;
    const l = new THREE.LineSegments(edges, edgeMat); l.scale.copy(m.scale); l.position.copy(m.position);
    g.add(m, l);
  }
  for (const p of report.build.parts) {
    if (!showFronts && p.role === "front") continue;
    const g = groups.get(p.cabinet); if (!g) continue;
    const [x, y, z] = p.box3d.pos, [sx, sy, sz] = p.box3d.size;
    for (const b of [p.box3d, ...(p.more3d || [])]) {
      const m = new THREE.Mesh(box, mat(matOf(p.material).color, isSel(p.cabinet)));
      m.scale.set(...b.size); m.position.set(b.pos[0] + b.size[0] / 2, b.pos[1] + b.size[1] / 2, b.pos[2] + b.size[2] / 2);
      m.userData.cab = p.cabinet;
      const l = new THREE.LineSegments(edges, edgeMat); l.scale.copy(m.scale); l.position.copy(m.position);
      g.add(m, l);
    }
    // Routed Shaker recess on the outside face (front parts are drawn inside face up, so x mirrors)
    if (p.role === "front") for (const k of (p.pockets || []).filter((k) => k.face === 2)) {
      const rc = new THREE.Mesh(box, recessMat);
      if (sx < sz) { // L corner return leaf: faces -x, drawing x runs back along z
        rc.scale.set(1, k.h, k.w);
        rc.position.set(x - 0.6, y + k.y + k.h / 2, z + sz - (k.x + k.w / 2));
      } else {
        rc.scale.set(k.w, k.h, 1);
        rc.position.set(x + sx - (k.x + k.w / 2), y + k.y + k.h / 2, z + sz + 0.6);
      }
      const rl = new THREE.LineSegments(edges, edgeMat); rl.scale.copy(rc.scale); rl.position.copy(rc.position);
      g.add(rc, rl);
    }
    // Sink and hob on worktop cut-outs: drawing y runs from the front edge.
    for (const c of p.cutouts || []) {
      const cx = x + c.x + c.w / 2, cz = z + sz - (c.y + c.h / 2), top = y + sy;
      if (c.kind === "hob") {
        const hob = new THREE.Mesh(box, hobMat);
        hob.scale.set(c.w + 30, 6, c.h + 30); hob.position.set(cx, top + 3, cz);
        g.add(hob);
      } else {
        const rim = new THREE.Mesh(box, steelMat);
        rim.scale.set(c.w + 30, 2, c.h + 30); rim.position.set(cx, top + 1, cz);
        const bowl = new THREE.Mesh(box, bowlMat);
        bowl.scale.set(c.w - 30, 2, c.h - 30); bowl.position.set(cx, top + 2.5, cz);
        g.add(rim, bowl);
      }
    }
  }
  if (!framed) { frame(); framed = true; }
  const sc = selected && project.cabinets.find((c) => c.id === selected);
  if (sc && groups.get(sc.id) && view !== "front") {
    const [hx, hz] = toWorld(sc, sc.width / 2, sc.depth);
    gizmoProxy.position.set(hx, sc.elevation, hz);
    gizmoProxy.rotation.set(0, THREE.MathUtils.degToRad(sc.rotation), 0);
    gizmo.attach(gizmoProxy);
    gizmo.showY = view !== "plan";
  } else gizmo.detach();
}
// Picture of the model as a PNG data URL, w x h pixels, without selection
// tint or gizmo. cam: "view" (the current camera) or "overview" (front-left,
// slightly above, framing everything).
function snapshot(w, h, cam = "view") {
  const keep = { selected, sel: [...selSet], size: renderer.getSize(new THREE.Vector2()), ratio: renderer.getPixelRatio() };
  selected = null; selSet.clear(); updateScene();
  gizmo.getHelper().visible = false;
  if (snapCue) snapCue.visible = false;
  let c = camera;
  if (cam === "overview" || view !== "3d") {
    const b = new THREE.Box3().setFromObject(root);
    if (b.isEmpty()) b.set(new THREE.Vector3(0, 0, 0), new THREE.Vector3(3000, 2200, 600));
    const ctr = b.getCenter(new THREE.Vector3()), size = b.getSize(new THREE.Vector3());
    c = new THREE.PerspectiveCamera(35, w / h, 10, 50000);
    const dist = Math.max(size.x, size.y * 1.6, size.z) * 1.55;
    c.position.set(ctr.x - dist * 0.35, ctr.y + dist * 0.38, ctr.z + dist);
    c.lookAt(ctr);
  } else {
    c = camera.clone(); c.aspect = w / h; c.updateProjectionMatrix();
  }
  // walls between the camera and the room are left out, and the floor grid
  const hidden = [grid, ...roomGroup.children.filter((g) => g.userData.w && wallLocal(g.userData.w, c.position.x, c.position.z)[1] < 0)];
  for (const o of hidden) o.visible = false;
  renderer.setPixelRatio(1);
  renderer.setSize(w, h, false);
  renderer.render(scene, c);
  for (const o of hidden) o.visible = true;
  const url = renderer.domElement.toDataURL("image/png");
  renderer.setPixelRatio(keep.ratio);
  renderer.setSize(keep.size.x, keep.size.y);
  selected = keep.selected; for (const id of keep.sel) selSet.add(id); selAnchor = selected;
  gizmo.getHelper().visible = true;
  updateScene(); resize();
  return url;
}
const dataUrlBytes = (u) => Uint8Array.from(atob(u.split(",")[1]), (ch) => ch.charCodeAt(0));

function frame() {
  // Fit cabinets and walls; an empty project frames a 4 x 3 m floor.
  const b = new THREE.Box3().setFromObject(root).union(new THREE.Box3().setFromObject(roomGroup));
  if (b.isEmpty()) b.set(new THREE.Vector3(0, 0, 0), new THREE.Vector3(4000, 2400, 3000));
  const c = b.getCenter(new THREE.Vector3()), size = b.getSize(new THREE.Vector3()), s = size.length();
  controls.target.copy(c);
  if (view === "3d") {
    camera.up.set(0, 1, 0);
    camera.position.set(c.x + s * 0.35, c.y + s * 0.35, c.z + s * 1.05);
  } else {
    const host = $("#v-3d"), aspect = host.clientWidth / Math.max(1, host.clientHeight);
    const [sw, sh] = view === "plan" ? [size.x, size.z] : [size.x, size.y];
    // Plan leaves room around the walls for their length labels.
    const pad = view === "plan" ? 1.12 + 800 / Math.max(sw, sh) : 1.12;
    ortho.userData.half = (Math.max(sh, sw / aspect) / 2) * pad;
    ortho.zoom = 1;
    if (view === "plan") { camera.up.set(0, 0, -1); camera.position.set(c.x, c.y + 20000, c.z); }
    else { camera.up.set(0, 1, 0); camera.position.set(c.x, c.y, c.z + 20000); }
    camera.lookAt(c);
    resize();
  }
  camera.updateProjectionMatrix();
}
// Room tools: always at the top of the left panel; Finish / Cancel also
// float on the canvas while drawing.
const ROOM_ICONS = {
  draw: '<path d="M4 26 V8 H16 V17 H25 V26 Z" fill="none"/><circle cx="4" cy="26" r="2" class="k"/><circle cx="4" cy="8" r="2" class="k"/><circle cx="16" cy="8" r="2" class="k"/><circle cx="16" cy="17" r="2" class="k"/><circle cx="25" cy="17" r="2" class="k"/>',
  dxf: '<path d="M6 3 h12 l6 6 v20 h-18 Z" fill="none"/><path d="M18 3 v6 h6" fill="none"/><path d="M10 16 h9 v8 h-9 Z" fill="none" stroke-width="1.2"/>',
  rect: '<rect x="4" y="7" width="20" height="19" fill="none"/><circle cx="4" cy="7" r="2" class="k"/><circle cx="24" cy="7" r="2" class="k"/><circle cx="24" cy="26" r="2" class="k"/><circle cx="4" cy="26" r="2" class="k"/>',
};
function iconButton(label, icon, onclick, cls = "") {
  const b = h("button", { class: cls, onclick });
  b.innerHTML = `<svg width="28" height="30" viewBox="0 0 28 32" stroke="currentColor" stroke-width="1.5"><style>.k{fill:currentColor;stroke:none}</style>${icon}</svg>`;
  b.append(label);
  return b;
}
function renderPlanTools() {
  const bar = $("#roombar"), el = $("#plantools");
  bar.replaceChildren(); el.replaceChildren();
  $("#v-3d").classList.toggle("drawing", !!drawing);
  el.hidden = !drawing;
  if (drawing) {
    const finish = () => h("button", { class: "pri", disabled: drawing.pts.length < 3, onclick: finishDrawing }, "Finish");
    const cancel = () => h("button", { onclick: () => { drawing = null; updatePreview(); recompute(); } }, "Cancel");
    bar.append(finish(), cancel());
    el.append(finish(), cancel());
  } else {
    bar.append(
      iconButton("Draw room", ROOM_ICONS.draw, startDrawing, "room"),
      iconButton("Rectangle", ROOM_ICONS.rect, rectangleRoom, "room"),
      iconButton("DXF plan", ROOM_ICONS.dxf, () => $("#dxffile").click(), "room"));
  }
}
function setView(v) {
  view = v;
  camera = v === "3d" ? persp : ortho;
  controls.object = camera;
  gizmo.camera = camera;
  controls.enableRotate = v === "3d";
  controls.mouseButtons.LEFT = v === "3d" ? THREE.MOUSE.ROTATE : THREE.MOUSE.PAN;
  controls.touches.ONE = v === "3d" ? THREE.TOUCH.ROTATE : THREE.TOUCH.PAN;
  controls.touches.TWO = THREE.TOUCH.DOLLY_PAN;
  $$("#views button").forEach((b) => b.classList.toggle("on", b.dataset.view === v));
  renderPlanTools();
  frame();
  controls.update();
}

// ---------- boot ----------
async function main() {
  await loadEngine();
  document.documentElement.dataset.engine = engine;
  const stored = loadLocal();
  project = migrate(stored || JSON.parse(cw.sample_project()));
  selected = null;

  renderPalette();
  $("#projbtn").onclick = () => { selected = null; selWall = null; recompute(); };
  $("#new").onclick = () => useProject(freshProject());
  $("#sample").onclick = () => useProject(JSON.parse(cw.sample_project()));
  $("#projectsbtn").onclick = openProjects;
  $("#dxffile").onchange = async (e) => {
    const f = e.target.files[0]; e.target.value = ""; if (!f) return;
    try {
      const r = JSON.parse(cw.import_room_dxf(JSON.stringify(project), await f.text()));
      project = r.project; selWall = null; selected = null;
      if (view !== "plan") setView("plan");
      commitRoom(); frame();
      toast(r.info);
    } catch (err) { toast(String(err.message || err)); }
  };
  $("#projects").addEventListener("click", (e) => { if (e.target.id === "projects") $("#projects").hidden = true; });
  $("#open").onclick = () => $("#file").click();
  $("#file").onchange = async (e) => {
    const f = e.target.files[0]; if (!f) return;
    try { const p = JSON.parse(await f.text()); cw.compute(JSON.stringify(migrate(p))); useProject(p); }
    catch (err) { toast("Not a valid project: " + (err.message || err)); }
    e.target.value = "";
  };
  $("#save").onclick = () => saveFile(slug(project.name) + ".json", JSON.stringify(project, null, 2));
  const today = () => new Date().toISOString().slice(0, 10);
  $("#zip").onclick = () => saveFile(slug(project.name) + "_cnc.zip", cw.export_zip(JSON.stringify(project), today()));
  $("#packet").onclick = () => saveFile(slug(project.name) + "_shop_packet.html", cw.packet_html(JSON.stringify(project), today()));
  $("#csv").onclick = () => saveFile(slug(project.name) + "_cutlist.csv", cw.export_one(JSON.stringify(project), "cutlist", 0));
  $("#labels").onclick = () => saveFile(slug(project.name) + "_labels.html", cw.export_one(JSON.stringify(project), "labels_html", 0));
  $("#obj").onclick = () => saveFile(slug(project.name) + ".obj", cw.export_one(JSON.stringify(project), "obj", 0));
  $$("#tabs button[data-tab]").forEach((b) => (b.onclick = () => { tab = b.dataset.tab; renderTab(); }));
  $("#fronts").onchange = (e) => { showFronts = e.target.checked; updateScene(); };
  $("#fit").onclick = frame;
  $("#shot").onclick = () => saveFile(slug(project.name) + ".png", dataUrlBytes(snapshot(2400, 1500)));
  $$("#views button").forEach((b) => (b.onclick = () => setView(b.dataset.view)));
  $("#undo").onclick = undo;
  $$("#mbar button").forEach((b) => (b.onclick = () => openSheet(b.dataset.sheet)));
  const header = $("header");
  $("#more").onclick = () => { const o = header.classList.toggle("open"); $("#more").setAttribute("aria-expanded", o); };
  $$("header .menu button").forEach((b) => b.addEventListener("click", () => { header.classList.remove("open"); $("#more").setAttribute("aria-expanded", false); }));
  // On a phone the view switch floats over the canvas instead of hiding at the end of the tab row.
  const placeViewSwitch = () => {
    const into = phone.matches ? $("#viewbar") : $("#tabs");
    const anchor = phone.matches ? null : $("#tabs label");
    into.insertBefore($("#views"), anchor);
    into.insertBefore($("#fit"), phone.matches ? null : anchor.nextSibling);
    into.insertBefore($("#shot"), $("#fit").nextSibling);
  };
  placeViewSwitch();
  phone.addEventListener("change", () => { openSheet(null); header.classList.remove("open"); placeViewSwitch(); resize(); });
  $("#redo").onclick = redo;
  addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    // In the coordinate boxes, undo/redo means the model, not the typed text.
    if (e.target.closest?.("#dial") && mod && /^[zy]$/i.test(e.key)) {
      e.preventDefault(); e.target.blur();
      (e.key.toLowerCase() === "y" || e.shiftKey) ? redo() : undo();
      return;
    }
    if (e.target.matches("input,select,textarea")) return;
    if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? redo() : undo(); return; }
    if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); redo(); return; }
    if ((e.key === "Delete" || e.key === "Backspace") && selected) {
      project.cabinets = project.cabinets.filter((c) => !isSel(c.id)); selected = null; recompute();
    }
    if (drawing && e.key === "Enter") { finishDrawing(); return; }
    if (drawing && e.key === "Escape") { drawing = null; updatePreview(); recompute(); return; }
    if (drawing && e.key === "Backspace") { drawing.pts.pop(); updatePreview(); renderPlanTools(); return; }
    if (e.key === "Escape") { selected = null; selWall = null; recompute(); }
    if (selected && e.key.startsWith("Arrow") && tab === "3d") {
      const c = project.cabinets.find((c) => c.id === selected);
      if (c) {
        e.preventDefault();
        const step = e.shiftKey ? 100 : 10;
        // screen directions: in Plan, up is -z; in 3D and Front, up/down move the height
        const [dx, dy] = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[e.key];
        if (view === "plan") placeCabinet(c, c.x + dx * step, c.z - dy * step);
        else placeCabinet(c, c.x + dx * step, c.z, Math.max(0, c.elevation + dy * step));
        recompute();
        return;
      }
    }
    if (e.key.toLowerCase() === "r" && !mod && selected) {
      const c = project.cabinets.find((c) => c.id === selected); if (c) rotateCabinet(c);
    }
  });

  setup3d();
  recompute();
  resize();

  // Read-only hook for scripts, agents and tests.
  window.ezCarpentry = {
    get project() { return JSON.parse(JSON.stringify(project)); },
    get selected() { return selected; },
    get view() { return view; },
    worldToScreen(x, y, z) {
      const r = renderer.domElement.getBoundingClientRect(), v = new THREE.Vector3(x, y, z).project(camera);
      return { x: r.left + ((v.x + 1) / 2) * r.width, y: r.top + ((1 - v.y) / 2) * r.height };
    },
  };

  if (isHosted()) {
    window.claude.use("downloads").then((d) => { downloads = d; }).catch(() => {});
  }
}
main().catch((e) => {
  document.body.replaceChildren(h("pre", { class: "fatal" }, "Failed to start: " + (e?.stack || e)));
});
