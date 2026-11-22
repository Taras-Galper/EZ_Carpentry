//! Turns parametric cabinets into flat, machinable parts.
//!
//! Construction: frameless (European / System 32), applied back,
//! full-overlay fronts. Each part is drawn in its own 2D frame
//! (x = drawing width, y = drawing height, inside face up) and also
//! carries a 3D box in cabinet-local space for the viewer.
//!
//! Cabinet-local space: x to the right, y up, z towards the front
//! (z = 0 at the back face, z = depth at the carcass front).

use crate::model::*;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Carcass,
    Back,
    Shelf,
    Front,
    DrawerBox,
    Worktop,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hole {
    /// Face bores: centre on the face. Edge bores: point on the edge.
    pub x: f64,
    pub y: f64,
    pub dia: f64,
    /// 0 means through.
    pub depth: f64,
    /// Edge bore drilled horizontally into this edge (0 bottom, 1 right,
    /// 2 top, 3 left in the drawing frame), centred in the thickness.
    /// None = vertical bore into the drawn face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<u8>,
}

/// Rectangular pocket or groove (a groove is a long thin pocket and may
/// run out through the part's edges).
#[derive(Debug, Clone, Serialize)]
pub struct Pocket {
    /// Lower-left corner and size in the drawing frame.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub depth: f64,
    /// Corner radius (0 = square, left to the cutter).
    pub r: f64,
    /// 1 = the drawn face, 2 = the opposite face (part is flipped).
    pub face: u8,
}

impl Pocket {
    pub fn as_cutout(&self) -> Cutout {
        Cutout { kind: CutoutKind::None, x: self.x, y: self.y, w: self.w, h: self.h, r: self.r }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Box3 {
    pub pos: [f64; 3],
    pub size: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub id: String,
    pub cabinet: String,
    pub name: String,
    pub role: Role,
    pub material: String,
    pub thickness: f64,
    /// Finished size in the drawing frame.
    pub w: f64,
    pub h: f64,
    /// Grain runs along drawing y.
    pub grain_y: bool,
    /// Edge banding: [bottom, right, top, left] in the drawing frame.
    pub edges: [bool; 4],
    /// Closed outline, counter-clockwise, drawing frame.
    pub outline: Vec<[f64; 2]>,
    pub holes: Vec<Hole>,
    /// Through cut-outs inside the outline (worktop sink / hob).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cutouts: Vec<Cutout>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pockets: Vec<Pocket>,
    pub box3d: Box3,
}

/// Rounded-rectangle through cut-out, drawing frame.
#[derive(Debug, Clone, Serialize)]
pub struct Cutout {
    pub kind: CutoutKind,
    /// Lower-left corner and size.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub r: f64,
}

impl Cutout {
    /// Closed contour as (x, y, bulge) vertices, counter-clockwise.
    /// Bulge = tan(angle / 4) on the segment leaving each vertex.
    pub fn contour(&self) -> Vec<(f64, f64, f64)> {
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        let r = self.r.clamp(0.0, self.w.min(self.h) / 2.0);
        if r <= 0.01 {
            return vec![(x0, y0, 0.0), (x1, y0, 0.0), (x1, y1, 0.0), (x0, y1, 0.0)];
        }
        let b = (std::f64::consts::PI / 8.0).tan(); // 90° arc
        vec![
            (x0 + r, y0, 0.0),
            (x1 - r, y0, b),
            (x1, y0 + r, 0.0),
            (x1, y1 - r, b),
            (x1 - r, y1, 0.0),
            (x0 + r, y1, b),
            (x0, y1 - r, 0.0),
            (x0, y0 + r, b),
        ]
    }

    /// Polyline approximation (for SVG / previews).
    pub fn points(&self, seg: usize) -> Vec<(f64, f64)> {
        let r = self.r.clamp(0.0, self.w.min(self.h) / 2.0);
        if r <= 0.01 {
            return self.contour().iter().map(|p| (p.0, p.1)).collect();
        }
        let c = [
            (self.x + self.w - r, self.y + r, -90.0),
            (self.x + self.w - r, self.y + self.h - r, 0.0),
            (self.x + r, self.y + self.h - r, 90.0),
            (self.x + r, self.y + r, 180.0),
        ];
        let mut out = vec![];
        for (cx, cy, a0) in c {
            for i in 0..=seg {
                let a = (a0 + 90.0 * i as f64 / seg as f64).to_radians();
                out.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        }
        out
    }
}

impl Part {
    /// Index of this part in the build (parts are unique by id).
    pub fn id_index(&self, b: &Build) -> usize {
        b.parts.iter().position(|q| q.id == self.id).unwrap_or(0)
    }

    /// Size to cut before edge banding is applied.
    pub fn cut_size(&self, band: f64) -> (f64, f64) {
        let e = |i: usize| if self.edges[i] { band } else { 0.0 };
        (self.w - e(1) - e(3), self.h - e(0) - e(2))
    }

    fn mirror_x(&mut self) {
        let w = self.w;
        for p in self.outline.iter_mut() {
            p[0] = w - p[0];
        }
        self.outline.reverse();
        for h in self.holes.iter_mut() {
            h.x = w - h.x;
            h.edge = h.edge.map(|e| match e {
                1 => 3,
                3 => 1,
                e => e,
            });
        }
        for p in self.pockets.iter_mut() {
            p.x = w - p.x - p.w;
        }
        self.edges.swap(1, 3);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Hardware {
    pub cabinet: String,
    pub name: String,
    pub qty: u32,
}

/// A box shown in 3D that is not a part to cut (appliance bodies).
#[derive(Debug, Clone, Serialize)]
pub struct Placeholder {
    pub cabinet: String,
    pub label: String,
    pub box3d: Box3,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Build {
    pub parts: Vec<Part>,
    pub hardware: Vec<Hardware>,
    pub placeholders: Vec<Placeholder>,
    pub warnings: Vec<String>,
}

fn rect(w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

struct Ctx<'a> {
    cab: &'a Cabinet,
    out: &'a mut Build,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        key: &str,
        name: &str,
        role: Role,
        mat: &Material,
        w: f64,
        h: f64,
        grain_y: bool,
        edges: [bool; 4],
        box3d: Box3,
    ) -> usize {
        self.out.parts.push(Part {
            id: format!("{}-{}", self.cab.id, key),
            cabinet: self.cab.id.clone(),
            name: name.into(),
            role,
            material: mat.id.clone(),
            thickness: mat.thickness,
            w: r1(w),
            h: r1(h),
            grain_y,
            edges,
            outline: rect(r1(w), r1(h)),
            holes: vec![],
            cutouts: vec![],
            pockets: vec![],
            box3d,
        });
        self.out.parts.len() - 1
    }

    fn hw(&mut self, name: impl Into<String>, qty: u32) {
        if qty == 0 {
            return;
        }
        let name = name.into();
        if let Some(h) = self
            .out
            .hardware
            .iter_mut()
            .find(|h| h.cabinet == self.cab.id && h.name == name)
        {
            h.qty += qty;
        } else {
            self.out.hardware.push(Hardware {
                cabinet: self.cab.id.clone(),
                name,
                qty,
            });
        }
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.out
            .warnings
            .push(format!("{} ({}): {}", self.cab.name, self.cab.id, msg.into()));
    }
}

const SLIDE_LENGTHS: [f64; 7] = [250.0, 300.0, 350.0, 400.0, 450.0, 500.0, 550.0];

fn hinge_count(h: f64) -> usize {
    if h <= 900.0 {
        2
    } else if h <= 1600.0 {
        3
    } else if h <= 2000.0 {
        4
    } else {
        5
    }
}

fn hinge_positions(h: f64) -> Vec<f64> {
    let n = hinge_count(h);
    let (a, b) = (100.0, h - 100.0);
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
        .collect()
}

/// Build every part and hardware item for a whole project.
pub fn build(project: &Project) -> Build {
    let mut out = Build::default();
    for cab in &project.cabinets {
        build_cabinet(project, cab, &mut out);
    }
    build_worktops(project, &mut out);
    out
}

/// Replace slab fronts from `start` on with five-piece Shaker fronts
/// when the cabinet asks for them and the front is big enough.
fn shakerize(project: &Project, cab: &Cabinet, out: &mut Build, start: usize) {
    let s = &project.settings;
    if cab.door_style == DoorStyle::ShakerRouted {
        // One-piece front: recess routed into the outside face (face 2);
        // the frame stays at full thickness. Small fronts stay plain.
        let f = s.shaker_frame;
        for p in out.parts[start..].iter_mut() {
            if p.role == Role::Front && p.w.min(p.h) >= 2.0 * f + 60.0 {
                let depth = s.routed_depth.clamp(1.0, p.thickness - 6.0);
                p.pockets.push(Pocket { x: f, y: f, w: r1(p.w - 2.0 * f), h: r1(p.h - 2.0 * f), depth, r: 3.0, face: 2 });
            }
        }
        return;
    }
    if cab.door_style != DoorStyle::Shaker {
        return;
    }
    let pm = project.material(&cab.panel_material);
    let f = s.shaker_frame;
    let mut i = start;
    while i < out.parts.len() {
        let p = &out.parts[i];
        if p.role == Role::Front && p.w.min(p.h) >= 2.0 * f + 60.0 {
            let p = out.parts.remove(i);
            let pieces = shaker_pieces(p, f, s.shaker_groove, &pm);
            let n = pieces.len();
            for (k, q) in pieces.into_iter().enumerate() {
                out.parts.insert(i + k, q);
            }
            i += n;
        } else {
            i += 1;
        }
    }
}

/// Fillers, dishwasher panels and appliance spaces: no carcass.
fn build_special(project: &Project, cab: &Cabinet, out: &mut Build) {
    let s = &project.settings;
    let cm = project.material(&cab.carcass_material);
    let fm = project.material(&cab.front_material);
    let start = out.parts.len();
    let mut cx = Ctx { cab, out };
    let (w, h, d) = (cab.width, cab.height, cab.depth);
    let (t, ft, g) = (cm.thickness, fm.thickness, s.front_gap);
    let toe = if cab.elevation > 0.0 { 0.0 } else { cab.toe_kick_height.max(0.0).min(h * 0.5) };
    let tkd = cab.toe_kick_depth.clamp(0.0, d * 0.5);
    if w < 10.0 || h < 50.0 {
        cx.warn("too small to build");
        return;
    }
    let toe_board = |cx: &mut Ctx| {
        if toe > 0.0 && cab.kind != CabinetKind::Appliance {
            cx.part("toe", "Toe kick", Role::Carcass, &cm, w, toe, false, [false, false, true, false],
                Box3 { pos: [0.0, 0.0, d - tkd - t], size: [w, toe, t] });
        }
    };
    match cab.kind {
        CabinetKind::Filler => {
            cx.part("filler", "Filler", Role::Front, &fm, w, h - toe, true, [true; 4],
                Box3 { pos: [0.0, toe, d], size: [w, h - toe, ft] });
            toe_board(&mut cx);
        }
        CabinetKind::Dishwasher => {
            let (fw, fh) = (w - g, h - toe - g);
            let idx = cx.part("panel", "Dishwasher panel", Role::Front, &fm, fw, fh, false, [true; 4],
                Box3 { pos: [g / 2.0, toe + g / 2.0, d], size: [fw, fh, ft] });
            if cab.handles {
                let sp = s.handle_spacing;
                let y = fh - 60.0;
                let p = &mut cx.out.parts[idx];
                p.holes.push(Hole { x: fw / 2.0 - sp / 2.0, y, dia: 5.0, depth: 0.0, edge: None });
                p.holes.push(Hole { x: fw / 2.0 + sp / 2.0, y, dia: 5.0, depth: 0.0, edge: None });
                cx.hw(format!("Handle {}mm", sp), 1);
            }
            cx.hw("Dishwasher panel fixing kit", 1);
            toe_board(&mut cx);
            cx.out.placeholders.push(Placeholder {
                cabinet: cab.id.clone(),
                label: "Dishwasher".into(),
                box3d: Box3 { pos: [10.0, toe, 0.0], size: [w - 20.0, h - toe - 5.0, d - 5.0] },
            });
            shakerize(project, cab, cx.out, start);
        }
        CabinetKind::Appliance => {
            cx.out.placeholders.push(Placeholder {
                cabinet: cab.id.clone(),
                label: cab.name.clone(),
                box3d: Box3 { pos: [5.0, 0.0, 0.0], size: [w - 10.0, h, d] },
            });
        }
        _ => {}
    }
}

/// Finished end panels outside the carcass, covering the door edge.
fn end_panels(project: &Project, cab: &Cabinet, out: &mut Build) {
    let fm = project.material(&cab.front_material);
    let ft = fm.thickness;
    let (w, h, d) = (cab.width, cab.height, cab.depth);
    let mut cx = Ctx { cab, out };
    // Drawn like a left side: x from the front, y up; front and top banded.
    for (on, key, name, x) in [(cab.end_left, "end-l", "End panel L", -ft), (cab.end_right, "end-r", "End panel R", w)] {
        if on {
            cx.part(key, name, Role::Front, &fm, d + ft, h, true, [false, false, true, true],
                Box3 { pos: [x, 0.0, 0.0], size: [ft, h, d + ft] });
        }
    }
}

/// Split a slab front into a five-piece Shaker door. Holes go to the
/// piece that contains their centre, in that piece's own frame.
fn shaker_pieces(p: Part, f: f64, g: f64, pm: &Material) -> Vec<Part> {
    let (w, h) = (p.w, p.h);
    let [x0, y0, z0] = p.box3d.pos;
    let ft = p.box3d.size[2];
    let pt = pm.thickness.min(ft);
    let piece = |key: &str, name: &str, mat: &Material, pw: f64, ph: f64, grain_y: bool, b: Box3| Part {
        id: format!("{}-{}", p.id, key),
        cabinet: p.cabinet.clone(),
        name: format!("{} {}", p.name, name),
        role: Role::Front,
        material: mat.id.clone(),
        thickness: mat.thickness,
        w: r1(pw),
        h: r1(ph),
        grain_y,
        edges: [false; 4],
        outline: rect(r1(pw), r1(ph)),
        holes: vec![],
        cutouts: vec![],
        pockets: vec![],
        box3d: b,
    };
    let fm = Material { id: p.material.clone(), thickness: p.thickness, ..Material::default() };
    let rail_len = w - 2.0 * f + 2.0 * g;
    let mut out = vec![
        piece("stl", "stile L", &fm, f, h, true, Box3 { pos: [x0, y0, z0], size: [f, h, ft] }),
        piece("str", "stile R", &fm, f, h, true, Box3 { pos: [x0 + w - f, y0, z0], size: [f, h, ft] }),
        piece("rb", "rail B", &fm, rail_len, f, false, Box3 { pos: [x0 + f, y0, z0], size: [w - 2.0 * f, f, ft] }),
        piece("rt", "rail T", &fm, rail_len, f, false, Box3 { pos: [x0 + f, y0 + h - f, z0], size: [w - 2.0 * f, f, ft] }),
        piece(
            "pnl",
            "panel",
            pm,
            w - 2.0 * f + 2.0 * g - 2.0,
            h - 2.0 * f + 2.0 * g - 2.0,
            true,
            Box3 { pos: [x0 + f, y0 + f, z0 + (ft - pt) / 2.0], size: [w - 2.0 * f, h - 2.0 * f, pt] },
        ),
    ];
    for hole in p.holes {
        let (x, y) = (hole.x, hole.y);
        let (k, nx, ny) = if x < f {
            (0, x, y)
        } else if x > w - f {
            (1, x - (w - f), y)
        } else if y < f {
            (2, x - f + g, y)
        } else if y > h - f {
            (3, x - f + g, y - (h - f))
        } else {
            (4, x - f + g - 1.0, y - f + g - 1.0)
        };
        out[k].holes.push(Hole { x: r1(nx), y: r1(ny), ..hole });
    }
    out
}

/// Cabinet-local (x, z) -> world (x, z) for a cabinet at `o` rotated `deg` about y.
pub fn to_world(ox: f64, oz: f64, deg: f64, x: f64, z: f64) -> (f64, f64) {
    let (sn, cs) = deg.to_radians().sin_cos();
    (ox + x * cs + z * sn, oz - x * sn + z * cs)
}

fn to_local(ox: f64, oz: f64, deg: f64, wx: f64, wz: f64) -> (f64, f64) {
    let (sn, cs) = deg.to_radians().sin_cos();
    let (dx, dz) = (wx - ox, wz - oz);
    (dx * cs - dz * sn, dx * sn + dz * cs)
}

/// One worktop per straight run of touching base cabinets, along each
/// cabinet's own axis (any rotation). Where an L corner makes two
/// worktops overlap, the later one is shortened to butt against the first.
fn build_worktops(project: &Project, out: &mut Build) {
    let s = &project.settings;
    if s.worktop_thickness <= 0.0 {
        return;
    }
    let tm = project.material("top");
    let r2 = |v: f64| (v * 2.0).round() / 2.0;
    // (cabinet, position along its axis, offset across it)
    let mut bases: Vec<(&Cabinet, f64, f64)> = project
        .cabinets
        .iter()
        .filter(|c| c.under_worktop())
        .map(|c| {
            let (sn, cs) = c.rotation.to_radians().sin_cos();
            (c, c.x * cs - c.z * sn, r2(c.x * sn + c.z * cs))
        })
        .collect();
    let key = |c: &Cabinet, across: f64| (r2(c.rotation.rem_euclid(360.0)), across, c.elevation, c.height);
    bases.sort_by(|a, b| {
        (key(a.0, a.2), a.1)
            .partial_cmp(&(key(b.0, b.2), b.1))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut runs: Vec<Vec<(&Cabinet, f64)>> = vec![];
    let mut last_key = None;
    for (c, along, across) in bases {
        let k = key(c, across);
        if let (Some(r), Some(lk)) = (runs.last_mut(), last_key) {
            let (p, pa): (&Cabinet, f64) = r[r.len() - 1];
            if lk == k && (along - (pa + p.width)).abs() < 1.0 {
                r.push((c, along));
                continue;
            }
        }
        last_key = Some(k);
        runs.push(vec![(c, along)]);
    }

    struct Top<'a> {
        first: &'a Cabinet,
        /// cabinets with their offset along the run from the first
        members: Vec<(&'a Cabinet, f64)>,
        ids: String,
        start: f64,
        len: f64,
        depth: f64,
    }
    let mut tops: Vec<Top> = runs
        .iter()
        .map(|r| {
            let (first, a0) = r[0];
            let (last, a1) = r[r.len() - 1];
            Top {
                first,
                members: r.iter().map(|(c, a)| (*c, a - a0)).collect(),
                ids: r.iter().map(|(c, _)| c.id.as_str()).collect::<Vec<_>>().join("+"),
                start: 0.0,
                len: a1 + last.width - a0,
                depth: r.iter().map(|(c, _)| c.depth).fold(0.0, f64::max) + s.worktop_overhang,
            }
        })
        .collect();

    // Trim overlaps at corners: earlier (longer-established) worktop wins.
    for j in 1..tops.len() {
        for i in 0..j {
            let (fi, fj) = (tops[i].first, tops[j].first);
            if (fi.rotation - fj.rotation).rem_euclid(180.0).abs() < 1e-6 {
                continue; // parallel runs never butt
            }
            // i's footprint corners in j's local frame
            let corners = [
                (tops[i].start, 0.0),
                (tops[i].start + tops[i].len, 0.0),
                (tops[i].start, tops[i].depth),
                (tops[i].start + tops[i].len, tops[i].depth),
            ];
            let mut lx = (f64::MAX, f64::MIN);
            let mut lz = (f64::MAX, f64::MIN);
            for (x, z) in corners {
                let (wx, wz) = to_world(fi.x, fi.z, fi.rotation, x, z);
                let (x2, z2) = to_local(fj.x, fj.z, fj.rotation, wx, wz);
                lx = (lx.0.min(x2), lx.1.max(x2));
                lz = (lz.0.min(z2), lz.1.max(z2));
            }
            let t = &mut tops[j];
            let (a, b) = (t.start, t.start + t.len);
            let z_overlap = lz.1.min(t.depth) - lz.0.max(0.0);
            let x_overlap = lx.1.min(b) - lx.0.max(a);
            if z_overlap <= 0.5 || x_overlap <= 0.5 {
                continue;
            }
            if lx.0 <= a + 0.5 {
                t.start = lx.1.min(b);
            } else if lx.1 >= b - 0.5 {
                t.len = lx.0 - a;
                continue;
            }
            t.len = b - t.start;
        }
    }

    for t in tops {
        if t.len < 50.0 {
            continue;
        }
        let th = s.worktop_thickness;
        // Cut-outs: drawing x along the worktop from its (trimmed) start,
        // y from the front edge.
        let mut cutouts = vec![];
        for (c, off) in &t.members {
            if c.cutout == CutoutKind::None {
                continue;
            }
            let (dw, dd, dr) = c.cutout.default_size();
            let cw = if c.cutout_width > 0.0 { c.cutout_width } else { dw };
            let cd = if c.cutout_depth > 0.0 { c.cutout_depth } else { dd };
            let cr = if c.cutout_radius > 0.0 { c.cutout_radius } else { dr };
            let x = off + c.width / 2.0 - cw / 2.0 - t.start;
            let y = c.cutout_setback;
            let label = format!("{} ({}) {:?} cut-out", c.name, c.id, c.cutout).to_lowercase();
            if x < 50.0 || x + cw > t.len - 50.0 || y < 30.0 || y + cd > t.depth - 30.0 {
                out.warnings.push(format!("{label} {cw:.0}×{cd:.0} leaves less than 50 mm to the worktop edge"));
            } else if cw > c.width - 2.0 * project.material(&c.carcass_material).thickness {
                out.warnings.push(format!("{label} is wider than the cabinet interior"));
            }
            cutouts.push(Cutout { kind: c.cutout, x: r1(x), y: r1(y), w: cw, h: cd, r: cr });
        }
        out.parts.push(Part {
            id: format!("{}-worktop", t.first.id),
            cabinet: t.first.id.clone(),
            name: "Worktop".into(),
            role: Role::Worktop,
            material: tm.id.clone(),
            thickness: th,
            w: r1(t.len),
            h: r1(t.depth),
            grain_y: false,
            edges: [true, false, false, false],
            outline: rect(r1(t.len), r1(t.depth)),
            holes: vec![],
            cutouts,
            pockets: vec![],
            box3d: Box3 { pos: [t.start, t.first.height, 0.0], size: [t.len, th, t.depth] },
        });
        if t.len > tm.sheet_length - 2.0 * s.sheet_trim {
            out.warnings.push(format!(
                "Worktop over {} is {:.0} mm, longer than the {} blank",
                t.ids, t.len, tm.name
            ));
        }
    }
}

fn build_cabinet(project: &Project, cab: &Cabinet, out: &mut Build) {
    if !cab.kind.is_carcass() {
        build_special(project, cab, out);
        return;
    }
    build_carcass(project, cab, out);
    end_panels(project, cab, out);
}

fn build_carcass(project: &Project, cab: &Cabinet, out: &mut Build) {
    let s = &project.settings;
    let cm = project.material(&cab.carcass_material);
    let bm = project.material(&cab.back_material);
    let fm = project.material(&cab.front_material);
    let dm = project.material(&cab.drawer_material);
    let start = out.parts.len();
    let mut cx = Ctx { cab, out };

    let (w, h, d) = (cab.width, cab.height, cab.depth);
    let t = cm.thickness;
    let bt = bm.thickness;
    if w < 2.0 * t + 50.0 || h < 2.0 * t + 50.0 || d < bt + 100.0 {
        cx.warn("too small to build");
        return;
    }
    let toe = if cab.kind == CabinetKind::Wall {
        0.0
    } else {
        cab.toe_kick_height.max(0.0)
    };
    let tkd = cab.toe_kick_depth.clamp(0.0, d * 0.5);
    // Back construction decides how deep the carcass panels are and where
    // the interior starts.
    let grooved = s.back_mode == BackMode::Grooved;
    let gd = s.groove_depth.clamp(1.0, t - 4.0);
    let inset = s.back_inset.max(0.0);
    let cd = if grooved { d } else { d - bt }; // depth of sides, bottom and top
    let z0 = d - cd; // their back edge (cabinet-local z)
    let bz = if grooved { inset + bt } else { bt }; // front face of the back panel
    let idp = d - bz; // usable interior depth
    let iw = w - 2.0 * t; // interior width
    let sl = s.system_line;
    // Joint between a side and a panel: face bore in the side, edge bore in the panel.
    let (side_dia, side_depth, edge_dia, edge_depth) = match s.joinery {
        Joinery::Confirmat => (7.0, 0.0, 5.0, 45.0),
        Joinery::Dowel => (8.0, 12.0, 8.0, 25.0),
    };
    let joint = |at: &[f64]| at.to_vec();
    // groove for the back panel, measured from the front of a panel of depth `pd`
    let back_groove_y = |pd: f64| pd - inset - bt - 0.5;

    // Holes on side panels, unmirrored frame: x from front, y from floor.
    let mut side_l: Vec<Hole> = vec![];
    let mut side_r: Vec<Hole> = vec![];
    let mut side_pockets: Vec<Pocket> = vec![];
    let both = |l: &mut Vec<Hole>, r: &mut Vec<Hole>, hole: Hole| {
        l.push(hole.clone());
        r.push(hole);
    };
    // Adds the side bores for a panel whose mid-thickness is at height `y`,
    // and the matching bores in both ends of that panel (drawing y = from front).
    let mut joints: Vec<(usize, Vec<f64>)> = vec![];

    // ---------- carcass ----------
    let yb = toe; // bottom panel underside
    let yt = h - t; // top panel underside
    let bottom = cx.part(
        "bottom",
        "Bottom",
        Role::Carcass,
        &cm,
        iw,
        cd,
        false,
        [true, false, false, false],
        Box3 { pos: [t, yb, z0], size: [iw, t, cd] },
    );
    let at = joint(&[50.0, cd / 2.0, cd - 50.0]);
    for &x in &at {
        both(&mut side_l, &mut side_r, Hole { x, y: yb + t / 2.0, dia: side_dia, depth: side_depth, edge: None });
    }
    joints.push((bottom, at));
    let mut back_groove_parts = vec![bottom];
    if cab.kind == CabinetKind::Base {
        let rw = 100.0_f64.min(cd / 2.0);
        let rf = cx.part(
            "rail-f",
            "Top rail front",
            Role::Carcass,
            &cm,
            iw,
            rw,
            false,
            [true, false, false, false],
            Box3 { pos: [t, yt, d - rw], size: [iw, t, rw] },
        );
        let rb = cx.part(
            "rail-b",
            "Top rail back",
            Role::Carcass,
            &cm,
            iw,
            rw,
            false,
            [false; 4],
            Box3 { pos: [t, yt, z0], size: [iw, t, rw] },
        );
        for x in [rw / 2.0, cd - rw / 2.0] {
            both(&mut side_l, &mut side_r, Hole { x, y: yt + t / 2.0, dia: side_dia, depth: side_depth, edge: None });
        }
        joints.push((rf, vec![rw / 2.0]));
        joints.push((rb, vec![rw / 2.0]));
        back_groove_parts.push(rb);
    } else {
        let top = cx.part(
            "top",
            "Top",
            Role::Carcass,
            &cm,
            iw,
            cd,
            false,
            [true, false, false, false],
            Box3 { pos: [t, yt, z0], size: [iw, t, cd] },
        );
        let at = joint(&[50.0, cd / 2.0, cd - 50.0]);
        for &x in &at {
            both(&mut side_l, &mut side_r, Hole { x, y: yt + t / 2.0, dia: side_dia, depth: side_depth, edge: None });
        }
        joints.push((top, at));
        back_groove_parts.push(top);
    }
    // Edge bores at both ends of each joined panel (left edge x = 0, right edge x = iw).
    for (idx, at) in joints {
        let p = &mut cx.out.parts[idx];
        for y in at {
            for (e, x) in [(3u8, 0.0), (1u8, p.w)] {
                p.holes.push(Hole { x, y: r1(y), dia: edge_dia, depth: edge_depth, edge: Some(e) });
            }
        }
    }
    if toe > 0.0 {
        cx.part(
            "toe",
            "Toe kick",
            Role::Carcass,
            &cm,
            iw,
            toe,
            false,
            [false, false, true, false],
            Box3 { pos: [t, 0.0, d - tkd - t], size: [iw, toe, t] },
        );
    }
    if grooved {
        // Back sits gd deep in the sides, bottom and top (or back rail).
        let (by0, by1) = (yb + t - gd, yt + gd);
        let bwid = iw + 2.0 * gd - 1.0;
        cx.part(
            "back",
            "Back",
            Role::Back,
            &bm,
            bwid,
            by1 - by0 - 1.0,
            true,
            [false; 4],
            Box3 { pos: [t - gd + 0.5, by0 + 0.5, inset], size: [bwid, by1 - by0 - 1.0, bt] },
        );
        let gw = bt + 0.5;
        for idx in back_groove_parts {
            let p = &mut cx.out.parts[idx];
            let pd = p.h;
            p.pockets.push(Pocket { x: 0.0, y: r1(back_groove_y(pd)), w: p.w, h: gw, depth: gd, r: 0.0, face: 1 });
        }
        // sides: full-height groove, x from the front
        let groove = Pocket { x: r1(back_groove_y(cd)), y: 0.0, w: gw, h: h, depth: gd, r: 0.0, face: 1 };
        side_pockets.push(groove);
    } else {
        cx.part(
            "back",
            "Back",
            Role::Back,
            &bm,
            w,
            h - toe,
            true,
            [false; 4],
            Box3 { pos: [0.0, toe, 0.0], size: [w, h - toe, bt] },
        );
    }

    // ---------- fronts ----------
    let g = s.front_gap;
    let ft = fm.thickness;
    let mut cursor = h;
    // Blind corner: open part of the front is [ox0, ox1].
    let blind = cab.blind_side != BlindSide::None;
    let bw = cab.blind_width.clamp(0.0, w - 150.0);
    let (ox0, ox1) = match cab.blind_side {
        BlindSide::Left => (bw, w),
        BlindSide::Right => (0.0, w - bw),
        BlindSide::None => (0.0, w),
    };
    if blind && !cab.drawers.is_empty() {
        cx.warn("drawers are not built in blind corner cabinets");
    }
    let drawers: &[f64] = if blind { &[] } else { &cab.drawers };
    let slide_len = SLIDE_LENGTHS
        .iter()
        .rev()
        .copied()
        .find(|l| *l <= idp - 10.0);
    for (i, &dh) in drawers.iter().enumerate() {
        let lo = (cursor - dh).max(toe);
        let fh = cursor - lo - g;
        if fh < 60.0 {
            cx.warn(format!("drawer {} too short", i + 1));
            break;
        }
        let fw = w - g;
        let idx = cx.part(
            &format!("dfront{}", i + 1),
            &format!("Drawer front {}", i + 1),
            Role::Front,
            &fm,
            fw,
            fh,
            false,
            [true; 4],
            Box3 { pos: [g / 2.0, lo + g / 2.0, d], size: [fw, fh, ft] },
        );
        if cab.handles {
            let sp = s.handle_spacing;
            let p = &mut cx.out.parts[idx];
            if fw > sp + 80.0 {
                p.holes.push(Hole { x: fw / 2.0 - sp / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0, edge: None });
                p.holes.push(Hole { x: fw / 2.0 + sp / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0, edge: None });
            } else {
                p.holes.push(Hole { x: fw / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0, edge: None });
            }
            cx.hw(format!("Handle {}mm", sp), 1);
        }

        // drawer box
        match slide_len {
            None => cx.warn("too shallow for drawer slides"),
            Some(sl_len) => {
                let box_lo = lo.max(toe + t) + 15.0;
                let box_hi = cursor.min(h - t) - 15.0;
                let bh = ((box_hi - box_lo) / 10.0).floor() * 10.0;
                let bh = bh.min(250.0);
                let dt = dm.thickness;
                let bw = iw - 2.0 * s.slide_clearance;
                if bh < 60.0 || bw < 2.0 * dt + 50.0 {
                    cx.warn(format!("drawer {} box too small", i + 1));
                } else {
                    let z0 = d - sl_len;
                    let x0 = t + s.slide_clearance;
                    // Bottom sits in a groove (gd deep, 10 mm up) in all four box parts.
                    let bg = |len: f64| Pocket { x: 0.0, y: 10.0, w: len, h: bt + 0.5, depth: gd.min(dt - 5.0), r: 0.0, face: 1 };
                    let bgd = gd.min(dt - 5.0);
                    for (k, xs) in [("l", x0), ("r", x0 + bw - dt)] {
                        let idx = cx.part(
                            &format!("d{}side{}", i + 1, k),
                            &format!("Drawer {} side", i + 1),
                            Role::DrawerBox,
                            &dm,
                            sl_len,
                            bh,
                            false,
                            [false; 4],
                            Box3 { pos: [xs, box_lo, z0], size: [dt, bh, sl_len] },
                        );
                        cx.out.parts[idx].pockets.push(bg(sl_len));
                    }
                    let iw2 = bw - 2.0 * dt;
                    for (k, zs) in [("f", d - dt), ("b", z0)] {
                        let idx = cx.part(
                            &format!("d{}{}", i + 1, k),
                            &format!("Drawer {} {}", i + 1, if k == "f" { "front" } else { "back" }),
                            Role::DrawerBox,
                            &dm,
                            iw2,
                            bh,
                            false,
                            [false; 4],
                            Box3 { pos: [x0 + dt, box_lo, zs], size: [iw2, bh, dt] },
                        );
                        cx.out.parts[idx].pockets.push(bg(iw2));
                    }
                    cx.part(
                        &format!("d{}bot", i + 1),
                        &format!("Drawer {} bottom", i + 1),
                        Role::DrawerBox,
                        &bm,
                        iw2 + 2.0 * bgd - 1.0,
                        sl_len - 2.0 * dt + 2.0 * bgd - 1.0,
                        false,
                        [false; 4],
                        Box3 {
                            pos: [x0 + dt - bgd + 0.5, box_lo + 10.0, z0 + dt - bgd + 0.5],
                            size: [iw2 + 2.0 * bgd - 1.0, bt, sl_len - 2.0 * dt + 2.0 * bgd - 1.0],
                        },
                    );
                    cx.hw(format!("Drawer slide {}mm (pair)", sl_len), 1);
                    let ys = box_lo + 20.0;
                    let mut xs = sl;
                    while xs < sl_len - 10.0 {
                        both(&mut side_l, &mut side_r, Hole { x: xs, y: ys, dia: 5.0, depth: 12.0, edge: None });
                        xs += 224.0;
                    }
                }
            }
        }
        cursor = lo;
    }

    // Door zone
    let zone_lo = toe;
    let zone_hi = cursor;
    if cab.doors > 0 && zone_hi - zone_lo > 100.0 {
        let fh = zone_hi - zone_lo - g;
        let ow = ox1 - ox0;
        let leaves: Vec<(f64, f64, HingeSide)> = if blind {
            // One leaf, hinged on the cabinet side away from the blind panel.
            let side = if cab.blind_side == BlindSide::Left { HingeSide::Right } else { HingeSide::Left };
            vec![(ox0 + g / 2.0, ow - g, side)]
        } else if cab.doors >= 2 {
            let lw = ow / 2.0 - g;
            vec![
                (ox0 + g / 2.0, lw, HingeSide::Left),
                (ox0 + ow / 2.0 + g / 2.0, lw, HingeSide::Right),
            ]
        } else {
            vec![(ox0 + g / 2.0, ow - g, cab.hinge_side)]
        };
        if cab.doors > 2 {
            cx.warn("more than 2 doors not supported, using 2");
        }
        if blind {
            let (bx0, bx1) = if cab.blind_side == BlindSide::Left { (0.0, ox0) } else { (ox1, w) };
            cx.part(
                "blind",
                "Blind panel",
                Role::Front,
                &fm,
                bx1 - bx0 - g,
                fh,
                true,
                [true; 4],
                Box3 { pos: [bx0 + g / 2.0, zone_lo + g / 2.0, d], size: [bx1 - bx0 - g, fh, ft] },
            );
        }
        let hy = hinge_positions(fh);
        for (n, (x0, fw, side)) in leaves.into_iter().enumerate() {
            let idx = cx.part(
                &format!("door{}", n + 1),
                &format!("Door {}", n + 1),
                Role::Front,
                &fm,
                fw,
                fh,
                true,
                [true; 4],
                Box3 { pos: [x0, zone_lo + g / 2.0, d], size: [fw, fh, ft] },
            );
            // Door is drawn inside face up, so a left-hinged door has
            // its hinge edge on the drawing's right.
            let cup_x = match side {
                HingeSide::Left => fw - s.hinge_cup_inset,
                HingeSide::Right => s.hinge_cup_inset,
            };
            let handle_x = match side {
                HingeSide::Left => 40.0,
                HingeSide::Right => fw - 40.0,
            };
            {
                let p = &mut cx.out.parts[idx];
                for &y in &hy {
                    p.holes.push(Hole { x: cup_x, y, dia: 35.0, depth: 13.0, edge: None });
                }
                if cab.handles {
                    let sp = s.handle_spacing;
                    let yc = match cab.kind {
                        CabinetKind::Base => fh - 60.0 - sp / 2.0,
                        CabinetKind::Wall => 60.0 + sp / 2.0,
                        _ => (1000.0 - zone_lo).clamp(sp, fh - sp),
                    };
                    p.holes.push(Hole { x: handle_x, y: yc - sp / 2.0, dia: 5.0, depth: 0.0, edge: None });
                    p.holes.push(Hole { x: handle_x, y: yc + sp / 2.0, dia: 5.0, depth: 0.0, edge: None });
                }
            }
            if cab.handles {
                cx.hw(format!("Handle {}mm", s.handle_spacing), 1);
            }
            cx.hw("Clip hinge 110° + plate", hy.len() as u32);
            let target = if side == HingeSide::Left { &mut side_l } else { &mut side_r };
            for &y in &hy {
                let yw = zone_lo + g / 2.0 + y;
                target.push(Hole { x: sl, y: yw - 16.0, dia: 5.0, depth: 12.0, edge: None });
                target.push(Hole { x: sl, y: yw + 16.0, dia: 5.0, depth: 12.0, edge: None });
            }
        }
    }

    // ---------- shelves ----------
    let in_lo = toe + t;
    let in_hi = zone_hi.min(h - t);
    if cab.shelves > 0 && in_hi - in_lo > 150.0 {
        let mut rows = vec![];
        let mut y = in_lo + 64.0;
        while y < in_hi - 64.0 {
            rows.push(y);
            y += 32.0;
        }
        for &y in &rows {
            for x in [sl, idp - sl] {
                both(&mut side_l, &mut side_r, Hole { x, y, dia: 5.0, depth: 12.0, edge: None });
            }
        }
        let n = cab.shelves;
        let sd = idp - s.shelf_setback;
        let sw = iw - 2.0;
        for i in 0..n {
            let target = in_lo + (in_hi - in_lo) * (i + 1) as f64 / (n + 1) as f64;
            let ys = rows
                .iter()
                .copied()
                .min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs()))
                .unwrap_or(target);
            cx.part(
                &format!("shelf{}", i + 1),
                &format!("Shelf {}", i + 1),
                Role::Shelf,
                &cm,
                sw,
                sd,
                false,
                [true, false, false, false],
                Box3 { pos: [t + 1.0, ys, bz], size: [sw, t, sd] },
            );
        }
        cx.hw("Shelf pin 5mm", 4 * n);
    }

    // ---------- shaker fronts ----------
    shakerize(project, cab, cx.out, start);

    // ---------- sides (added last so all holes are known) ----------
    for (key, name, holes, x) in [
        ("side-l", "Left side", side_l, 0.0),
        ("side-r", "Right side", side_r, w - t),
    ] {
        let idx = cx.part(
            key,
            name,
            Role::Carcass,
            &cm,
            cd,
            h,
            true,
            [false, false, false, true],
            Box3 { pos: [x, 0.0, z0], size: [t, h, cd] },
        );
        let p = &mut cx.out.parts[idx];
        p.pockets = side_pockets.clone();
        if toe > 0.0 && tkd > 0.0 {
            p.outline = vec![
                [0.0, r1(toe)],
                [r1(tkd), r1(toe)],
                [r1(tkd), 0.0],
                [r1(cd), 0.0],
                [r1(cd), r1(h)],
                [0.0, r1(h)],
            ];
        }
        p.holes = holes
            .into_iter()
            .map(|mut h| {
                h.x = r1(h.x);
                h.y = r1(h.y);
                h
            })
            .collect();
        if key == "side-r" {
            p.mirror_x();
        }
    }
    // Sides first in the parts list.
    let n = cx.out.parts.len();
    cx.out.parts[start..n].rotate_right(2);
}
