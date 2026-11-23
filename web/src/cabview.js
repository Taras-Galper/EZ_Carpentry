// Cabinet view: one cabinet on its own in 3D, its parts listed beside it.
// Hover a part (in 3D or the list) to find it, pull the parts apart with
// Explode, click a part to open it in the part view.
//
// main.js passes a context:
//   { cab, parts, partObject(p), partSvg(p, size), matOf, h, crumbs, edited(id), openPart(id) }
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

const st = { explode: 0, hover: null, gl: null, for: null, objs: new Map() };
const hoverMat = new THREE.MeshStandardMaterial({ color: 0x2f6fe4, roughness: 0.6, emissive: 0x0b2a66 });
const fmt = (v) => String(Math.round(v * 10) / 10);

function gl3d(host) {
  if (!st.gl) {
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(2, devicePixelRatio));
    renderer.setClearColor(0xf4f2ee);
    const scene = new THREE.Scene();
    scene.add(new THREE.HemisphereLight(0xffffff, 0x9a9488, 1.5));
    const sun = new THREE.DirectionalLight(0xffffff, 1.5); sun.position.set(0.5, 1, 0.8); scene.add(sun);
    const camera = new THREE.PerspectiveCamera(35, 1, 1, 100000);
    const controls = new OrbitControls(camera, renderer.domElement);
    const group = new THREE.Group(); scene.add(group);
    st.gl = { renderer, scene, camera, controls, group, render: () => renderer.render(scene, camera) };
    controls.addEventListener("change", st.gl.render);
  }
  if (st.gl.renderer.domElement.parentNode !== host) host.prepend(st.gl.renderer.domElement);
  const w = host.clientWidth || 600, h = host.clientHeight || 400;
  st.gl.renderer.setSize(w, h); st.gl.camera.aspect = w / h; st.gl.camera.updateProjectionMatrix();
  return st.gl;
}

// Centre of a part in the cabinet frame (its first box, or its plan shape).
function centre(p) {
  const [x, y, z] = p.box3d.pos, [sx, sy, sz] = p.box3d.size;
  if (p.shape?.length >= 3) {
    const xs = p.shape.map((q) => q[0]), zs = p.shape.map((q) => q[1]);
    return new THREE.Vector3((Math.min(...xs) + Math.max(...xs)) / 2, y + sy / 2, (Math.min(...zs) + Math.max(...zs)) / 2);
  }
  return new THREE.Vector3(x + sx / 2, y + sy / 2, z + sz / 2);
}

function place() {
  const G = st.gl; if (!G) return;
  for (const { obj, off } of st.objs.values()) obj.position.copy(off).multiplyScalar(st.explode);
  G.render();
}

function setHover(id, host) {
  if (st.hover === id) return;
  for (const [pid, { obj }] of st.objs)
    obj.traverse((m) => { if (m.isMesh && m.userData.part) m.material = pid === id ? hoverMat : m.userData.mat; });
  st.hover = id;
  host?.querySelectorAll(".cv-list li").forEach((li) => li.classList.toggle("hot", li.dataset.id === id));
  st.gl?.render();
}

function build(host, ctx) {
  const G = gl3d(host);
  for (const c of [...G.group.children]) G.group.remove(c);
  st.objs.clear(); st.hover = null;
  const box = new THREE.Box3();
  for (const p of ctx.parts) box.expandByPoint(centre(p));
  const objs = ctx.parts.map((p) => {
    const obj = ctx.partObject(p);
    obj.traverse((m) => { if (m.isMesh && m.userData.part) m.userData.mat = m.material; });
    G.group.add(obj);
    return [p, obj];
  });
  const all = new THREE.Box3().setFromObject(G.group), C = box.getCenter(new THREE.Vector3());
  for (const [p, obj] of objs) st.objs.set(p.id, { obj, off: centre(p).sub(C) });
  place();
  if (st.for !== ctx.cab.id) {
    st.for = ctx.cab.id;
    const sph = all.getBoundingSphere(new THREE.Sphere()), r = sph.radius * 1.35;
    const dir = new THREE.Vector3(0.55, 0.4, 1).normalize();
    const dist = r / Math.sin(THREE.MathUtils.degToRad(G.camera.fov / 2)) / Math.min(1, G.camera.aspect);
    G.controls.target.copy(sph.center);
    G.camera.position.copy(sph.center).addScaledVector(dir, dist);
    G.camera.near = r / 50; G.camera.far = r * 100; G.camera.updateProjectionMatrix();
    G.controls.update();
  }
  G.render();
}

function wire(glh, host) {
  if (glh.__wired) return;
  glh.__wired = true;
  const ray = new THREE.Raycaster(), ptr = new THREE.Vector2(), tip = glh.querySelector(".cv-tip");
  const pick = (e) => {
    const r = glh.getBoundingClientRect();
    ptr.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    st.gl.scene.updateMatrixWorld();
    ray.setFromCamera(ptr, st.gl.camera);
    return ray.intersectObjects(st.gl.group.children, true).find((x) => x.object.isMesh && x.object.userData.part)?.object.userData.part || null;
  };
  let down = null;
  glh.addEventListener("pointermove", (e) => {
    if (e.buttons) { tip.hidden = true; return; }
    const id = pick(e), ctx = host.__ctx;
    setHover(id, host);
    const p = id && ctx.parts.find((q) => q.id === id);
    tip.hidden = !p;
    if (p) {
      const r = glh.getBoundingClientRect();
      tip.textContent = `${p.name} · ${fmt(p.w)} × ${fmt(p.h)} × ${fmt(p.thickness)}`;
      tip.style.left = e.clientX - r.left + 14 + "px"; tip.style.top = e.clientY - r.top + 14 + "px";
    }
  });
  glh.addEventListener("pointerleave", () => { tip.hidden = true; setHover(null, host); });
  glh.addEventListener("pointerdown", (e) => { down = [e.clientX, e.clientY]; });
  glh.addEventListener("pointerup", (e) => {
    if (!down || Math.hypot(e.clientX - down[0], e.clientY - down[1]) > 4) { down = null; return; }
    down = null;
    const id = pick(e);
    if (id) host.__ctx.openPart(id);
  });
}

export function renderCabView(host, ctx) {
  const { h } = ctx;
  if (!host.__built) {
    host.__built = true;
    const slider = h("input", { type: "range", min: 0, max: 1.2, step: 0.01, value: st.explode, "aria-label": "Explode",
      oninput: (e) => { st.explode = +e.target.value; place(); } });
    host.append(
      h("div", { class: "cv-gl" },
        h("div", { class: "cv-bar" }, h("span", { class: "crumbslot" }),
          h("label", { class: "cv-explode" }, h("span", {}, "Explode"), slider)),
        h("div", { class: "cv-tip", hidden: true })),
      h("ul", { class: "cv-list" }));
    new ResizeObserver(() => { if (!host.hidden && host.__ctx) build(host.querySelector(".cv-gl"), host.__ctx); }).observe(host.querySelector(".cv-gl"));
  }
  host.__ctx = ctx;
  host.querySelector(".crumbslot").replaceChildren(ctx.crumbs);
  const list = host.querySelector(".cv-list");
  list.replaceChildren(
    h("li", { class: "cv-head" }, h("b", {}, `Parts · ${ctx.parts.length}`)),
    ...ctx.parts.map((p) => h("li", {
      "data-id": p.id, tabindex: 0, class: st.hover === p.id ? "hot" : "",
      onclick: () => ctx.openPart(p.id), onkeydown: (e) => { if (e.key === "Enter") ctx.openPart(p.id); },
      onmouseenter: () => setHover(p.id, host), onmouseleave: () => setHover(null, host),
    },
    h("span", { class: "cv-thumb" }, ctx.partSvg(p, 56)),
    h("span", { class: "cv-name" }, h("b", {}, p.name, ctx.edited(p.id) ? h("span", { class: "edited", title: "Edited" }, " ✎") : null),
      h("small", {}, `${fmt(p.w)} × ${fmt(p.h)} × ${fmt(p.thickness)} · ${ctx.matOf(p.material).name}`)),
    h("span", { class: "cv-go", "aria-hidden": "true" }, "›"))));
  const glh = host.querySelector(".cv-gl");
  wire(glh, host);
  requestAnimationFrame(() => build(glh, ctx));
}

export const cabViewState = st;
