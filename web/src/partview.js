// Part view: one part on its own. A zoomable drawing of either face (bores,
// pockets, cut-outs, banding, dimensions for the selected bore) or the part
// in 3D, and the panel that edits it. Edits live in project.part_edits[id]
// and are applied by the Rust build, so every export sees them.
//
// main.js passes a context:
//   { part, edit, commit(fn), refresh(), close(), step(d), index, count,
//     h, matOf, materials, summarizeHoles, exportDxf() }
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

const NS = "http://www.w3.org/2000/svg";
const BAND = "#e0662a", BLIND = "#1667c4", THRU = "#c42020", ADDED = "#14915a", SELC = "#2f6fe4";
const st = { mode: "2d", face: 1, view: null, fitFor: null, sel: null, spec: { dia: 5, depth: 12 }, gl: null, drag: null, preview: null };

export const same = (a, b) => !!a && !!b && Math.abs(a.x - b.x) < 0.05 && Math.abs(a.y - b.y) < 0.05 &&
  Math.abs(a.dia - b.dia) < 0.05 && (a.edge ?? null) === (b.edge ?? null);
const clean = (o) => ({ x: o.x, y: o.y, dia: o.dia, depth: o.depth, ...(o.edge != null ? { edge: o.edge } : {}) });
const half = (v) => Math.round(v * 2) / 2;
const fmt = (v) => String(Math.round(v * 10) / 10);
const group = (labels) => { const m = new Map(); for (const l of labels) m.set(l, (m.get(l) || 0) + 1); return [...m].map(([l, n]) => `${n}× ${l}`).join(", "); };

// ---------- edits ----------
function ops(ctx) {
  const isAdded = (o) => (ctx.edit.add || []).some((a) => same(a, o));
  // a bore put back where a removed one was cancels both
  const tidy = (e) => {
    for (const a of [...(e.add || [])]) {
      const r = (e.remove || []).find((r) => same(r, a) && r.depth === a.depth);
      if (r) { e.add = e.add.filter((x) => x !== a); e.remove = e.remove.filter((x) => x !== r); }
    }
  };
  return {
    isAdded,
    del(o) { ctx.commit((e) => { if (isAdded(o)) e.add = e.add.filter((a) => !same(a, o)); else (e.remove ||= []).push(clean(o)); }); st.sel = null; },
    move(o, n) {
      n = clean(n);
      ctx.commit((e) => {
        if (isAdded(o)) e.add = e.add.map((a) => (same(a, o) ? n : a));
        else { (e.remove ||= []).push(clean(o)); (e.add ||= []).push(n); }
        tidy(e);
      });
      st.sel = n;
    },
    add(n) { n = clean(n); ctx.commit((e) => { (e.add ||= []).push(n); tidy(e); }); st.sel = n; st.spec = { dia: n.dia, depth: n.depth }; },
  };
}

// Put a bore onto edge `edge` (null = the face), keeping it on the part.
function onEdge(p, o, edge) {
  const r = o.dia / 2, cl = (v, lo, hi) => Math.min(Math.max(v, lo), hi);
  const n = { ...o, edge };
  if (edge == null) { delete n.edge; n.x = cl(o.x, r + 1, p.w - r - 1); n.y = cl(o.y, r + 1, p.h - r - 1); return n; }
  if (edge === 0 || edge === 2) { n.y = edge === 0 ? 0 : p.h; n.x = cl(o.x, r, p.w - r); } else { n.x = edge === 1 ? p.w : 0; n.y = cl(o.y, r, p.h - r); }
  if (o.edge == null) { n.dia = Math.min(o.dia, 8); n.depth = 30; }
  return n;
}

// ---------- 2D drawing ----------
function fit(svg, p) {
  const W = svg.clientWidth || 600, H = svg.clientHeight || 400, m = 70;
  const s = Math.max(0.01, Math.min((W - 2 * m) / p.w, (H - 2 * m) / p.h));
  st.view = { s, ox: (W - p.w * s) / 2, oy: (H + p.h * s) / 2 };
}

function el(tag, attrs, parent) {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  parent?.append(e);
  return e;
}

function inside(outline, x, y) {
  let c = false;
  for (let i = 0, j = outline.length - 1; i < outline.length; j = i++) {
    const [a, b] = [outline[i], outline[j]];
    if ((a[1] > y) !== (b[1] > y) && x < a[0] + ((y - a[1]) / (b[1] - a[1])) * (b[0] - a[0])) c = !c;
  }
  return c;
}

// Which side of the bounding rectangle (0 bottom, 1 right, 2 top, 3 left)
// segment a-b lies on, or -1.
function edgeOf(p, a, b) {
  const on = (u, v, t) => Math.abs(u - t) < 0.5 && Math.abs(v - t) < 0.5;
  return on(a[1], b[1], 0) ? 0 : on(a[0], b[0], p.w) ? 1 : on(a[1], b[1], p.h) ? 2 : on(a[0], b[0], 0) ? 3 : -1;
}

function draw(svg, ctx) {
  const p = ctx.part, { s, ox, oy } = st.view, f2 = st.face === 2;
  const X = (x) => ox + s * (f2 ? p.w - x : x), Y = (y) => oy - s * y;
  svg.replaceChildren();
  const holes = p.holes.map((o) => (st.preview && same(o, st.preview.from) ? st.preview.to : o));
  const { isAdded } = ops(ctx);
  // grain: faint lines across the part
  const g = el("g", {}, svg);
  el("polygon", { points: p.outline.map(([x, y]) => `${X(x)},${Y(y)}`).join(" "), fill: ctx.matOf(p.material).color, stroke: "#222", "stroke-width": 1.2, class: "pv-face" }, g);
  const grain = el("path", { fill: "none", stroke: "rgba(0,0,0,.07)", "pointer-events": "none" }, g);
  let d = "";
  const step = Math.max(12, 40 * s) / s;
  if (p.grain_y) for (let x = step / 2; x < p.w; x += step) d += `M${X(x)},${Y(0)}V${Y(p.h)}`;
  else for (let y = step / 2; y < p.h; y += step) d += `M${X(0)},${Y(y)}H${X(p.w)}`;
  grain.setAttribute("d", d);
  for (const c of p.cutouts || []) el("rect", { x: Math.min(X(c.x), X(c.x + c.w)), y: Y(c.y + c.h), width: c.w * s, height: c.h * s, rx: c.r * s, fill: "#f4f2ee", stroke: "#222" }, svg);
  for (const k of p.pockets || []) {
    const near = (k.face === 2) === f2;
    el("rect", { x: Math.min(X(k.x), X(k.x + k.w)), y: Y(k.y + k.h), width: Math.max(1, k.w * s), height: Math.max(1, k.h * s), rx: k.r * s,
      fill: near ? "rgba(0,0,0,.18)" : "none", stroke: near ? "#444" : "#999", "stroke-dasharray": near ? "" : "5 4" }, svg);
  }
  // banding: thick on banded edges; every edge clickable
  const segs = p.outline.map((a, i) => [a, p.outline[(i + 1) % p.outline.length], i]);
  const rect = !(p.band_segs || []).length;
  for (const sg of segs) {
    const [a, b, i] = sg, k = rect ? edgeOf(p, a, b) : -1;
    if (rect && k < 0) continue; // a notch: banded with its side or not at all
    const on = rect ? k >= 0 && p.edges[k] : (p.band_segs || []).includes(i);
    const at = { x1: X(a[0]), y1: Y(a[1]), x2: X(b[0]), y2: Y(b[1]) };
    if (on) el("line", { ...at, stroke: BAND, "stroke-width": 4, "pointer-events": "none" }, svg);
    const hit = el("line", { ...at, stroke: "transparent", "stroke-width": 14, class: "pv-edge", "data-seg": i, "data-edge": k }, svg);
    el("title", {}, hit).textContent = on ? "Banded" : "Not banded";
  }
  // bores
  holes.forEach((o, i) => {
    const sel = same(o, st.sel) || (st.preview && same(st.preview.to, o));
    const col = sel ? SELC : isAdded(o) ? ADDED : o.depth > 0 ? BLIND : THRU;
    const gg = el("g", { class: "pv-hole", "data-i": i }, svg);
    el("title", {}, gg).textContent = `Ø${fmt(o.dia)} ${o.edge != null ? "edge, " + fmt(o.depth) + " deep" : o.depth > 0 ? fmt(o.depth) + " deep" : "through"} · ${fmt(o.x)}, ${fmt(o.y)}`;
    if (o.edge != null) {
      const [dx, dy] = [[0, 1], [-1, 0], [0, -1], [1, 0]][o.edge];
      const x2 = o.x + dx * o.depth, y2 = o.y + dy * o.depth;
      el("line", { x1: X(o.x), y1: Y(o.y), x2: X(x2), y2: Y(y2), stroke: col, "stroke-width": Math.max(3, o.dia * s), "stroke-opacity": 0.75 }, gg);
      el("line", { x1: X(o.x), y1: Y(o.y), x2: X(x2), y2: Y(y2), stroke: "transparent", "stroke-width": 16 }, gg);
      // drilled from the other face's edge too: same in both views
      return;
    }
    const near = !f2 || o.depth === 0;
    el("circle", { cx: X(o.x), cy: Y(o.y), r: Math.max(2.5, (o.dia / 2) * s), fill: o.depth === 0 ? "#fff" : near ? col : "none",
      "fill-opacity": near ? (o.depth === 0 ? 1 : 0.35) : 0, stroke: col, "stroke-width": sel ? 2.5 : 1.4, "stroke-dasharray": near ? "" : "3 3" }, gg);
    el("circle", { cx: X(o.x), cy: Y(o.y), r: Math.max(9, (o.dia / 2) * s), fill: "transparent" }, gg);
  });
  // overall dimensions
  const L = Math.min(X(0), X(p.w)), R = Math.max(X(0), X(p.w));
  dim(svg, L, Y(0) + 34, R, Y(0) + 34, fmt(p.w), "h");
  dim(svg, L - 34, Y(0), L - 34, Y(p.h), fmt(p.h), "v");
  // the selected bore: from the left and bottom as seen, and its size
  const o = st.preview?.to || holes.find((x) => same(x, st.sel));
  if (o) {
    const hx = X(o.x), hy = Y(o.y);
    el("line", { x1: hx, y1: hy, x2: hx, y2: Y(0) + 58, stroke: SELC, "stroke-dasharray": "3 3" }, svg);
    el("line", { x1: hx, y1: hy, x2: L - 58, y2: hy, stroke: SELC, "stroke-dasharray": "3 3" }, svg);
    dim(svg, L, Y(0) + 58, hx, Y(0) + 58, fmt(f2 ? p.w - o.x : o.x), "h", SELC);
    dim(svg, L - 58, Y(0), L - 58, hy, fmt(o.y), "v", SELC);
    const t = el("text", { x: hx + 10, y: hy - 10, fill: SELC, "font-size": 12, "font-weight": 600, class: "pv-lbl" }, svg);
    t.textContent = `Ø${fmt(o.dia)}${o.depth > 0 ? " × " + fmt(o.depth) : " thru"}`;
  }
}

function dim(svg, x1, y1, x2, y2, text, dir, col = "#555") {
  if (Math.hypot(x2 - x1, y2 - y1) < 1) return;
  const g = el("g", { stroke: col, fill: col, "font-size": 11, "pointer-events": "none" }, svg);
  el("line", { x1, y1, x2, y2 }, g);
  const t = 5;
  for (const [x, y] of [[x1, y1], [x2, y2]])
    el("line", dir === "h" ? { x1: x, y1: y - t, x2: x, y2: y + t } : { x1: x - t, y1: y, x2: x + t, y2: y }, g);
  const tx = el("text", dir === "h"
    ? { x: (x1 + x2) / 2, y: y1 - 4, "text-anchor": "middle", stroke: "none" }
    : { x: x1 - 4, y: (y1 + y2) / 2, "text-anchor": "middle", stroke: "none", transform: `rotate(-90 ${x1 - 4} ${(y1 + y2) / 2})` }, g);
  tx.textContent = text;
}

function wire(svg, ctx) {
  if (svg.__wired) { svg.__ctx = ctx; return; }
  svg.__wired = true; svg.__ctx = ctx;
  const pts = new Map();
  const toPart = (e) => {
    const c = svg.__ctx, p = c.part, r = svg.getBoundingClientRect(), { s, ox, oy } = st.view;
    const mx = (e.clientX - r.left - ox) / s;
    return [st.face === 2 ? p.w - mx : mx, (oy - (e.clientY - r.top)) / s];
  };
  svg.addEventListener("wheel", (e) => {
    e.preventDefault();
    const r = svg.getBoundingClientRect(), k = Math.exp(-e.deltaY * 0.0015), v = st.view;
    const px = e.clientX - r.left, py = e.clientY - r.top;
    v.ox = px - (px - v.ox) * k; v.oy = py - (py - v.oy) * k; v.s *= k;
    draw(svg, svg.__ctx);
  }, { passive: false });
  svg.addEventListener("pointerdown", (e) => {
    svg.setPointerCapture(e.pointerId);
    pts.set(e.pointerId, [e.clientX, e.clientY]);
    if (pts.size === 2) { st.drag = { pinch: true }; return; }
    const c = svg.__ctx, hole = e.target.closest(".pv-hole"), edge = e.target.closest(".pv-edge");
    st.drag = { x: e.clientX, y: e.clientY, moved: false, ox: st.view.ox, oy: st.view.oy };
    if (hole) { st.drag.hole = c.part.holes[+hole.dataset.i]; st.drag.at = toPart(e); }
    else if (edge) st.drag.edge = { seg: +edge.dataset.seg, k: +edge.dataset.edge };
  });
  svg.addEventListener("pointermove", (e) => {
    if (!pts.has(e.pointerId)) return;
    const prev = pts.get(e.pointerId);
    pts.set(e.pointerId, [e.clientX, e.clientY]);
    const d = st.drag; if (!d) return;
    if (d.pinch && pts.size === 2) {
      const [a, b] = [...pts.values()], other = a === pts.get(e.pointerId) ? b : a;
      const k = Math.hypot(e.clientX - other[0], e.clientY - other[1]) / Math.max(1, Math.hypot(prev[0] - other[0], prev[1] - other[1]));
      const r = svg.getBoundingClientRect(), v = st.view, px = (e.clientX + other[0]) / 2 - r.left, py = (e.clientY + other[1]) / 2 - r.top;
      v.ox = px - (px - v.ox) * k + (e.clientX - prev[0]) / 2; v.oy = py - (py - v.oy) * k + (e.clientY - prev[1]) / 2; v.s *= k;
      draw(svg, svg.__ctx); return;
    }
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) return;
    d.moved = true;
    const c = svg.__ctx, p = c.part;
    if (d.hole) {
      const [x, y] = toPart(e), o = d.hole;
      let n = { ...o, x: half(o.x + x - d.at[0]), y: half(o.y + y - d.at[1]) };
      n = onEdge(p, n, o.edge ?? null);
      if (o.edge == null) { n.x = Math.min(Math.max(n.x, 0), p.w); n.y = Math.min(Math.max(n.y, 0), p.h); }
      st.preview = { from: o, to: n };
      draw(svg, c);
    } else if (!d.edge || d.moved) {
      st.view.ox = d.ox + e.clientX - d.x; st.view.oy = d.oy + e.clientY - d.y;
      draw(svg, c);
    }
  });
  const up = (e) => {
    pts.delete(e.pointerId);
    const d = st.drag; if (!d) return;
    if (d.pinch) { if (!pts.size) st.drag = null; return; }
    st.drag = null;
    const c = svg.__ctx, p = c.part;
    if (d.hole) {
      const pv = st.preview; st.preview = null;
      if (d.moved && pv && !same(pv.from, pv.to)) return ops(c).move(pv.from, pv.to);
      st.sel = d.hole; return c.refresh();
    }
    if (d.edge && !d.moved) {
      return c.commit((ed) => {
        if (d.edge.k >= 0 && !(p.band_segs || []).length) { const b = [...p.edges]; b[d.edge.k] = !b[d.edge.k]; ed.edges = b; }
        else { const set = new Set(p.band_segs || []); set.has(d.edge.seg) ? set.delete(d.edge.seg) : set.add(d.edge.seg); ed.band_segs = [...set].sort((a, b) => a - b); }
      });
    }
    if (!d.moved && st.sel) { st.sel = null; c.refresh(); }
  };
  svg.addEventListener("pointerup", up);
  svg.addEventListener("pointercancel", up);
  svg.addEventListener("dblclick", (e) => {
    // pointer capture makes the svg the target, so test the point itself
    const c = svg.__ctx, [x, y] = toPart(e);
    if (!inside(c.part.outline, x, y) || c.part.holes.some((o) => o.edge == null && Math.hypot(o.x - x, o.y - y) < o.dia / 2 + 3)) return;
    ops(c).add(onEdge(c.part, { x: half(x), y: half(y), dia: st.spec.dia, depth: Math.min(st.spec.depth, c.part.thickness - 4) }, null));
  });
}

// ---------- 3D ----------
function gl3d(host) {
  if (!st.gl) {
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(2, devicePixelRatio));
    renderer.setClearColor(0xf4f2ee);
    const scene = new THREE.Scene();
    scene.add(new THREE.HemisphereLight(0xffffff, 0x9a9488, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.4); sun.position.set(0.4, 0.7, 1); scene.add(sun);
    const camera = new THREE.PerspectiveCamera(35, 1, 1, 100000);
    const controls = new OrbitControls(camera, renderer.domElement);
    const group = new THREE.Group(); scene.add(group);
    st.gl = { renderer, scene, camera, controls, group, render: () => renderer.render(scene, camera), for: null };
    controls.addEventListener("change", st.gl.render);
  }
  if (st.gl.renderer.domElement.parentNode !== host) host.append(st.gl.renderer.domElement);
  const w = host.clientWidth || 600, hh = host.clientHeight || 400;
  st.gl.renderer.setSize(w, hh); st.gl.camera.aspect = w / hh; st.gl.camera.updateProjectionMatrix();
  return st.gl;
}

function build3d(host, ctx) {
  const p = ctx.part, G = gl3d(host), th = p.thickness;
  for (const c of [...G.group.children]) { G.group.remove(c); c.geometry?.dispose(); }
  const shape = new THREE.Shape(p.outline.map(([x, y]) => new THREE.Vector2(x, y)));
  for (const c of p.cutouts || []) {
    const hp = new THREE.Path(); hp.moveTo(c.x, c.y); hp.lineTo(c.x + c.w, c.y); hp.lineTo(c.x + c.w, c.y + c.h); hp.lineTo(c.x, c.y + c.h); hp.closePath();
    shape.holes.push(hp);
  }
  for (const o of p.holes) if (o.edge == null && o.depth === 0) { const hp = new THREE.Path(); hp.absarc(o.x, o.y, o.dia / 2, 0, Math.PI * 2, true); shape.holes.push(hp); }
  const geo = new THREE.ExtrudeGeometry(shape, { depth: th, bevelEnabled: false, curveSegments: 20 });
  const m = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ color: ctx.matOf(p.material).color, roughness: 0.75 }));
  G.group.add(m, new THREE.LineSegments(new THREE.EdgesGeometry(geo, 30), new THREE.LineBasicMaterial({ color: 0x333333 })));
  const dark = new THREE.MeshBasicMaterial({ color: 0x2a2a2a }), selM = new THREE.MeshBasicMaterial({ color: SELC }), addM = new THREE.MeshBasicMaterial({ color: ADDED });
  const { isAdded } = ops(ctx);
  for (const o of p.holes) {
    if (o.edge == null && o.depth === 0) continue;
    const mt = same(o, st.sel) ? selM : isAdded(o) ? addM : dark;
    const len = Math.min(o.depth, 400);
    const cyl = new THREE.Mesh(new THREE.CylinderGeometry(o.dia / 2, o.dia / 2, len, 20), mt);
    if (o.edge == null) {
      // drilled into face 1 (top, z = th): show its depth by sinking it in, lid just above the face
      cyl.rotation.x = Math.PI / 2; cyl.position.set(o.x, o.y, th - len / 2 + 0.3);
    } else {
      const [dx, dy] = [[0, 1], [-1, 0], [0, -1], [1, 0]][o.edge];
      cyl.position.set(o.x + (dx * (len / 2 - 0.3)), o.y + (dy * (len / 2 - 0.3)), th / 2);
      if (dx === 0) cyl.rotation.z = 0; else cyl.rotation.z = Math.PI / 2;
    }
    G.group.add(cyl);
  }
  const pk = new THREE.MeshBasicMaterial({ color: 0x555555, transparent: true, opacity: 0.55 });
  for (const k of p.pockets || []) {
    const b = new THREE.Mesh(new THREE.BoxGeometry(k.w, k.h, k.depth), pk);
    b.position.set(k.x + k.w / 2, k.y + k.h / 2, k.face === 2 ? k.depth / 2 - 0.3 : th - k.depth / 2 + 0.3);
    G.group.add(b);
  }
  const band = new THREE.MeshStandardMaterial({ color: BAND, roughness: 0.6 });
  const rect = !(p.band_segs || []).length;
  p.outline.forEach((a, i) => {
    const b = p.outline[(i + 1) % p.outline.length];
    const mx = (a[0] + b[0]) / 2, my = (a[1] + b[1]) / 2;
    const k = edgeOf(p, a, b);
    if (!(rect ? k >= 0 && p.edges[k] : (p.band_segs || []).includes(i))) return;
    const L = Math.hypot(b[0] - a[0], b[1] - a[1]), ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
    const s = new THREE.Mesh(new THREE.BoxGeometry(L, 1.5, th + 0.4), band);
    // outside of a counter-clockwise outline is to the right of the segment
    s.position.set(mx + Math.sin(ang) * 0.75, my - Math.cos(ang) * 0.75, th / 2); s.rotation.z = ang;
    G.group.add(s);
  });
  // only refit when the part changes (rebuilds after an edit keep the camera)
  if (G.for !== p.id) {
    G.for = p.id;
    const r = Math.hypot(p.w, p.h) / 2;
    G.controls.target.set(p.w / 2, p.h / 2, th / 2);
    const dir = new THREE.Vector3(0.45, 0.3, 0.85).normalize(), aspect = Math.min(1, G.camera.aspect);
    const dist = (r * 1.15) / Math.sin(THREE.MathUtils.degToRad(G.camera.fov / 2)) / aspect;
    G.camera.position.set(p.w / 2, p.h / 2, th / 2).addScaledVector(dir, dist);
    G.camera.near = r / 100; G.camera.far = r * 100; G.camera.updateProjectionMatrix();
    G.controls.update();
  }
  G.render();
}

// ---------- the view ----------
export function renderPartView(host, ctx) {
  const { h } = ctx, p = ctx.part;
  if (st.fitFor !== p.id) { st.fitFor = p.id; st.view = null; st.sel = null; if (st.gl) st.gl.for = null; }
  if (st.sel && !p.holes.some((o) => same(o, st.sel))) st.sel = null;
  if (!host.__built) {
    host.__built = true;
    host.append(
      h("div", { class: "pv-bar" },
        h("span", { class: "crumbslot" }),
        h("div", { class: "seg", id: "pv-mode" }, ...[["2d", "2D"], ["3d", "3D"]].map(([k, t]) =>
          h("button", { "data-mode": k, onclick: () => { st.mode = k; renderPartView(host, host.__ctx); } }, t))),
        h("div", { class: "seg", id: "pv-face" }, ...[[1, "Face 1"], [2, "Face 2"]].map(([k, t]) =>
          h("button", { "data-face": k, onclick: () => { st.face = k; renderPartView(host, host.__ctx); } }, t))),
        h("button", { onclick: () => { st.view = null; if (st.gl) st.gl.for = null; renderPartView(host, host.__ctx); } }, "Fit")),
      el("svg", { class: "pv-svg" }),
      h("div", { class: "pv-gl" }));
    new ResizeObserver(() => { if (!host.hidden && host.__ctx) { if (st.mode === "2d" && st.view) draw(host.querySelector(".pv-svg"), host.__ctx); else if (st.mode === "3d") build3d(host.querySelector(".pv-gl"), host.__ctx); } }).observe(host);
  }
  host.__ctx = ctx;
  if (ctx.crumbs) host.querySelector(".crumbslot").replaceChildren(ctx.crumbs);
  host.querySelectorAll("[data-mode]").forEach((b) => b.classList.toggle("on", b.dataset.mode === st.mode));
  host.querySelectorAll("[data-face]").forEach((b) => b.classList.toggle("on", +b.dataset.face === st.face));
  host.querySelector("#pv-face").hidden = st.mode !== "2d";
  const svg = host.querySelector(".pv-svg"), glh = host.querySelector(".pv-gl");
  svg.style.display = st.mode === "2d" ? "" : "none"; glh.style.display = st.mode === "3d" ? "" : "none";
  if (st.mode === "2d") {
    wire(svg, ctx);
    requestAnimationFrame(() => { if (!st.view) fit(svg, p); draw(svg, ctx); });
  } else requestAnimationFrame(() => build3d(glh, ctx));
}

// ---------- the panel ----------
export function partPanel(put, ctx) {
  const { h } = ctx, p = ctx.part, e = ctx.edit, O = ops(ctx);
  const sel = p.holes.find((o) => same(o, st.sel));
  const same_t = ctx.materials.filter((m) => Math.abs(m.thickness - p.thickness) < 0.01 || m.id === p.material);
  const num = (label, v, set) => h("label", {}, h("span", {}, label),
    h("input", { type: "number", step: "any", value: fmt(v), onchange: (ev) => { const n = parseFloat(ev.target.value); if (isFinite(n)) set(n); } }));
  const rect = !(p.band_segs || []).length;
  const edited = Object.keys(e).length > 0;
  put(
    h("div", { class: "row" }, h("h3", {}, `${p.cabinet} · ${p.name}`), h("button", { onclick: ctx.close }, "Done")),
    h("div", { class: "row pv-nav" },
      h("button", { "aria-label": "Previous part", title: "Previous part (←)", onclick: () => ctx.step(-1) }, "‹"),
      h("span", { class: "grow" }, `${fmt(p.w)} × ${fmt(p.h)} × ${fmt(p.thickness)}`),
      h("span", { class: "mute" }, `${ctx.index + 1} / ${ctx.count}`),
      h("button", { "aria-label": "Next part", title: "Next part (→)", onclick: () => ctx.step(1) }, "›")),
    h("label", {}, h("span", {}, "Material"),
      h("select", { onchange: (ev) => ctx.commit((ed) => { ed.material = ev.target.value; }) },
        same_t.map((m) => h("option", { value: m.id, selected: m.id === p.material }, m.name)))),
    h("label", {}, h("span", {}, "Grain"),
      h("div", { class: "seg" }, ...[[true, "↕"], [false, "↔"]].map(([g, t]) =>
        h("button", { class: p.grain_y === g ? "on" : "", "aria-label": g ? "Grain along the height" : "Grain along the width",
          onclick: () => ctx.commit((ed) => { ed.grain_y = g; }) }, t)))),
    rect ? h("label", {}, h("span", {}, "Band"),
      h("div", { class: "seg multi", id: "pv-band" }, ...["Bottom", "Right", "Top", "Left"].map((t, k) =>
        h("button", { class: p.edges[k] ? "on" : "", onclick: () => ctx.commit((ed) => { const b = [...p.edges]; b[k] = !b[k]; ed.edges = b; }) }, t)))) : null,
    h("div", { class: "row" }, h("h3", {}, `Bores · ${p.holes.length}`),
      h("button", { id: "pv-add", onclick: () => O.add(onEdge(p, { x: p.w / 2, y: p.h / 2, dia: st.spec.dia, depth: Math.min(st.spec.depth, p.thickness - 4) }, null)) }, "+ Bore")),
    sel ? h("div", { class: "mat", id: "pv-bore" },
      h("div", { class: "grid2" },
        num("X", sel.x, (v) => O.move(sel, onEdge(p, { ...sel, x: v }, sel.edge ?? null))),
        num("Y", sel.y, (v) => O.move(sel, onEdge(p, { ...sel, y: v }, sel.edge ?? null))),
        num("Ø", sel.dia, (v) => v > 0 && O.move(sel, { ...sel, dia: v })),
        num("Depth", sel.depth, (v) => v >= 0 && O.move(sel, { ...sel, depth: v }))),
      h("label", {}, h("span", {}, "Into"),
        h("select", { onchange: (ev) => O.move(sel, onEdge(p, sel, ev.target.value === "" ? null : +ev.target.value)) },
          [["", "Face"], ["0", "Bottom edge"], ["1", "Right edge"], ["2", "Top edge"], ["3", "Left edge"]].map(([v, t]) =>
            h("option", { value: v, selected: String(sel.edge ?? "") === v }, t)))),
      h("div", { class: "row" }, h("span", { class: "grow" }),
        h("button", { class: "danger", onclick: () => O.del(sel) }, "Delete"))) : null,
    h("dl", {},
      h("dt", {}, "Bores"), h("dd", {}, ctx.summarizeHoles(p.holes)),
      ...((p.pockets || []).length ? [h("dt", {}, "Pockets"), h("dd", {}, group(p.pockets.map((k) => `${fmt(k.w)}×${fmt(k.h)}×${fmt(k.depth)}${k.face === 2 ? " face 2" : ""}`)))] : [])),
    h("div", { class: "row" },
      h("button", { onclick: ctx.exportDxf }, "DXF"),
      h("span", { class: "grow" }),
      edited ? h("button", { class: "danger", id: "pv-reset", onclick: () => { st.sel = null; ctx.commit(null); } }, "Reset part") : null));
}

// Keys while the part view is open; true when handled.
export function partKey(e, ctx) {
  const p = ctx.part, sel = p.holes.find((o) => same(o, st.sel)), O = ops(ctx);
  if (e.key === "Escape") { if (sel) { st.sel = null; ctx.refresh(); } else ctx.close(); return true; }
  if (sel && (e.key === "Delete" || e.key === "Backspace")) { O.del(sel); return true; }
  if (e.key.startsWith("Arrow")) {
    e.preventDefault();
    if (!sel) { if (e.key === "ArrowLeft" || e.key === "ArrowRight") ctx.step(e.key === "ArrowLeft" ? -1 : 1); return true; }
    const n = e.shiftKey ? 10 : 1, [dx, dy] = { ArrowLeft: [-n, 0], ArrowRight: [n, 0], ArrowUp: [0, n], ArrowDown: [0, -n] }[e.key];
    O.move(sel, onEdge(p, { ...sel, x: sel.x + (st.face === 2 ? -dx : dx), y: sel.y + dy }, sel.edge ?? null));
    return true;
  }
  if (e.key === "f" || e.key === "F") { st.view = null; if (st.gl) st.gl.for = null; ctx.refresh(); return true; }
  if (e.key === "Delete" || e.key === "Backspace") return true; // never delete the cabinet from here
  return false;
}

export const partViewState = st;
