//! Shop drawings: one SVG per cabinet with front elevation, side
//! section and plan, dimensioned in millimetres.

use crate::model::{Cabinet, Project};
use crate::parts::{Build, Part, Role};
use std::fmt::Write;

struct Svg {
    o: String,
    fs: f64,
}

impl Svg {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, class: &str) {
        let _ = write!(self.o, r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" class="{class}"/>"#);
    }
    fn poly(&mut self, pts: &[(f64, f64)], class: &str) {
        let p: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
        let _ = write!(self.o, r#"<polygon points="{}" class="{class}"/>"#, p.join(" "));
    }
    fn circle(&mut self, x: f64, y: f64, r: f64, class: &str) {
        let _ = write!(self.o, r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" class="{class}"/>"#);
    }
    fn text(&mut self, x: f64, y: f64, s: &str, anchor: &str, size: f64, rot: f64) {
        let _ = write!(
            self.o,
            r#"<text x="{x:.1}" y="{y:.1}" font-size="{size:.1}" text-anchor="{anchor}" transform="rotate({rot} {x:.1} {y:.1})">{}</text>"#,
            esc(s)
        );
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, class: &str) {
        let _ = write!(self.o, r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" class="{class}"/>"#);
    }
    /// Horizontal dimension from a to b at height y (svg units).
    fn hdim(&mut self, a: f64, b: f64, y: f64, ext_from: f64) {
        if (b - a).abs() < 0.5 {
            return;
        }
        let t = self.fs * 0.35;
        self.line(a, ext_from, a, y + t, "ext");
        self.line(b, ext_from, b, y + t, "ext");
        self.line(a, y, b, y, "dim");
        self.line(a - t, y + t, a + t, y - t, "dim");
        self.line(b - t, y + t, b + t, y - t, "dim");
        self.text((a + b) / 2.0, y - self.fs * 0.3, &fmt(b - a), "middle", self.fs, 0.0);
    }
    /// Vertical dimension from a to b at x.
    fn vdim(&mut self, a: f64, b: f64, x: f64, ext_from: f64) {
        if (b - a).abs() < 0.5 {
            return;
        }
        let t = self.fs * 0.35;
        self.line(ext_from, a, x - t, a, "ext");
        self.line(ext_from, b, x - t, b, "ext");
        self.line(x, a, x, b, "dim");
        self.line(x - t, a + t, x + t, a - t, "dim");
        self.line(x - t, b + t, x + t, b - t, "dim");
        self.text(x - self.fs * 0.3, (a + b) / 2.0, &fmt((b - a).abs()), "middle", self.fs, -90.0);
    }
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn class(p: &Part) -> &'static str {
    match p.role {
        Role::Front => "front",
        Role::Back => "back",
        Role::Shelf => "shelf",
        Role::DrawerBox => "drawer",
        Role::Worktop => "top",
        Role::Carcass => "carcass",
    }
}

/// Map a hole on a front piece (drawn inside face up) to the cabinet's
/// front view (x from the cabinet's left, y up).
fn front_hole(p: &Part, hx: f64, hy: f64) -> (f64, f64) {
    let [x0, y0, _] = p.box3d.pos;
    let [sx, sy, _] = p.box3d.size;
    let ox = (p.w - sx) / 2.0;
    let oy = (p.h - sy) / 2.0;
    (x0 + sx - (hx - ox), y0 + (hy - oy))
}

pub fn cabinet_svg(project: &Project, build: &Build, cab: &Cabinet) -> String {
    // run parts (shared plinths) span other cabinets: not in a single cabinet's drawing
    let parts: Vec<&Part> = build.parts.iter().filter(|p| p.cabinet == cab.id && p.role != Role::Worktop && p.shared.is_empty()).collect();
    let (w, h, d) = (cab.width, cab.height, cab.depth);
    let ft = parts.iter().filter(|p| p.role == Role::Front).map(|p| p.thickness).fold(0.0, f64::max);
    let dd = d + ft; // depth including fronts
    let pd = if cab.is_l() { dd.max(cab.width2) } else { dd }; // plan extent: an L's return arm
    let fs = (w.max(h) / 38.0).clamp(14.0, 40.0);
    let m = fs * 6.0; // margin for dimensions
    let gap = fs * 9.0;
    let title_h = fs * 4.5;
    let (fx, fy) = (m, m + title_h); // front view top-left
    let (sx, sy) = (fx + w + gap, fy); // side view top-left
    let (px, py) = (fx, fy + h + gap); // plan top-left
    let cw = sx + pd + m;
    let ch = py + pd + m;

    let mut s = Svg { o: String::new(), fs };
    let _ = write!(
        s.o,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {cw:.0} {ch:.0}" width="{:.0}mm" height="{:.0}mm" font-family="sans-serif">
<style>
rect,polygon{{stroke:#222;stroke-width:{lw:.2}}}
.carcass{{fill:#f2f0ea}}.back{{fill:#e4e1d9}}.shelf{{fill:#ebe8e0}}.drawer{{fill:#efe2c4}}.front{{fill:#d9c09c}}.recess{{fill:#cdb18b}}.top{{fill:#b8936b}}
.ghost{{fill:none;stroke:#999;stroke-dasharray:{dash:.1}}}
.cup{{fill:none;stroke:#1667c4;stroke-dasharray:{dash:.1}}}.hole{{fill:#222;stroke:none}}
.dim{{stroke:#1d5fbf;stroke-width:{lw:.2}}}.ext{{stroke:#1d5fbf;stroke-width:{thin:.2};opacity:.6}}
text{{fill:#1f1d1a}}.ttl{{font-weight:700}}
</style>"##,
        cw,
        ch,
        lw = fs / 12.0,
        thin = fs / 20.0,
        dash = fs / 3.0
    );
    let mat = |id: &str| project.material(id).name;
    s.text(m, m, &format!("{} · {}", cab.id, cab.name), "start", fs * 1.4, 0.0);
    s.text(
        m,
        m + fs * 1.8,
        &format!(
            "{:.0} × {:.0} × {:.0} · {:?} · carcass {} · fronts {}",
            w,
            h,
            d,
            cab.kind,
            mat(&cab.carcass_material),
            mat(&cab.front_material)
        ),
        "start",
        fs * 0.85,
        0.0,
    );

    // ---------- front elevation (x right, y up) ----------
    let fy0 = fy + h; // svg y of floor line
    let fyv = |y: f64| fy0 - y;
    for p in parts.iter().filter(|p| p.role == Role::Carcass) {
        let [x, y, _] = p.box3d.pos;
        let [bx, by, _] = p.box3d.size;
        s.rect(fx + x, fyv(y + by), bx, by, "carcass");
    }
    let fronts: Vec<&&Part> = parts.iter().filter(|p| p.role == Role::Front).collect();
    for p in &fronts {
        let [x, y, _] = p.box3d.pos;
        let [bx, by, _] = p.box3d.size;
        s.rect(fx + x, fyv(y + by), bx, by, "front");
    }
    // Routed Shaker recesses (outside face)
    for p in &fronts {
        for k in p.pockets.iter().filter(|k| k.face == 2) {
            let (xa, ya) = front_hole(p, k.x, k.y);
            let (xb, yb) = front_hole(p, k.x + k.w, k.y + k.h);
            s.rect(fx + xa.min(xb), fyv(ya.max(yb)), (xb - xa).abs(), (yb - ya).abs(), "recess");
        }
    }
    for p in &fronts {
        for o in p.holes.iter().filter(|o| o.edge.is_none()) {
            let (x, y) = front_hole(p, o.x, o.y);
            if o.dia >= 20.0 {
                s.circle(fx + x, fyv(y), o.dia / 2.0, "cup");
            } else {
                s.circle(fx + x, fyv(y), (o.dia * 0.8).max(fs * 0.18), "hole");
            }
        }
    }
    s.text(fx + w / 2.0, fy - fs * 0.8, "FRONT", "middle", fs * 0.9, 0.0);
    // overall width below, height left
    s.hdim(fx, fx + w, fy0 + fs * 2.5, fy0 + fs * 0.4);
    s.vdim(fy0, fy, fx - fs * 2.5, fx - fs * 0.4);
    // chain of front heights on the right
    let mut ys: Vec<f64> = vec![];
    // whole fronts only: Shaker rails and panels would clutter the chain
    for p in fronts.iter().filter(|p| !p.name.ends_with("rail B") && !p.name.ends_with("rail T") && !p.name.ends_with("panel")) {
        let y = p.box3d.pos[1];
        let top = y + p.box3d.size[1];
        for v in [y, top] {
            if !ys.iter().any(|q| (q - v).abs() < 0.5) {
                ys.push(v);
            }
        }
    }
    ys.sort_by(|a, b| a.total_cmp(b));
    let toe = parts.iter().find(|p| p.id.ends_with("-bottom")).map(|p| p.box3d.pos[1]).unwrap_or(0.0);
    if toe > 0.0 && !ys.iter().any(|q| q.abs() < 0.5) {
        ys.insert(0, 0.0);
    }
    for pair in ys.windows(2) {
        // only dimension slots of real fronts / toe kick, skip 3 mm gaps
        if pair[1] - pair[0] > 10.0 {
            s.vdim(fyv(pair[0]), fyv(pair[1]), fx + w + fs * 2.5, fx + w + fs * 0.4);
        }
    }

    // ---------- side section (front on the left) ----------
    let u = |z: f64| sx + dd - z; // z (cabinet local) -> svg x
    let sy0 = sy + h;
    let syv = |y: f64| sy0 - y;
    if let Some(side) = parts.iter().find(|p| p.id.ends_with("-side-l")) {
        // side drawing: x from carcass front, y up; carcass front is at z = d
        let y0 = side.box3d.pos[1]; // sides on legs start above the floor
        let pts: Vec<(f64, f64)> = side.outline.iter().map(|q| (u(d - q[0]), syv(q[1] + y0))).collect();
        s.poly(&pts, "ghost");
    }
    // off-centre so the cut passes through a leaf of a double door
    let mid = w * 0.35;
    let mut cut: Vec<&&Part> = parts
        .iter()
        .filter(|p| {
            let x0 = p.box3d.pos[0];
            let x1 = x0 + p.box3d.size[0];
            x0 <= mid && mid <= x1
        })
        .collect();
    // draw large parts first so drawer boxes and fronts stay visible
    cut.sort_by_key(|p| match p.role {
        Role::Back | Role::Carcass => 0,
        Role::Shelf => 1,
        Role::DrawerBox => 2,
        _ => 3,
    });
    for p in &cut {
        let [_, y, z] = p.box3d.pos;
        let [_, by, bz] = p.box3d.size;
        s.rect(u(z + bz), syv(y + by), bz, by, class(p));
    }
    // drawer box sides sit beside the section plane: show their outline
    for p in parts.iter().filter(|p| p.role == Role::DrawerBox && p.name.ends_with("side")) {
        let [_, y, z] = p.box3d.pos;
        let [_, by, bz] = p.box3d.size;
        s.rect(u(z + bz), syv(y + by), bz, by, "ghost");
    }
    s.text(sx + dd / 2.0, sy - fs * 0.8, "SECTION", "middle", fs * 0.9, 0.0);
    s.hdim(u(d), u(0.0), sy0 + fs * 2.5, sy0 + fs * 0.4);
    if ft > 0.0 {
        s.hdim(u(d + ft), u(d), sy0 + fs * 4.5, sy0 + fs * 0.4);
    }
    // shelf positions (top of shelf from floor)
    let mut shelf_y: Vec<f64> = parts.iter().filter(|p| p.role == Role::Shelf).map(|p| p.box3d.pos[1] + p.box3d.size[1]).collect();
    shelf_y.sort_by(|a, b| a.total_cmp(b));
    let mut prev = toe + parts.iter().find(|p| p.id.ends_with("-bottom")).map(|p| p.box3d.size[1]).unwrap_or(0.0);
    for y in shelf_y {
        s.vdim(syv(prev), syv(y), sx + dd + fs * 2.5, u(0.0) + fs * 0.4);
        prev = y;
    }

    // ---------- plan (x right, front at the bottom) ----------
    let pv = |z: f64| py + z;
    for p in parts.iter().filter(|p| p.role != Role::Front) {
        for b in std::iter::once(&p.box3d).chain(p.more3d.iter()) {
            let [x, _, z] = b.pos;
            let [bx, _, bz] = b.size;
            s.rect(px + x, pv(z), bx, bz, if p.role == Role::Carcass || p.role == Role::Back { "carcass" } else { "ghost" });
        }
    }
    for p in &fronts {
        let [x, _, z] = p.box3d.pos;
        let [bx, _, bz] = p.box3d.size;
        s.rect(px + x, pv(z), bx, bz, "front");
    }
    s.text(px + w / 2.0, py - fs * 0.8, "PLAN", "middle", fs * 0.9, 0.0);
    s.hdim(px, px + w, pv(pd) + fs * 2.5, pv(pd) + fs * 0.4);
    s.vdim(pv(pd), pv(0.0), px - fs * 2.5, px - fs * 0.4);

    s.o.push_str("</svg>");
    s.o
}

/// Installer's elevation of one wall, seen from inside the room: every
/// carcass, front and worktop within 700 mm of the wall projected onto it,
/// with doors and windows, and dimension chains for cabinet widths,
/// openings and heights. `None` when the room has no such wall.
pub fn wall_svg(project: &Project, build: &Build, wi: usize) -> Option<String> {
    let room = &project.room;
    let w = room.wall(wi)?;
    let len = w.len;
    let rh = room.height.max(1000.0);
    let fs = (len.max(rh) / 45.0).clamp(14.0, 50.0);
    let m = fs * 7.0;
    let title_h = fs * 4.0;
    let (ox, oy) = (m, m + title_h + fs * 5.0); // wall's top-left corner in svg
    let cw = ox + len + m;
    let ch = oy + rh + fs * 13.0;
    let tx = |t: f64| ox + t;
    let vy = |y: f64| oy + rh - y;
    let reach = 700.0;

    // Project a cabinet-local box onto the wall: (t0, t1, y0, y1, s_near).
    let project_box = |c: &Cabinet, b: &crate::parts::Box3, rot: f64| {
        let [x0, y0, z0] = b.pos;
        let [sx, sy, sz] = b.size;
        let mut t = (f64::MAX, f64::MIN);
        let mut s = (f64::MAX, f64::MIN);
        let (rs, rc) = rot.to_radians().sin_cos();
        for (dx, dz) in [(0.0, 0.0), (sx, 0.0), (0.0, sz), (sx, sz)] {
            let (x, z) = (x0 + dx * rc + dz * rs, z0 - dx * rs + dz * rc);
            let (wx, wz) = crate::parts::to_world(c.x, c.z, c.rotation, x, z);
            let (tt, ss) = w.local(wx, wz);
            t = (t.0.min(tt), t.1.max(tt));
            s = (s.0.min(ss), s.1.max(ss));
        }
        (t.0, t.1, c.elevation + y0, c.elevation + y0 + sy, s.0, s.1)
    };
    let near = |t0: f64, t1: f64, s0: f64, s1: f64| s0 < reach && s1 > -10.0 && t1 > 1.0 && t0 < len - 1.0;
    let aligned = |c: &Cabinet| {
        let dr = (c.rotation - w.rotation).rem_euclid(360.0);
        let (t, s) = w.local(c.x, c.z);
        (dr < 0.5 || dr > 359.5) && s.abs() < 50.0 && t > -1.0 && t < len + 1.0
    };

    // items: (s_near, class, t0, t1, y0, y1)
    let mut items: Vec<(f64, &str, f64, f64, f64, f64)> = vec![];
    for c in &project.cabinets {
        for r in c.rects() {
            let b = crate::parts::Box3 { pos: [r[0], 0.0, r[1]], size: [r[2] - r[0], c.height, r[3] - r[1]] };
            let (t0, t1, y0, y1, s0, s1) = project_box(c, &b, 0.0);
            if near(t0, t1, s0, s1) {
                let cls = if c.kind.is_carcass() { "carcass" } else { "ghost" };
                items.push((s1 - 30.0, cls, t0, t1, y0, y1));
            }
        }
    }
    for p in build.parts.iter().filter(|p| matches!(p.role, Role::Front | Role::Worktop)) {
        let Some(c) = project.cabinets.iter().find(|c| c.id == p.cabinet) else { continue };
        for b in std::iter::once(&p.box3d).chain(p.more3d.iter()) {
            let (t0, t1, y0, y1, s0, s1) = project_box(c, b, p.rot);
            if near(t0, t1, s0, s1) {
                items.push((s1, class(p), t0, t1, y0, y1));
            }
        }
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut s = Svg { o: String::new(), fs };
    let _ = write!(
        s.o,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {cw:.0} {ch:.0}" width="{:.0}mm" height="{:.0}mm" font-family="sans-serif">
<style>
rect,polygon{{stroke:#222;stroke-width:{lw:.2}}}
.wall{{fill:#fafaf8}}.carcass{{fill:#f2f0ea}}.front{{fill:#d9c09c}}.top{{fill:#b8936b}}.back,.shelf,.drawer{{fill:none}}
.ghost{{fill:none;stroke:#999;stroke-dasharray:{dash:.1}}}.opening{{fill:#e8f0f7;stroke:#6b8fb3;stroke-dasharray:{dash:.1}}}
.dim{{stroke:#1d5fbf;stroke-width:{lw:.2}}}.ext{{stroke:#1d5fbf;stroke-width:{thin:.2};opacity:.6}}
text{{fill:#1f1d1a}}
</style>"##,
        cw / 10.0,
        ch / 10.0,
        lw = fs / 12.0,
        thin = fs / 20.0,
        dash = fs / 3.0
    );
    s.text(m, m, &format!("Wall {} · {} mm", wi + 1, fmt(len)), "start", fs * 1.4, 0.0);
    s.rect(tx(0.0), vy(rh), len, rh, "wall");
    for o in room.openings.iter().filter(|o| o.wall == wi) {
        s.rect(tx(o.offset), vy(o.sill + o.height), o.width, o.height, "opening");
        let sill = if o.sill > 0.0 { format!(", sill {}", fmt(o.sill)) } else { String::new() };
        let label = format!("{:?} {}×{}{}", o.kind, fmt(o.width), fmt(o.height), sill);
        s.text(tx(o.offset + o.width / 2.0), vy(o.sill + o.height / 2.0), &label, "middle", fs * 0.8, 0.0);
    }
    for &(_, cls, t0, t1, y0, y1) in &items {
        let (a, b) = (t0.max(0.0), t1.min(len));
        if b - a > 0.5 {
            s.rect(tx(a), vy(y1), b - a, y1 - y0, cls);
        }
    }
    // cabinet labels on the aligned cabinets
    for c in project.cabinets.iter().filter(|c| aligned(c)) {
        let (t, _) = w.local(c.x, c.z);
        s.text(tx(t + c.width / 2.0), vy(c.elevation + c.height) + fs * 1.3, &c.id, "middle", fs * 0.8, 0.0);
    }

    // ---------- dimension chains ----------
    let chain = |s: &mut Svg, mut edges: Vec<f64>, y: f64, from: f64| {
        edges.retain(|e| *e > -0.5 && *e < len + 0.5);
        edges.push(0.0);
        edges.push(len);
        edges.sort_by(|a, b| a.total_cmp(b));
        edges.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        for p in edges.windows(2) {
            s.hdim(tx(p[0]), tx(p[1]), y, from);
        }
    };
    let edges_of = |low: bool| -> Vec<f64> {
        project
            .cabinets
            .iter()
            .filter(|c| aligned(c) && ((c.elevation < 500.0) == low))
            .flat_map(|c| {
                let (t, _) = w.local(c.x, c.z);
                [t, t + c.width]
            })
            .collect()
    };
    // base run below the floor, openings below that, overall last
    chain(&mut s, edges_of(true), vy(0.0) + fs * 2.5, vy(0.0) + fs * 0.4);
    let ops: Vec<f64> = room.openings.iter().filter(|o| o.wall == wi).flat_map(|o| [o.offset, o.offset + o.width]).collect();
    if !ops.is_empty() {
        chain(&mut s, ops, vy(0.0) + fs * 5.5, vy(0.0) + fs * 0.4);
    }
    s.hdim(tx(0.0), tx(len), vy(0.0) + fs * 8.5, vy(0.0) + fs * 0.4);
    // wall units above the ceiling line
    let up = edges_of(false);
    if !up.is_empty() {
        chain(&mut s, up, vy(rh) - fs * 2.5, vy(rh) - fs * 0.4);
    }
    // heights on the right: floor, worktop, wall-unit bottoms and tops, ceiling
    let mut ys: Vec<f64> = vec![0.0, rh];
    for &(_, cls, _, _, y0, y1) in &items {
        if cls == "top" || cls == "carcass" {
            ys.push(y0);
            ys.push(y1);
        }
    }
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup_by(|a, b| (*a - *b).abs() < 5.0);
    for p in ys.windows(2) {
        s.vdim(vy(p[1]), vy(p[0]), tx(len) + fs * 3.0, tx(len) + fs * 0.4);
    }
    s.o.push_str("</svg>");
    Some(s.o)
}

/// Floor plan: room outline with doors and windows, every cabinet's
/// footprint (wall units dashed) with its id, and each wall's length.
/// `None` without a room.
pub fn plan_svg(project: &Project) -> Option<String> {
    let room = &project.room;
    let walls = room.walls();
    if walls.len() < 3 {
        return None;
    }
    let (mut x0, mut z0, mut x1, mut z1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &room.walls {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
        z0 = z0.min(p[1]);
        z1 = z1.max(p[1]);
    }
    let span = (x1 - x0).max(z1 - z0);
    let fs = (span / 45.0).clamp(14.0, 60.0);
    let th = room.thickness.max(0.0);
    let m = fs * 7.0 + th;
    let title_h = fs * 3.0;
    let (ox, oy) = (m - x0, m + title_h - z0);
    let cw = x1 - x0 + 2.0 * m;
    let ch = z1 - z0 + 2.0 * m + title_h;
    let px = |x: f64| ox + x;
    let pz = |z: f64| oy + z;

    let mut s = Svg { o: String::new(), fs };
    let _ = write!(
        s.o,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {cw:.0} {ch:.0}" width="{:.0}mm" height="{:.0}mm" font-family="sans-serif">
<style>
rect,polygon{{stroke:#222;stroke-width:{lw:.2}}}
.wallband{{fill:#cfcac0;stroke:none}}.floor{{fill:#fafaf8}}.base{{fill:#e9dcc6}}.upper{{fill:none;stroke:#555;stroke-dasharray:{dash:.1}}}.tall{{fill:#d9c09c}}
.door{{fill:#fafaf8;stroke:none}}.swing{{fill:none;stroke:#6b8fb3;stroke-width:{lw:.2}}}.window{{fill:#e8f0f7;stroke:#6b8fb3}}
.dim{{stroke:#1d5fbf;stroke-width:{lw:.2}}}.ext{{stroke:#1d5fbf;stroke-width:{thin:.2};opacity:.6}}
text{{fill:#1f1d1a}}
</style>"##,
        cw / 20.0,
        ch / 20.0,
        lw = fs / 12.0,
        thin = fs / 20.0,
        dash = fs / 3.0
    );
    s.text(fs * 2.0, fs * 2.5, &format!("{} · plan", project.name), "start", fs * 1.4, 0.0);
    // walls: a band of the wall thickness outside the inside outline
    for w in &walls {
        let (a0, a1) = (w.point(0.0, 0.0), w.point(w.len, 0.0));
        let (b1, b0) = (w.point(w.len + th, -th), w.point(-th, -th));
        s.poly(&[(px(a0.0), pz(a0.1)), (px(a1.0), pz(a1.1)), (px(b1.0), pz(b1.1)), (px(b0.0), pz(b0.1))], "wallband");
    }
    let floor: Vec<(f64, f64)> = room.walls.iter().map(|p| (px(p[0]), pz(p[1]))).collect();
    s.poly(&floor, "floor");
    for o in &room.openings {
        let Some(w) = room.wall(o.wall) else { continue };
        let q = |t: f64, n: f64| {
            let (x, z) = w.point(t, n);
            (px(x), pz(z))
        };
        let pts = [q(o.offset, 0.5), q(o.offset + o.width, 0.5), q(o.offset + o.width, -th - 0.5), q(o.offset, -th - 0.5)];
        if o.sill > 0.0 {
            s.poly(&pts, "window");
        } else {
            s.poly(&pts, "door");
            // swing: leaf from the hinge (opening start) into the room, quarter arc
            let (hx, hy) = q(o.offset, 0.0);
            let (lx, ly) = q(o.offset, o.width);
            let (ex, ey) = q(o.offset + o.width, 0.0);
            s.line(hx, hy, lx, ly, "swing");
            let sweep = if (lx - hx) * (ey - hy) - (ly - hy) * (ex - hx) > 0.0 { 1 } else { 0 };
            let _ = write!(s.o, r#"<path d="M{lx:.1} {ly:.1} A{r:.1} {r:.1} 0 0 {sweep} {ex:.1} {ey:.1}" class="swing"/>"#, r = o.width);
        }
    }
    // cabinets: floor-standing first, wall units dashed on top
    let mut cabs: Vec<&Cabinet> = project.cabinets.iter().collect();
    cabs.sort_by(|a, b| (a.elevation > 0.0).cmp(&(b.elevation > 0.0)));
    for c in cabs {
        let cls = if c.elevation > 0.0 {
            "upper"
        } else if c.height > 1200.0 {
            "tall"
        } else {
            "base"
        };
        for r in c.rects() {
            let pts: Vec<(f64, f64)> = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])]
                .iter()
                .map(|&(x, z)| {
                    let (wx, wz) = crate::parts::to_world(c.x, c.z, c.rotation, x, z);
                    (px(wx), pz(wz))
                })
                .collect();
            s.poly(&pts, cls);
        }
        let (cx, cz) = crate::parts::to_world(c.x, c.z, c.rotation, c.width / 2.0, c.depth / 2.0);
        let dy = if c.elevation > 0.0 { -fs * 0.6 } else { fs * 0.4 };
        s.text(px(cx), pz(cz) + dy, &c.id, "middle", fs * 0.8, 0.0);
    }
    // wall lengths, outside each wall
    for w in &walls {
        let off = th + fs * 2.5;
        let (a, b) = (w.point(0.0, -off), w.point(w.len, -off));
        let (a, b) = ((px(a.0), pz(a.1)), (px(b.0), pz(b.1)));
        let (e0, e1) = (w.point(0.0, -th), w.point(w.len, -th));
        s.line(px(e0.0), pz(e0.1), a.0, a.1, "ext");
        s.line(px(e1.0), pz(e1.1), b.0, b.1, "ext");
        s.line(a.0, a.1, b.0, b.1, "dim");
        let t = fs * 0.35;
        for p in [a, b] {
            s.line(p.0 - t, p.1 + t, p.0 + t, p.1 - t, "dim");
        }
        let mut ang = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
        // keep text upright; flipped text hangs below its baseline, so move it out past the line
        let flip = ang > 90.0 || ang < -90.0;
        if flip {
            ang += 180.0;
        }
        let mid = w.point(w.len / 2.0, -off - if flip { fs * 1.1 } else { fs * 0.4 });
        s.text(px(mid.0), pz(mid.1), &fmt(w.len), "middle", fs, ang);
    }
    s.o.push_str("</svg>");
    Some(s.o)
}
