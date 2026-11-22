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
    let parts: Vec<&Part> = build.parts.iter().filter(|p| p.cabinet == cab.id && p.role != Role::Worktop).collect();
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
