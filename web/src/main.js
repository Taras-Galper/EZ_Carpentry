import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import init, * as cwWasm from "../pkg/casework_wasm.js";
import wasmBytes from "../pkg/casework_wasm_bg.wasm";

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

const STORE = "casework.project.v1";
let project, report, selected = null, tab = "3d", showFronts = true;
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

// ---------- persistence ----------
function saveLocal() {
  try { localStorage.setItem(STORE, JSON.stringify(project)); } catch {}
}
function loadLocal() {
  try { const s = localStorage.getItem(STORE); return s ? JSON.parse(s) : null; } catch { return null; }
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
  renderList();
  renderProps();
  renderStatus();
  renderTab();
}

// ---------- cabinet presets ----------
const uid = (p) => { let i = 1; while (project.cabinets.some((c) => c.id === p + i)) i++; return p + i; };
const PRESETS = {
  Base: () => ({ kind: "base", name: "Base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [160], shelves: 1 }),
  Drawers: () => ({ kind: "base", name: "Drawer base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [180, 300, 290], shelves: 0 }),
  Wall: () => ({ kind: "wall", name: "Wall", width: 600, height: 720, depth: 320, elevation: 1450, toe_kick_height: 0, doors: 1, drawers: [], shelves: 2 }),
  Sink: () => ({ kind: "base", name: "Sink base", width: 800, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 2, drawers: [], shelves: 0, cutout: "sink" }),
  Hob: () => ({ kind: "base", name: "Hob base", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [180, 300, 290], shelves: 0, cutout: "hob" }),
  Dishwasher: () => ({ kind: "dishwasher", name: "Dishwasher", width: 600, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [], shelves: 0 }),
  Fridge: () => ({ kind: "appliance", name: "Fridge", width: 600, height: 2000, depth: 650, toe_kick_height: 0, doors: 0, drawers: [], shelves: 0 }),
  Filler: () => ({ kind: "filler", name: "Filler", width: 50, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 0, drawers: [], shelves: 0 }),
  Corner: () => ({ kind: "base", name: "Blind corner", width: 1200, height: 870, depth: 560, toe_kick_height: 100, toe_kick_depth: 60, doors: 1, drawers: [], shelves: 1, blind_side: "right", blind_width: 600 }),
  Tall: () => ({ kind: "tall", name: "Tall", width: 600, height: 2200, depth: 580, toe_kick_height: 100, toe_kick_depth: 60, doors: 2, drawers: [], shelves: 5 }),
};
function addCabinet(type) {
  const p = PRESETS[type]();
  const prefix = { base: "b", wall: "w", tall: "t", dishwasher: "d", appliance: "a", filler: "f" }[p.kind];
  // Append to the back-wall run of the same tier; match the project's front style.
  const same = project.cabinets.filter((c) => !c.rotation && (c.kind === "wall") === (p.kind === "wall"));
  const x = same.reduce((m, c) => Math.max(m, c.x + c.width), 0);
  const style = project.cabinets.find((c) => c.id === selected)?.door_style || project.cabinets[0]?.door_style || "slab";
  const c = {
    id: uid(prefix), x, z: 0, elevation: 0, rotation: 0, hinge_side: "left", handles: true, door_style: style,
    carcass_material: "carcass", back_material: "back", front_material: "front", drawer_material: "drawer",
    toe_kick_depth: 60, ...p,
  };
  project.cabinets.push(c);
  // Fill every field the preset leaves out with the Rust defaults.
  project = JSON.parse(cw.normalize(JSON.stringify(project)));
  selected = c.id;
  recompute();
}

// ---------- left: add palette ----------
// Small front-elevation icons, 28 x 34 viewBox.
const ICONS = {
  Base: '<rect x="3" y="8" width="22" height="22"/><line x1="20" y1="12" x2="20" y2="17"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Drawers: '<rect x="3" y="8" width="22" height="22"/><line x1="3" y1="14" x2="25" y2="14"/><line x1="3" y1="22" x2="25" y2="22"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Sink: '<rect x="1" y="6" width="26" height="2.5" class="k"/><rect x="3" y="8.5" width="22" height="21.5"/><line x1="14" y1="8.5" x2="14" y2="30"/><path d="M10 6 v-3 h4"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Hob: '<rect x="1" y="6" width="26" height="2.5" class="k"/><rect x="3" y="8.5" width="22" height="21.5"/><line x1="3" y1="15" x2="25" y2="15"/><line x1="3" y1="22" x2="25" y2="22"/><line x1="7" y1="4" x2="21" y2="4" class="t"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Corner: '<rect x="2" y="8" width="24" height="22"/><rect x="15" y="8" width="11" height="22" class="k"/><line x1="12" y1="12" x2="12" y2="17"/><rect x="2" y="30" width="24" height="3" class="k"/>',
  Wall: '<rect x="3" y="3" width="22" height="16"/><line x1="20" y1="13" x2="20" y2="17"/><line x1="0" y1="2" x2="28" y2="2" class="t"/>',
  Tall: '<rect x="7" y="1" width="14" height="30"/><line x1="7" y1="13" x2="21" y2="13"/><rect x="7" y="31" width="14" height="2" class="k"/>',
  Dishwasher: '<rect x="3" y="8" width="22" height="22"/><line x1="10" y1="11.5" x2="18" y2="11.5" class="t"/><rect x="3" y="30" width="22" height="3" class="k"/>',
  Fridge: '<rect x="6" y="3" width="16" height="30" stroke-dasharray="2.2 1.6"/><line x1="9" y1="9" x2="9" y2="14"/><line x1="9" y1="18" x2="9" y2="24"/>',
  Filler: '<rect x="11" y="8" width="6" height="22"/><rect x="11" y="30" width="6" height="3" class="k"/>',
};
function renderPalette() {
  const el = $("#palette");
  for (const [type, body] of Object.entries(ICONS)) {
    const b = h("button", { title: `Add ${type.toLowerCase()} cabinet`, "data-add": type, onclick: () => addCabinet(type) });
    b.innerHTML = `<svg width="28" height="34" viewBox="0 0 28 34" fill="#fff" stroke="currentColor" stroke-width="1.3"><style>.k{fill:currentColor;opacity:.35;stroke:none}.t{stroke-width:2.2}</style>${body}</svg>`;
    b.append(type);
    el.append(b);
  }
}

// ---------- left: list ----------
function renderList() {
  const ul = $("#list"); ul.replaceChildren();
  for (const c of project.cabinets) {
    ul.append(h("li", { class: c.id === selected ? "sel" : "", onclick: () => { selected = c.id; recompute(); } },
      h("b", {}, c.id), h("span", {}, c.name), h("em", {}, `${c.width}×${c.height}×${c.depth}`)));
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
  const c = project.cabinets.find((c) => c.id === selected);
  const mats = project.materials.map((m) => [m.id, m.name]);
  if (!c) {
    const s = project.settings;
    put(
      h("h3", {}, "Project"),
      field("Name", project, "name", "text"),
      h("h3", {}, "Machining"),
      h("div", { class: "grid2" },
        field("Kerf", s, "kerf"), field("Sheet trim", s, "sheet_trim"),
        field("Front gap", s, "front_gap"), field("Edge band", s, "edge_band"),
        field("System line", s, "system_line"), field("Hinge cup inset", s, "hinge_cup_inset"),
        field("Slide clearance", s, "slide_clearance"), field("Handle c/c", s, "handle_spacing"),
        field("Shelf setback", s, "shelf_setback")),
      h("h3", {}, "Room"),
      h("div", { class: "grid3" }, field("Width", s, "room_width"), field("Depth", s, "room_depth"), field("Height", s, "room_height")),
      h("h3", {}, "Worktop"),
      h("div", { class: "grid2" }, field("Thickness", s, "worktop_thickness"), field("Overhang", s, "worktop_overhang")),
      h("h3", {}, "Shaker"),
      h("div", { class: "grid2" }, field("Frame width", s, "shaker_frame"), field("Groove", s, "shaker_groove")),
      h("h3", {}, "Materials"),
      ...project.materials.map((m) => h("div", { class: "mat" },
        h("div", { class: "grid2" },
          field("Name", m, "name", "text"), field("Colour", m, "color", "text"),
          field("Thickness", m, "thickness"), field("Grain", m, "grain", "check"),
          field("Sheet L", m, "sheet_length"), field("Sheet W", m, "sheet_width")))),
    );
    return;
  }
  const carcass = ["base", "wall", "tall"].includes(c.kind);
  const blind = c.blind_side && c.blind_side !== "none";
  const cut = c.cutout && c.cutout !== "none";
  put(
    h("div", { class: "row" },
      h("h3", {}, c.id),
      h("button", { onclick: () => {
        const d = structuredClone(c); d.id = uid(c.id.replace(/\d+$/, "")); d.x = c.x + c.width;
        project.cabinets.push(d); selected = d.id; recompute();
      } }, "Duplicate"),
      h("button", { title: "Rotate 90° (R)", onclick: () => rotateCabinet(c) }, "⟳"),
      h("button", { class: "danger", onclick: () => {
        project.cabinets = project.cabinets.filter((x) => x !== c); selected = null; recompute();
      } }, "Delete")),
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
        field("Doors", c, "doors", "select", [[0, "None"], [1, "1"], [2, "2"]]),
        field("Hinge", c, "hinge_side", "select", [["left", "Left"], ["right", "Right"]]),
        field("Front style", c, "door_style", "select", [["slab", "Slab"], ["shaker", "Shaker"]]),
        field("Blind corner", c, "blind_side", "select", [["none", "None"], ["left", "Left"], ["right", "Right"]]),
        blind ? [field("Blind width", c, "blind_width"), h("span")] : null,
        field("Shelves", c, "shelves", "int"),
        field("Handles", c, "handles", "check"),
        field("End panel L", c, "end_left", "check"),
        field("End panel R", c, "end_right", "check")),
      field("Drawers (front heights, top first)", c, "drawers", "list"),
    ] : null,
    c.kind === "dishwasher" ? h("div", { class: "grid2" },
      field("Front style", c, "door_style", "select", [["slab", "Slab"], ["shaker", "Shaker"]]),
      field("Handles", c, "handles", "check")) : null,
    c.kind === "base" ? [
      h("h3", {}, "Worktop cut-out"),
      h("div", { class: "grid2" },
        field("Type", c, "cutout", "select", [["none", "None"], ["sink", "Sink"], ["hob", "Hob"]]),
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
  $$("#tabs button").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
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
      h("button", { onclick: () => saveFile(slug(project.name) + "_quote.html", cw.export_one(json, "quote_html", 0)) }, "Customer quote"),
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
  const prices = h("div", { class: "qprices" },
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
  for (const c of p.cutouts || []) {
    const r = document.createElementNS(ns, "rect");
    r.setAttribute("x", X(c.x)); r.setAttribute("y", Y(c.y + c.h));
    r.setAttribute("width", c.w * s); r.setAttribute("height", c.h * s);
    r.setAttribute("rx", c.r * s); r.setAttribute("fill", "#fff"); r.setAttribute("stroke", "#222");
    svg.append(r);
  }
  for (const o of p.holes) {
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
  for (const o of holes) { const k = `Ø${o.dia} ${o.depth > 0 ? o.depth + " deep" : "thru"}`; m.set(k, (m.get(k) || 0) + 1); }
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
        h("span", {}, `#${i + 1} ${matOf(s.material).name} · ${s.placements.length} parts · ${s.yield_pct}%`),
        h("button", { onclick: () => saveFile(name + ".dxf", cw.export_one(json, "sheet_dxf", i)) }, "DXF"),
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
      h("thead", {}, h("tr", {}, h("th", {}, "Item"), h("th", {}, "Qty"))),
      h("tbody", {}, [...tot].map(([n, q]) => h("tr", {}, h("td", {}, n), h("td", { class: "n" }, q))))),
    h("table", { class: "hw" },
      h("thead", {}, h("tr", {}, h("th", {}, "Cab"), h("th", {}, "Item"), h("th", {}, "Qty"))),
      h("tbody", {}, report.build.hardware.map((x) =>
        h("tr", {}, h("td", {}, x.cabinet), h("td", {}, x.name), h("td", { class: "n" }, x.qty))))));
}

// ---------- 3D ----------
let renderer, scene, camera, controls, root, framed = false;
let persp, ortho, view = "3d"; // "3d" | "plan" | "front"
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
  const grid = new THREE.GridHelper(10000, 100, 0xcfcac0, 0xe2ded6); scene.add(grid);
  root = new THREE.Group(); scene.add(root);
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
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    aim(e);
    const hit = ray.intersectObjects(root.children, true).find((x) => x.object.userData.cab);
    const c = hit && project.cabinets.find((c) => c.id === hit.object.userData.cab);
    const p = c && floorPoint();
    drag = { sx: e.clientX, sy: e.clientY, c, moved: false, off: p ? [c.x - p.x, c.z - p.z] : null };
  });
  el.addEventListener("pointermove", (e) => {
    if (!drag?.c || !drag.off) return;
    if (!drag.moved && Math.hypot(e.clientX - drag.sx, e.clientY - drag.sy) < 5) return;
    if (!drag.moved) { drag.moved = true; controls.enabled = false; el.setPointerCapture(e.pointerId); }
    aim(e);
    const p = floorPoint(); if (!p) return;
    let [x, z] = snap(drag.c, p.x + drag.off[0], view === "front" ? drag.c.z : p.z + drag.off[1]);
    if (view === "front") z = drag.c.z;
    drag.x = x; drag.z = z;
    groups.get(drag.c.id)?.position.set(x, drag.c.elevation, z);
    $("#coords").textContent = `${drag.c.id}  x ${x}  z ${z}`;
  });
  el.addEventListener("pointerup", (e) => {
    const d = drag; drag = null;
    controls.enabled = true;
    $("#coords").textContent = "";
    if (!d) return;
    if (d.moved && d.x != null) {
      d.c.x = d.x; d.c.z = d.z; selected = d.c.id; recompute(); return;
    }
    if (Math.hypot(e.clientX - d.sx, e.clientY - d.sy) > 4) return;
    selected = d.c ? d.c.id : null;
    recompute();
  });
  (function loop() { requestAnimationFrame(loop); controls.update(); renderer.render(scene, camera); })();
}
// Cabinet-local (x, z) -> world, same convention as the Rust core and three.js.
function toWorld(c, x, z, ox = c.x, oz = c.z, rot = c.rotation) {
  const r = THREE.MathUtils.degToRad(rot), sn = Math.sin(r), cs = Math.cos(r);
  return [ox + x * cs + z * sn, oz - x * sn + z * cs];
}
function footprint(c, ox = c.x, oz = c.z, rot = c.rotation) {
  const pts = [[0, 0], [c.width, 0], [0, c.depth], [c.width, c.depth]].map(([x, z]) => toWorld(c, x, z, ox, oz, rot));
  const xs = pts.map((p) => p[0]), zs = pts.map((p) => p[1]);
  return { x0: Math.min(...xs), x1: Math.max(...xs), z0: Math.min(...zs), z1: Math.max(...zs) };
}
// 10 mm grid, then snap the footprint's edges to walls and other cabinets within 40 mm.
function snap(c, x, z) {
  x = Math.round(x / 10) * 10; z = Math.round(z / 10) * 10;
  const f = footprint(c, x, z);
  const s = project.settings, xs = [0], zs = [0];
  if (s.room_width > 0) xs.push(s.room_width);
  if (s.room_depth > 0) zs.push(s.room_depth);
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
  return [Math.round(x + best([f.x0, f.x1], xs)), Math.round(z + best([f.z0, f.z1], zs))];
}
// Rotate 90° clockwise (seen from above) about the footprint centre.
function rotateCabinet(c) {
  const f = footprint(c), cx = (f.x0 + f.x1) / 2, cz = (f.z0 + f.z1) / 2;
  let rot = c.rotation - 90;
  rot = ((rot + 180) % 360 + 360) % 360 - 180;
  const [lx, lz] = toWorld(c, c.width / 2, c.depth / 2, 0, 0, rot);
  c.rotation = rot; c.x = Math.round(cx - lx); c.z = Math.round(cz - lz);
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
const wallMat = new THREE.MeshStandardMaterial({ color: 0xdedad2, roughness: 1, transparent: true, opacity: 0.45, depthWrite: false });
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
  const rs = project.settings;
  if (rs.room_width > 0 && rs.room_height > 0) {
    const t = 100, W = rs.room_width, D = rs.room_depth || 3000, H = rs.room_height;
    for (const [x, z, sx, sz] of [[-t, -t, W + 2 * t, t], [-t, 0, t, D], [W, 0, t, D]]) {
      const m = new THREE.Mesh(box, wallMat);
      m.scale.set(sx, H, sz); m.position.set(x + sx / 2, H / 2, z + sz / 2);
      root.add(m);
    }
  }
  for (const ph of report.build.placeholders || []) {
    const g = groups.get(ph.cabinet); if (!g) continue;
    const [x, y, z] = ph.box3d.pos, [sx, sy, sz] = ph.box3d.size;
    const m = new THREE.Mesh(box, ph.cabinet === selected ? applianceSel : applianceMat);
    m.scale.set(sx, sy, sz); m.position.set(x + sx / 2, y + sy / 2, z + sz / 2);
    m.userData.cab = ph.cabinet;
    const l = new THREE.LineSegments(edges, edgeMat); l.scale.copy(m.scale); l.position.copy(m.position);
    g.add(m, l);
  }
  for (const p of report.build.parts) {
    if (!showFronts && p.role === "front") continue;
    const g = groups.get(p.cabinet); if (!g) continue;
    const [x, y, z] = p.box3d.pos, [sx, sy, sz] = p.box3d.size;
    const m = new THREE.Mesh(box, mat(matOf(p.material).color, p.cabinet === selected));
    m.scale.set(sx, sy, sz); m.position.set(x + sx / 2, y + sy / 2, z + sz / 2);
    m.userData.cab = p.cabinet;
    const l = new THREE.LineSegments(edges, edgeMat); l.scale.copy(m.scale); l.position.copy(m.position);
    g.add(m, l);
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
}
function frame() {
  const b = new THREE.Box3().setFromObject(root);
  if (b.isEmpty()) return;
  const c = b.getCenter(new THREE.Vector3()), size = b.getSize(new THREE.Vector3()), s = size.length();
  controls.target.copy(c);
  if (view === "3d") {
    camera.up.set(0, 1, 0);
    camera.position.set(c.x + s * 0.35, c.y + s * 0.35, c.z + s * 1.05);
  } else {
    const host = $("#v-3d"), aspect = host.clientWidth / Math.max(1, host.clientHeight);
    const [sw, sh] = view === "plan" ? [size.x, size.z] : [size.x, size.y];
    ortho.userData.half = (Math.max(sh, sw / aspect) / 2) * 1.12;
    ortho.zoom = 1;
    if (view === "plan") { camera.up.set(0, 0, -1); camera.position.set(c.x, c.y + 20000, c.z); }
    else { camera.up.set(0, 1, 0); camera.position.set(c.x, c.y, c.z + 20000); }
    camera.lookAt(c);
    resize();
  }
  camera.updateProjectionMatrix();
}
function setView(v) {
  view = v;
  camera = v === "3d" ? persp : ortho;
  controls.object = camera;
  controls.enableRotate = v === "3d";
  controls.mouseButtons.LEFT = v === "3d" ? THREE.MOUSE.ROTATE : THREE.MOUSE.PAN;
  $$("#views button").forEach((b) => b.classList.toggle("on", b.dataset.view === v));
  frame();
  controls.update();
}

// ---------- boot ----------
async function main() {
  await loadEngine();
  document.documentElement.dataset.engine = engine;
  project = migrate(loadLocal() || JSON.parse(cw.sample_project()));
  selected = null;

  renderPalette();
  $("#projbtn").onclick = () => { selected = null; recompute(); };
  $("#new").onclick = () => { project = JSON.parse(cw.sample_project()); project.cabinets = []; project.name = "Untitled"; selected = null; recompute(); };
  $("#sample").onclick = () => { project = JSON.parse(cw.sample_project()); selected = null; framed = false; recompute(); };
  $("#open").onclick = () => $("#file").click();
  $("#file").onchange = async (e) => {
    const f = e.target.files[0]; if (!f) return;
    try { project = migrate(JSON.parse(await f.text())); cw.compute(JSON.stringify(project)); selected = null; framed = false; recompute(); }
    catch (err) { toast("Not a valid project: " + (err.message || err)); }
    e.target.value = "";
  };
  $("#save").onclick = () => saveFile(slug(project.name) + ".json", JSON.stringify(project, null, 2));
  $("#zip").onclick = () => saveFile(slug(project.name) + "_cnc.zip", cw.export_zip(JSON.stringify(project)));
  $("#csv").onclick = () => saveFile(slug(project.name) + "_cutlist.csv", cw.export_one(JSON.stringify(project), "cutlist", 0));
  $("#labels").onclick = () => saveFile(slug(project.name) + "_labels.csv", cw.export_one(JSON.stringify(project), "labels", 0));
  $("#obj").onclick = () => saveFile(slug(project.name) + ".obj", cw.export_one(JSON.stringify(project), "obj", 0));
  $$("#tabs button").forEach((b) => (b.onclick = () => { tab = b.dataset.tab; renderTab(); }));
  $("#fronts").onchange = (e) => { showFronts = e.target.checked; updateScene(); };
  $("#fit").onclick = frame;
  $$("#views button").forEach((b) => (b.onclick = () => setView(b.dataset.view)));
  $("#undo").onclick = undo;
  $("#redo").onclick = redo;
  addEventListener("keydown", (e) => {
    if (e.target.matches("input,select,textarea")) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? redo() : undo(); return; }
    if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); redo(); return; }
    if ((e.key === "Delete" || e.key === "Backspace") && selected) {
      project.cabinets = project.cabinets.filter((c) => c.id !== selected); selected = null; recompute();
    }
    if (e.key === "Escape") { selected = null; recompute(); }
    if (e.key.toLowerCase() === "r" && !mod && selected) {
      const c = project.cabinets.find((c) => c.id === selected); if (c) rotateCabinet(c);
    }
  });

  setup3d();
  recompute();
  resize();

  if (isHosted()) {
    window.claude.use("downloads").then((d) => { downloads = d; }).catch(() => {});
  }
}
main().catch((e) => {
  document.body.replaceChildren(h("pre", { class: "fatal" }, "Failed to start: " + (e?.stack || e)));
});
