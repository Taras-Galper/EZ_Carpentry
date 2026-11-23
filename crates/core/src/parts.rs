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
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Further boxes of the same part in 3D (L-shaped panels).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub more3d: Vec<Box3>,
    /// Banded outline segments (i = outline[i] to outline[i + 1]) for
    /// non-rectangular parts; `edges` covers rectangles.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub band_segs: Vec<usize>,
    /// Part serving several cabinets (run plinths): (cabinet id, share of
    /// its cost). Empty = belongs to `cabinet` alone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shared: Vec<(String, f64)>,
    /// Rotation (degrees, same sense as a cabinet's) of box3d about the
    /// vertical through box3d.pos, for parts at an angle (diagonal doors).
    #[serde(skip_serializing_if = "is_zero")]
    pub rot: f64,
    /// Plan outline (cabinet-local x, z) of a horizontal panel that is not
    /// a box (diagonal-front tops, bottoms, shelves); box3d gives its height.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<[f64; 2]>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
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
    /// Not a plain rectangle: "L", "5-sided", "notched", or "" for a rectangle.
    pub fn shape_label(&self) -> &'static str {
        match self.outline.len() {
            0..=4 => "",
            5 => "5-sided",
            6 if !self.band_segs.is_empty() => "L",
            _ => "notched",
        }
    }

    /// Banded shaped edges (outline segments beyond the four sides), as text.
    pub fn band_segs_text(&self) -> String {
        if self.band_segs.is_empty() {
            return String::new();
        }
        let n = self.outline.len();
        let lens: Vec<String> = self
            .band_segs
            .iter()
            .filter(|&&i| i < n)
            .map(|&i| {
                let (a, b) = (self.outline[i], self.outline[(i + 1) % n]);
                format!("{:.0}", ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt())
            })
            .collect();
        format!("front {}", lens.join("+"))
    }

    /// Has operations on the second face (part is turned over for them).
    pub fn face2(&self) -> bool {
        self.pockets.iter().any(|k| k.face == 2)
    }

    /// Banded edge length in mm.
    pub fn band_length(&self) -> f64 {
        let e = self.edges;
        let rect = (if e[0] { self.w } else { 0.0 })
            + (if e[2] { self.w } else { 0.0 })
            + (if e[1] { self.h } else { 0.0 })
            + (if e[3] { self.h } else { 0.0 });
        let n = self.outline.len();
        let segs: f64 = self
            .band_segs
            .iter()
            .filter(|&&i| i < n)
            .map(|&i| {
                let (a, b) = (self.outline[i], self.outline[(i + 1) % n]);
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
            })
            .sum();
        rect + segs
    }

    /// Index of this part in the build (parts are unique by id).
    pub fn id_index(&self, b: &Build) -> usize {
        b.parts.iter().position(|q| q.id == self.id).unwrap_or(0)
    }

    /// Size to cut before edge banding is applied.
    pub fn cut_size(&self, band: f64) -> (f64, f64) {
        let e = |i: usize| if self.edges[i] { band } else { 0.0 };
        (self.w - e(1) - e(3), self.h - e(0) - e(2))
    }

    /// The part as it is cut, before edge banding: every banded edge (sides
    /// in `edges`, segments in `band_segs`) moves in by the band thickness,
    /// and bores, pockets and cut-outs move with the panel so they land in
    /// the right place once the band is on. w / h become the cut size.
    pub fn machined(&self, band: f64) -> Part {
        let mut p = self.clone();
        let n = self.outline.len();
        if band <= 0.0 || n < 3 || (!self.edges.iter().any(|e| *e) && self.band_segs.is_empty()) {
            return p;
        }
        let o = &self.outline;
        let on = |a: [f64; 2], b: [f64; 2]| -> Option<usize> {
            let eq = |u: f64, v: f64| (u - v).abs() < 0.01;
            if eq(a[1], 0.0) && eq(b[1], 0.0) {
                Some(0)
            } else if eq(a[0], self.w) && eq(b[0], self.w) {
                Some(1)
            } else if eq(a[1], self.h) && eq(b[1], self.h) {
                Some(2)
            } else if eq(a[0], 0.0) && eq(b[0], 0.0) {
                Some(3)
            } else {
                None
            }
        };
        // inward offset of each segment i (outline[i] -> outline[i+1])
        let area2: f64 = (0..n).map(|i| o[i][0] * o[(i + 1) % n][1] - o[(i + 1) % n][0] * o[i][1]).sum();
        let ccw = area2 > 0.0;
        let off: Vec<f64> = (0..n)
            .map(|i| {
                let (a, b) = (o[i], o[(i + 1) % n]);
                let side = on(a, b).map(|e| self.edges[e]).unwrap_or(false);
                if side || self.band_segs.contains(&i) {
                    band
                } else {
                    0.0
                }
            })
            .collect();
        // offset line of segment i: point + direction
        let line = |i: usize| {
            let (a, b) = (o[i], o[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-9);
            let (ux, uy) = (dx / l, dy / l);
            // inward normal: left of the direction for counter-clockwise outlines
            let (nx, ny) = if ccw { (-uy, ux) } else { (uy, -ux) };
            ([a[0] + nx * off[i], a[1] + ny * off[i]], [ux, uy])
        };
        let mut pts = vec![];
        for i in 0..n {
            let (p0, d0) = line((i + n - 1) % n);
            let (p1, d1) = line(i);
            let c = d0[0] * d1[1] - d0[1] * d1[0];
            let v = if c.abs() < 1e-9 {
                p1
            } else {
                let s = ((p1[0] - p0[0]) * d1[1] - (p1[1] - p0[1]) * d1[0]) / c;
                [p0[0] + d0[0] * s, p0[1] + d0[1] * s]
            };
            pts.push(v);
        }
        let (mx, my) = pts.iter().fold((f64::MAX, f64::MAX), |a, v| (a.0.min(v[0]), a.1.min(v[1])));
        let (xx, xy) = pts.iter().fold((f64::MIN, f64::MIN), |a, v| (a.0.max(v[0]), a.1.max(v[1])));
        p.outline = pts.iter().map(|v| [r1(v[0] - mx), r1(v[1] - my)]).collect();
        p.w = r1(xx - mx);
        p.h = r1(xy - my);
        for h in p.holes.iter_mut() {
            h.x = r1(h.x - mx);
            h.y = r1(h.y - my);
        }
        for k in p.pockets.iter_mut() {
            k.x = r1(k.x - mx);
            k.y = r1(k.y - my);
        }
        for c in p.cutouts.iter_mut() {
            c.x = r1(c.x - mx);
            c.y = r1(c.y - my);
        }
        p
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
    /// LED strips and the circuits that feed them.
    pub lights: Vec<crate::lighting::Light>,
    pub circuits: Vec<crate::lighting::Circuit>,
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
            more3d: vec![],
            band_segs: vec![],
            shared: vec![],
            rot: 0.0,
            shape: vec![],
        });
        self.out.parts.len() - 1
    }

    fn hw(&mut self, name: impl Into<String>, qty: u32) {
        if qty == 0 {
            return;
        }
        let name = name.into();
        if let Some(h) = self.out.hardware.iter_mut().find(|h| h.cabinet == self.cab.id && h.name == name) {
            h.qty += qty;
        } else {
            self.out.hardware.push(Hardware { cabinet: self.cab.id.clone(), name, qty });
        }
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.out.warnings.push(format!("{} ({}): {}", self.cab.name, self.cab.id, msg.into()));
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

/// Heights of the corner-joint bores in a drawer box of height `bh`,
/// clear of the bottom groove (10 mm up).
fn box_joint_y(bh: f64) -> Vec<f64> {
    if bh >= 90.0 {
        vec![(bh * 0.35).max(36.0), bh - 25.0]
    } else {
        vec![(bh / 2.0 + 6.0).max(36.0)]
    }
}

/// Hinge cup centres on a door of height `h` whose bottom edge is at `y0`
/// in the cabinet, moved (at most 16 mm) so each hinge plate's two screws
/// (cup centre ± 16) land on the 32 mm row of system holes that starts at
/// `row0`: plate and shelf-pin holes then share bores instead of clashing.
fn hinges_on_rows(h: f64, y0: f64, row0: f64) -> Vec<f64> {
    hinge_positions(h)
        .into_iter()
        .map(|y| {
            let plate = y0 + y - 16.0;
            let snapped = row0 + ((plate - row0) / 32.0).round() * 32.0;
            (snapped + 16.0 - y0).clamp(60.0, h - 60.0)
        })
        .collect()
}

fn hinge_positions(h: f64) -> Vec<f64> {
    let n = hinge_count(h);
    let (a, b) = (100.0, h - 100.0);
    (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect()
}

/// Build every part and hardware item for a whole project.
pub fn build(project: &Project) -> Build {
    let mut out = Build::default();
    for cab in &project.cabinets {
        build_cabinet(project, cab, &mut out);
    }
    let joints = build_worktops(project, &mut out);
    if project.settings.plinth_runs {
        build_plinths(project, &mut out);
    }
    if project.settings.cornice > 0.0 {
        build_trim(project, &mut out, Trim::Cornice);
    }
    if project.settings.light_rail > 0.0 {
        build_trim(project, &mut out, Trim::LightRail);
    }
    crate::lighting::plan(project, &mut out);
    crate::edits::apply(project, &mut out);
    crate::install::plan(project, &mut out, &joints);
    out
}

/// One plinth per straight run of floor cabinets whose plinth fronts line
/// up (same rotation, back line, plinth height and plinth front), split
/// into equal lengths when longer than a board. Free ends without an end
/// panel get a return back to the wall line.
#[derive(Clone, Copy, PartialEq)]
enum Trim {
    Cornice,
    LightRail,
}

/// Cornice on top of, or light rail under, each straight run of upper
/// cabinets whose tops (or bottoms) and fronts line up: a board in the front
/// material flush with the doors, wrapping round free ends with returns.
fn build_trim(project: &Project, out: &mut Build, kind: Trim) {
    let s = &project.settings;
    let th = if kind == Trim::Cornice { s.cornice } else { s.light_rail };
    let r2 = |v: f64| (v * 2.0).round() / 2.0;
    let level = |c: &Cabinet| if kind == Trim::Cornice { c.elevation + c.height } else { c.elevation };
    let mut cabs: Vec<(&Cabinet, f64, f64)> = project
        .cabinets
        .iter()
        .filter(|c| match kind {
            Trim::Cornice => matches!(c.kind, CabinetKind::Wall | CabinetKind::Tall),
            Trim::LightRail => c.kind == CabinetKind::Wall,
        })
        .filter(|c| !c.is_diagonal())
        .map(|c| {
            let (sn, cs) = c.rotation.to_radians().sin_cos();
            (c, c.x * cs - c.z * sn, r2(c.x * sn + c.z * cs))
        })
        .collect();
    let key = |c: &Cabinet, across: f64| (r2(c.rotation.rem_euclid(360.0)), across, r2(level(c)), r2(c.depth));
    cabs.sort_by(|a, b| (key(a.0, a.2), a.1).partial_cmp(&(key(b.0, b.2), b.1)).unwrap_or(std::cmp::Ordering::Equal));
    let mut runs: Vec<Vec<(&Cabinet, f64)>> = vec![];
    let mut last = None;
    for (c, along, across) in cabs {
        let k = key(c, across);
        if let (Some(r), Some(lk)) = (runs.last_mut(), last) {
            let (p, pa): (&Cabinet, f64) = r[r.len() - 1];
            if lk == k && (along - (pa + p.width)).abs() < 1.0 {
                r.push((c, along));
                continue;
            }
        }
        last = Some(k);
        runs.push(vec![(c, along)]);
    }
    let walls = project.room.walls();
    // Nothing at this height (wall or cabinet) right beside a run end.
    let free = |c: &Cabinet, lx: f64, lz: f64, y: f64| -> bool {
        let (wx, wz) = to_world(c.x, c.z, c.rotation, lx, lz);
        let by_cab = project.cabinets.iter().any(|o| {
            if o.elevation > y + 1.0 || o.elevation + o.height < y - 1.0 {
                return false;
            }
            let (x, z) = to_local(o.x, o.z, o.rotation, wx, wz);
            o.rects().iter().any(|r| x > r[0] - 0.5 && x < r[2] + 0.5 && z > r[1] - 0.5 && z < r[3] + 0.5)
        });
        let by_wall = walls.iter().any(|w| {
            let (t, s2) = w.local(wx, wz);
            s2 > -200.0 && s2 < 40.0 && t > -1.0 && t < w.len + 1.0
        });
        !by_cab && !by_wall
    };
    let (key0, name) = if kind == Trim::Cornice { ("cornice", "Cornice") } else { ("lightrail", "Light rail") };
    for r in runs {
        let start = out.parts.len();
        let (first, a0) = r[0];
        let (lastc, a1) = r[r.len() - 1];
        let len = a1 + lastc.width - a0;
        let fm = project.material(&first.front_material);
        let ft = fm.thickness;
        let d = first.depth;
        let y = if kind == Trim::Cornice { first.height } else { -th }; // cabinet-local
        let probe_y = if kind == Trim::Cornice { level(first) - 10.0 } else { level(first) + 10.0 };
        // outside any end panels, then the return boards
        let pl = if first.end_left { ft } else { 0.0 };
        let pr = if lastc.end_right { ft } else { 0.0 };
        let fl = free(first, -pl - 30.0, d / 2.0, probe_y);
        let fr = free(lastc, lastc.width + pr + 30.0, d / 2.0, probe_y);
        let x0 = -pl - if fl { ft } else { 0.0 };
        let x1 = len + pr + if fr { ft } else { 0.0 };
        let max = (fm.sheet_length - 2.0 * s.sheet_trim).max(500.0);
        let n = ((x1 - x0) / max).ceil().max(1.0) as usize;
        let piece = (x1 - x0) / n as f64;
        for k in 0..n {
            let key = if n == 1 { key0.to_string() } else { format!("{key0}{}", k + 1) };
            let label = if n == 1 { name.to_string() } else { format!("{name} {}/{}", k + 1, n) };
            push_part(
                out,
                first,
                &key,
                &label,
                Role::Front,
                &fm,
                piece,
                th,
                [true, false, true, false],
                Box3 { pos: [x0 + piece * k as f64, y, d], size: [piece, th, ft] },
            );
        }
        if fl {
            push_part(
                out,
                first,
                &format!("{key0}-rl"),
                &format!("{name} return L"),
                Role::Front,
                &fm,
                d,
                th,
                [true, false, true, false],
                Box3 { pos: [x0, y, 0.0], size: [ft, th, d] },
            );
        }
        if fr {
            push_part(
                out,
                first,
                &format!("{key0}-rr"),
                &format!("{name} return R"),
                Role::Front,
                &fm,
                d,
                th,
                [true, false, true, false],
                Box3 { pos: [x1 - ft, y, 0.0], size: [ft, th, d] },
            );
        }
        let shares: Vec<(String, f64)> = r.iter().map(|(c, _)| (c.id.clone(), c.width / len)).collect();
        for p in &mut out.parts[start..] {
            p.shared = shares.clone();
        }
    }
}

fn build_plinths(project: &Project, out: &mut Build) {
    let s = &project.settings;
    let r2 = |v: f64| (v * 2.0).round() / 2.0;
    let mut cabs: Vec<(&Cabinet, f64, f64)> = project
        .cabinets
        .iter()
        .filter(|c| c.on_run_plinth())
        .map(|c| {
            let (sn, cs) = c.rotation.to_radians().sin_cos();
            (c, c.x * cs - c.z * sn, r2(c.x * sn + c.z * cs))
        })
        .collect();
    let front = |c: &Cabinet| r2(c.depth - c.toe_kick_depth.clamp(0.0, c.depth * 0.5));
    let key = |c: &Cabinet, across: f64| (r2(c.rotation.rem_euclid(360.0)), across, r2(c.toe_kick_height), front(c));
    cabs.sort_by(|a, b| (key(a.0, a.2), a.1).partial_cmp(&(key(b.0, b.2), b.1)).unwrap_or(std::cmp::Ordering::Equal));
    let mut runs: Vec<Vec<(&Cabinet, f64)>> = vec![];
    let mut last = None;
    for (c, along, across) in cabs {
        let k = key(c, across);
        if let (Some(r), Some(lk)) = (runs.last_mut(), last) {
            let (p, pa): (&Cabinet, f64) = r[r.len() - 1];
            if lk == k && (along - (pa + p.width)).abs() < 1.0 {
                r.push((c, along));
                continue;
            }
        }
        last = Some(k);
        runs.push(vec![(c, along)]);
    }
    let walls = project.room.walls();
    // Nothing (wall or cabinet on the floor) right beside a run end.
    let free = |c: &Cabinet, lx: f64, lz: f64| -> bool {
        let (wx, wz) = to_world(c.x, c.z, c.rotation, lx, lz);
        let by_cab = project.cabinets.iter().any(|o| {
            if o.elevation > 50.0 {
                return false;
            }
            let (x, z) = to_local(o.x, o.z, o.rotation, wx, wz);
            o.rects().iter().any(|r| x > r[0] - 0.5 && x < r[2] + 0.5 && z > r[1] - 0.5 && z < r[3] + 0.5)
        });
        let by_wall = walls.iter().any(|w| {
            let (t, s2) = w.local(wx, wz);
            s2 > -200.0 && s2 < 40.0 && t > -1.0 && t < w.len + 1.0 // close to a wall, or just behind it
        });
        !by_cab && !by_wall
    };
    for r in runs {
        let start = out.parts.len();
        let (first, a0) = r[0];
        let (lastc, a1) = r[r.len() - 1];
        let len = a1 + lastc.width - a0;
        let cm = project.material(&first.carcass_material);
        let t = cm.thickness;
        let toe = first.toe_kick_height;
        let zf = front(first); // plinth front face, first cabinet's local z
        let name = if s.plinth == Plinth::Legs { "Plinth" } else { "Toe kick" };
        let max = (cm.sheet_length - 2.0 * s.sheet_trim).max(500.0);
        let n = (len / max).ceil().max(1.0) as usize;
        let piece = len / n as f64;
        for k in 0..n {
            let key = if n == 1 { "plinth".to_string() } else { format!("plinth{}", k + 1) };
            let label = if n == 1 { name.to_string() } else { format!("{name} {}/{}", k + 1, n) };
            push_part(
                out,
                first,
                &key,
                &label,
                Role::Carcass,
                &cm,
                piece,
                toe,
                [false, false, true, false],
                Box3 { pos: [piece * k as f64, 0.0, zf - t], size: [piece, toe, t] },
            );
        }
        // returns: from the back of the plinth to 20 mm off the back line; ends
        // within 30 mm of a wall or cabinet count as closed
        let rl = zf - t - 20.0;
        let mid = zf / 2.0;
        if rl > 50.0 {
            if !first.end_left && free(first, -30.0, mid) {
                push_part(
                    out,
                    first,
                    "plinth-rl",
                    &format!("{name} return L"),
                    Role::Carcass,
                    &cm,
                    rl,
                    toe,
                    [false, false, true, false],
                    Box3 { pos: [0.0, 0.0, 20.0], size: [t, toe, rl] },
                );
            }
            if !lastc.end_right && free(lastc, lastc.width + 30.0, mid) {
                push_part(
                    out,
                    first,
                    "plinth-rr",
                    &format!("{name} return R"),
                    Role::Carcass,
                    &cm,
                    rl,
                    toe,
                    [false, false, true, false],
                    Box3 { pos: [len - t, 0.0, 20.0], size: [t, toe, rl] },
                );
            }
        }
        if r.iter().any(|(c, _)| c.led_plinth) {
            crate::lighting::plinth(first, len, zf, out);
        }
        let shares: Vec<(String, f64)> = r.iter().map(|(c, _)| (c.id.clone(), c.width / len)).collect();
        for p in &mut out.parts[start..] {
            p.shared = shares.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_part(
    out: &mut Build,
    cab: &Cabinet,
    key: &str,
    name: &str,
    role: Role,
    mat: &Material,
    w: f64,
    h: f64,
    edges: [bool; 4],
    box3d: Box3,
) {
    let mut cx = Ctx { cab, out };
    cx.part(key, name, role, mat, w, h, false, edges, box3d);
}

/// Replace slab fronts from `start` on with five-piece Shaker fronts
/// when the cabinet asks for them and the front is big enough.
fn shakerize(project: &Project, cab: &Cabinet, out: &mut Build, start: usize) {
    let s = &project.settings;
    // Glass doors: always a five-piece frame; the glass is bought, not cut.
    let is_door = |p: &Part| p.role == Role::Front && (p.name.starts_with("Door") || p.name.starts_with("Upper door"));
    if cab.glass {
        let f = s.shaker_frame;
        let pm = project.material(&cab.panel_material);
        let mut i = start;
        while i < out.parts.len() {
            let p = &out.parts[i];
            if is_door(p) && p.w.min(p.h) >= 2.0 * f + 60.0 {
                let p = out.parts.remove(i);
                let mut pieces = shaker_pieces(p, f, s.shaker_groove, &pm);
                let glass = pieces.pop().expect("panel piece");
                let name = format!("Glass 4 mm {:.0}×{:.0}", glass.w, glass.h);
                match out.hardware.iter_mut().find(|h| h.cabinet == cab.id && h.name == name) {
                    Some(h) => h.qty += 1,
                    None => out.hardware.push(Hardware { cabinet: cab.id.clone(), name, qty: 1 }),
                }
                out.placeholders.push(Placeholder { cabinet: cab.id.clone(), label: "Glass".into(), box3d: glass.box3d });
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
        if toe > 0.0 && cab.kind != CabinetKind::Appliance && !(s.plinth_runs && cab.on_run_plinth()) {
            cx.part(
                "toe",
                "Toe kick",
                Role::Carcass,
                &cm,
                w,
                toe,
                false,
                [false, false, true, false],
                Box3 { pos: [0.0, 0.0, d - tkd - t], size: [w, toe, t] },
            );
        }
    };
    match cab.kind {
        CabinetKind::Filler => {
            cx.part("filler", "Filler", Role::Front, &fm, w, h - toe, true, [true; 4], Box3 { pos: [0.0, toe, d], size: [w, h - toe, ft] });
            toe_board(&mut cx);
        }
        CabinetKind::Dishwasher => {
            let (fw, fh) = (w - g, h - toe - g);
            let idx = cx.part(
                "panel",
                "Dishwasher panel",
                Role::Front,
                &fm,
                fw,
                fh,
                false,
                [true; 4],
                Box3 { pos: [g / 2.0, toe + g / 2.0, d], size: [fw, fh, ft] },
            );
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
            // An L corner's right end is its return arm's end side.
            let b = if key == "end-r" && cab.is_l() {
                Box3 { pos: [w - d - ft, 0.0, cab.width2], size: [d + ft, h, ft] }
            } else {
                Box3 { pos: [x, 0.0, 0.0], size: [ft, h, d + ft] }
            };
            cx.part(key, name, Role::Front, &fm, d + ft, h, true, [false, false, true, true], b);
        }
    }
    if cab.back_finished && cab.is_l() {
        cx.warn("finished backs are not built on L corners");
    } else if cab.back_finished {
        // covers the end panels too
        let x0 = if cab.end_left { -ft } else { 0.0 };
        let x1 = if cab.end_right { w + ft } else { w };
        cx.part(
            "back-fin",
            "Finished back",
            Role::Front,
            &fm,
            x1 - x0,
            h,
            true,
            [false, true, true, true],
            Box3 { pos: [x0, 0.0, -ft], size: [x1 - x0, h, ft] },
        );
    }
}

/// Split a slab front into a five-piece Shaker door. Holes go to the
/// piece that contains their centre, in that piece's own frame.
fn shaker_pieces(p: Part, f: f64, g: f64, pm: &Material) -> Vec<Part> {
    let (w, h) = (p.w, p.h);
    let [x0, y0, z0] = p.box3d.pos;
    // Fronts on an L corner's return face lie in the y-z plane.
    let along_z = p.box3d.size[0] < p.box3d.size[2];
    let ft = if along_z { p.box3d.size[0] } else { p.box3d.size[2] };
    let rot = p.rot;
    // (across the face, up, into the thickness) -> box
    let fb = |u: f64, v: f64, off: f64, du: f64, dv: f64, th: f64| {
        if rot != 0.0 {
            // angled front: pieces turn with it about the door's corner
            let (wx, wz) = to_world(x0, z0, rot, u, off);
            Box3 { pos: [wx, y0 + v, wz], size: [du, dv, th] }
        } else if along_z {
            Box3 { pos: [x0 + off, y0 + v, z0 + u], size: [th, dv, du] }
        } else {
            Box3 { pos: [x0 + u, y0 + v, z0 + off], size: [du, dv, th] }
        }
    };
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
        more3d: vec![],
        band_segs: vec![],
        shared: vec![],
        rot: p.rot,
        shape: vec![],
    };
    let fm = Material { id: p.material.clone(), thickness: p.thickness, ..Material::default() };
    let rail_len = w - 2.0 * f + 2.0 * g;
    let mut out = vec![
        piece("stl", "stile L", &fm, f, h, true, fb(0.0, 0.0, 0.0, f, h, ft)),
        piece("str", "stile R", &fm, f, h, true, fb(w - f, 0.0, 0.0, f, h, ft)),
        piece("rb", "rail B", &fm, rail_len, f, false, fb(f, 0.0, 0.0, w - 2.0 * f, f, ft)),
        piece("rt", "rail T", &fm, rail_len, f, false, fb(f, h - f, 0.0, w - 2.0 * f, f, ft)),
        piece(
            "pnl",
            "panel",
            pm,
            w - 2.0 * f + 2.0 * g - 2.0,
            h - 2.0 * f + 2.0 * g - 2.0,
            true,
            fb(f, f, (ft - pt) / 2.0, w - 2.0 * f, h - 2.0 * f, pt),
        ),
    ];
    // Visible frame edges are banded: each stile's outer edge and both ends,
    // each rail's outer edge (inner edges take the panel groove).
    out[0].edges = [true, false, true, true];
    out[1].edges = [true, true, true, false];
    out[2].edges = [true, false, false, false];
    out[3].edges = [false, false, true, false];
    for hole in p.holes {
        let (x, mut y) = (hole.x, hole.y);
        // A handle bore that would land in the thin floating panel goes to
        // the middle of the top rail instead.
        let in_panel = x >= f && x <= w - f && y >= f && y <= h - f;
        if in_panel && hole.depth <= 0.0 && hole.dia <= 6.0 {
            y = h - f / 2.0;
        }
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
fn build_worktops(project: &Project, out: &mut Build) -> Vec<String> {
    let s = &project.settings;
    let mut joints: Vec<String> = vec![];
    if s.worktop_thickness <= 0.0 {
        return joints;
    }
    let tm = project.material("top");
    let r2 = |v: f64| (v * 2.0).round() / 2.0;
    // (cabinet, position along its axis, offset across it)
    // An L corner's return arm joins the perpendicular run as a virtual
    // cabinet (id suffix "~b") starting where the main arm's depth ends.
    let virt: Vec<Cabinet> = project
        .cabinets
        .iter()
        .filter(|c| c.is_l() && c.under_worktop())
        .map(|c| {
            let (x, z) = to_world(c.x, c.z, c.rotation, c.width, c.depth);
            Cabinet {
                id: format!("{}~b", c.id),
                x,
                z,
                rotation: c.rotation - 90.0,
                width: c.width2 - c.depth,
                corner: Corner::None,
                cutout: CutoutKind::None,
                ..c.clone()
            }
        })
        .collect();
    let mut bases: Vec<(&Cabinet, f64, f64)> = project
        .cabinets
        .iter()
        .chain(virt.iter())
        .filter(|c| c.under_worktop())
        .map(|c| {
            let (sn, cs) = c.rotation.to_radians().sin_cos();
            (c, c.x * cs - c.z * sn, r2(c.x * sn + c.z * cs))
        })
        .collect();
    let key = |c: &Cabinet, across: f64| (r2(c.rotation.rem_euclid(360.0)), across, c.elevation, c.height);
    bases.sort_by(|a, b| (key(a.0, a.2), a.1).partial_cmp(&(key(b.0, b.2), b.1)).unwrap_or(std::cmp::Ordering::Equal));
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
        /// cabinets of a back-to-back run merged into this worktop, with
        /// that run's front edge (in their own local z)
        backs: Vec<(&'a Cabinet, f64)>,
        ids: String,
        start: f64,
        len: f64,
        /// back edge (first cabinet's local z) and overall depth
        z0: f64,
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
                backs: vec![],
                ids: r.iter().map(|(c, _)| c.id.trim_end_matches("~b")).collect::<Vec<_>>().join("+"),
                start: 0.0,
                len: a1 + last.width - a0,
                z0: 0.0,
                depth: r.iter().map(|(c, _)| c.depth).fold(0.0, f64::max) + s.worktop_overhang,
            }
        })
        .collect();

    // Islands: two runs back to back (opposite facing, backs on one line,
    // overlapping along it) share one worktop.
    let mut merged = vec![false; tops.len()];
    for i in 0..tops.len() {
        if merged[i] {
            continue;
        }
        for j in 0..tops.len() {
            if i == j || merged[j] || merged[i] {
                continue;
            }
            let (fi, fj) = (tops[i].first, tops[j].first);
            if ((fi.rotation - fj.rotation).rem_euclid(360.0) - 180.0).abs() > 0.5 || (fi.elevation - fj.elevation).abs() > 1.0 {
                continue;
            }
            let mut xs = vec![];
            let mut on_line = true;
            for x in [tops[j].start, tops[j].start + tops[j].len] {
                let (wx, wz) = to_world(fj.x, fj.z, fj.rotation, x, 0.0);
                let (lx, lz) = to_local(fi.x, fi.z, fi.rotation, wx, wz);
                on_line &= lz.abs() < 2.0;
                xs.push(lx);
            }
            let (b0, b1) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
            let (a0, a1) = (tops[i].start, tops[i].start + tops[i].len);
            if !on_line || b1.min(a1) - b0.max(a0) < 50.0 {
                continue;
            }
            let dj = tops[j].depth;
            let backs: Vec<(&Cabinet, f64)> = tops[j].members.iter().map(|(c, _)| (*c, dj)).collect();
            let ids = tops[j].ids.clone();
            let t = &mut tops[i];
            t.start = a0.min(b0);
            t.len = a1.max(b1) - t.start;
            t.z0 = -dj;
            t.depth += dj;
            t.backs = backs;
            t.ids = format!("{}+{}", t.ids, ids);
            merged[j] = true;
        }
    }
    let mut k = 0;
    tops.retain(|_| {
        k += 1;
        !merged[k - 1]
    });
    // Seating overhang at the back (peninsulas, single-row islands).
    for t in tops.iter_mut().filter(|t| t.backs.is_empty()) {
        let back = t.members.iter().map(|(c, _)| c.worktop_back).fold(0.0, f64::max);
        if back > 0.0 {
            t.z0 = -back;
            t.depth += back;
        }
    }

    // Run ends: cover end panels, and overhang ends that stand free
    // (no wall, no cabinet at worktop height right beside them).
    let walls = project.room.walls();
    let th = s.worktop_thickness;
    let free = |c: &Cabinet, lx: f64, lz: f64| -> bool {
        let (wx, wz) = to_world(c.x, c.z, c.rotation, lx, lz);
        let (y0, y1) = (c.elevation + c.height - 50.0, c.elevation + c.height + th);
        let by_cab = project.cabinets.iter().any(|o| {
            if o.elevation >= y1 || o.elevation + o.height <= y0 {
                return false;
            }
            let (x, z) = to_local(o.x, o.z, o.rotation, wx, wz);
            o.rects().iter().any(|r| x > r[0] - 0.5 && x < r[2] + 0.5 && z > r[1] - 0.5 && z < r[3] + 0.5)
        });
        let by_wall = walls.iter().any(|w| {
            let (t, s2) = w.local(wx, wz);
            s2.abs() < 10.0 && t > -1.0 && t < w.len + 1.0
        });
        !by_cab && !by_wall
    };
    for t in tops.iter_mut() {
        let (first, _) = t.members[0];
        let (last, last_off) = t.members[t.members.len() - 1];
        let panel = |c: &Cabinet, right: bool| {
            let on = if right { c.end_right && !c.is_l() } else { c.end_left && !c.id.ends_with("~b") };
            if on {
                project.material(&c.front_material).thickness
            } else {
                0.0
            }
        };
        let (pl, pr) = (panel(first, false), panel(last, true));
        let mid = first.depth / 2.0;
        let el = pl + if free(first, -pl - 2.0, mid) { s.worktop_end_overhang } else { 0.0 };
        let er = pr + if free(last, last.width + pr + 2.0, mid) { s.worktop_end_overhang } else { 0.0 };
        let _ = last_off;
        t.start -= el;
        t.len += el + er;
    }

    // Trim overlaps at corners: earlier (longer-established) worktop wins.
    // Joints: (worktop i, worktop j butting into it, at j's start?).
    let mut butt: Vec<(usize, usize, bool)> = vec![];
    for j in 1..tops.len() {
        for i in 0..j {
            let (fi, fj) = (tops[i].first, tops[j].first);
            if (fi.rotation - fj.rotation).rem_euclid(180.0).abs() < 1e-6 {
                continue; // parallel runs never butt
            }
            // i's footprint corners in j's local frame
            let (zi0, zi1) = (tops[i].z0, tops[i].z0 + tops[i].depth);
            let corners =
                [(tops[i].start, zi0), (tops[i].start + tops[i].len, zi0), (tops[i].start, zi1), (tops[i].start + tops[i].len, zi1)];
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
            let z_overlap = lz.1.min(t.z0 + t.depth) - lz.0.max(t.z0);
            let x_overlap = lx.1.min(b) - lx.0.max(a);
            // touching or overlapping at a corner: a joint to bolt together
            if z_overlap > -1.0 && x_overlap > -1.0 && (z_overlap > 0.5 || x_overlap > 0.5) {
                joints.push(t.first.id.trim_end_matches("~b").to_string());
                butt.push((i, j, (lx.0 + lx.1) / 2.0 < (a + b) / 2.0));
            }
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

    // Bolt recesses under each corner joint: three bolts across the joint,
    // each a Ø35 x 20 hole 75 mm from the joint with a 15 mm slot to it.
    let th = s.worktop_thickness;
    let (hole_d, slot_d) = ((th * 0.55).min(20.0), (th * 0.4).min(15.0));
    let mut recess: Vec<Vec<Pocket>> = vec![vec![]; tops.len()];
    for &(i, j, at_start) in &butt {
        let (ti, tj) = (&tops[i], &tops[j]);
        if ti.len < 50.0 || tj.len < 50.0 {
            continue;
        }
        let xe = if at_start { tj.start } else { tj.start + tj.len }; // j's end at the joint
        let inward = if at_start { 1.0 } else { -1.0 };
        let (fi, fj) = (ti.first, tj.first);
        let front_i = ti.z0 + ti.depth;
        for k in [0.2, 0.5, 0.8] {
            let z = tj.z0 + tj.depth * k;
            let front_j = tj.z0 + tj.depth;
            // on j: hole 75 in from its end, slot out to the end
            let xh = xe + inward * 75.0;
            let (yd, xd) = (front_j - z, xh - tj.start);
            recess[j].push(Pocket { x: r1(xd - 17.5), y: r1(yd - 17.5), w: 35.0, h: 35.0, depth: hole_d, r: 17.5, face: 2 });
            let (sx0, sx1) = if at_start { (0.0, xd) } else { (xd, tj.len) };
            recess[j].push(Pocket { x: r1(sx0), y: r1(yd - 7.5), w: r1(sx1 - sx0), h: 15.0, depth: slot_d, r: 0.0, face: 2 });
            // on i: the bolt line carries on 75 mm past the joint into i
            let (wx, wz) = to_world(fj.x, fj.z, fj.rotation, xe - inward * 75.0, z);
            let (xi, zi) = to_local(fi.x, fi.z, fi.rotation, wx, wz);
            let (xd, yd) = (xi - ti.start, front_i - zi);
            if xd < 20.0 || xd > ti.len - 20.0 || yd < 20.0 {
                continue;
            }
            recess[i].push(Pocket { x: r1(xd - 17.5), y: r1(yd - 17.5), w: 35.0, h: 35.0, depth: hole_d, r: 17.5, face: 2 });
            recess[i].push(Pocket { x: r1(xd - 7.5), y: 0.0, w: 15.0, h: r1(yd), depth: slot_d, r: 0.0, face: 2 });
        }
    }

    for (ti, t) in tops.into_iter().enumerate() {
        let joint_pockets = std::mem::take(&mut recess[ti]);
        if t.len < 50.0 {
            continue;
        }
        // Cut-outs: drawing x along the worktop from its (trimmed) start,
        // y from the front edge.
        let mut cutouts = vec![];
        let front = t.z0 + t.depth; // front edge, first cabinet's local z
        let f = t.first;
        let all = t.members.iter().map(|&(c, _)| (c, front)).chain(t.backs.iter().copied());
        for (c, zf) in all {
            if c.cutout == CutoutKind::None {
                continue;
            }
            let (dw, dd, dr) = c.cutout.default_size();
            let cw = if c.cutout_width > 0.0 { c.cutout_width } else { dw };
            let cd = if c.cutout_depth > 0.0 { c.cutout_depth } else { dd };
            let cr = if c.cutout_radius > 0.0 { c.cutout_radius } else { dr };
            // centre in the cabinet's own frame (zf: its run's front edge), then in the first's
            let (wx, wz) = to_world(c.x, c.z, c.rotation, c.width / 2.0, zf - c.cutout_setback - cd / 2.0);
            let (lx, lz) = to_local(f.x, f.z, f.rotation, wx, wz);
            let x = lx - cw / 2.0 - t.start;
            let y = front - lz - cd / 2.0;
            let label = format!("{} ({}) {:?} cut-out", c.name, c.id, c.cutout).to_lowercase();
            if x < 50.0 || x + cw > t.len - 50.0 || y < 30.0 || y + cd > t.depth - 30.0 {
                out.warnings.push(format!("{label} {cw:.0}×{cd:.0} leaves less than 50 mm to the worktop edge"));
            } else if cw > c.width - 2.0 * project.material(&c.carcass_material).thickness {
                out.warnings.push(format!("{label} is wider than the cabinet interior"));
            }
            cutouts.push(Cutout { kind: c.cutout, x: r1(x), y: r1(y), w: cw, h: cd, r: cr });
        }
        // Placed in the frame of the first real cabinet of the run; a run
        // that is only an L's return arm goes in that L's own frame.
        let real_anchor = t.members.iter().find(|(c, _)| !c.id.ends_with("~b")).map(|(c, _)| *c);
        let (anchor, box3d, suffix) = match t.members.iter().find(|(c, _)| !c.id.ends_with("~b")) {
            Some((c, off)) => (c.id.clone(), Box3 { pos: [t.start - off, c.height, t.z0], size: [t.len, th, t.depth] }, ""),
            None => {
                let id = t.first.id.trim_end_matches("~b").to_string();
                let real = project.cabinets.iter().find(|c| c.id == id).unwrap_or(t.first);
                let b = Box3 { pos: [real.width - t.z0 - t.depth, real.height, real.depth + t.start], size: [t.depth, th, t.len] };
                (id, b, "2")
            }
        };
        // Upstands: along the back edge where it meets a wall, and up each
        // end that stops at a wall (not for an L's lone return arm).
        let upstands: Vec<(&str, &str, Box3)> = match (real_anchor, s.upstand > 0.0) {
            (Some(c), true) => {
                let [bx, by, bz] = box3d.pos;
                let [len, bt, dep] = box3d.size;
                let um = project.material("upstand");
                let (uh, ut) = (s.upstand, um.thickness);
                let at_wall = |lx: f64, lz: f64| {
                    let (wx, wz) = to_world(c.x, c.z, c.rotation, lx, lz);
                    walls.iter().any(|w| {
                        let (tt, s2) = w.local(wx, wz);
                        s2 > -200.0 && s2 < 15.0 && tt > -1.0 && tt < w.len + 1.0
                    })
                };
                let y = by + bt;
                let back = at_wall(bx + len / 2.0, bz);
                let z1 = bz + if back { ut } else { 0.0 };
                let mut v = vec![];
                if back {
                    v.push(("upstand", "Upstand", Box3 { pos: [bx, y, bz], size: [len, uh, ut] }));
                }
                if at_wall(bx - 30.0, bz + dep / 2.0) {
                    v.push(("upstand-l", "Upstand L", Box3 { pos: [bx, y, z1], size: [ut, uh, bz + dep - z1] }));
                }
                if at_wall(bx + len + 30.0, bz + dep / 2.0) {
                    v.push(("upstand-r", "Upstand R", Box3 { pos: [bx + len - ut, y, z1], size: [ut, uh, bz + dep - z1] }));
                }
                v
            }
            _ => vec![],
        };
        for (key, name, b) in upstands {
            let l = b.size[0].max(b.size[2]);
            out.parts.push(Part {
                id: format!("{}-{}{}", anchor, key, suffix),
                cabinet: anchor.clone(),
                name: name.into(),
                role: Role::Worktop,
                material: "upstand".into(),
                thickness: project.material("upstand").thickness,
                w: r1(l),
                h: r1(s.upstand),
                grain_y: false,
                edges: [false; 4],
                outline: rect(r1(l), r1(s.upstand)),
                holes: vec![],
                cutouts: vec![],
                pockets: vec![],
                box3d: b,
                more3d: vec![],
                band_segs: vec![],
                shared: vec![],
                rot: 0.0,
                shape: vec![],
            });
        }
        out.parts.push(Part {
            id: format!("{}-worktop{}", anchor, suffix),
            cabinet: anchor,
            name: "Worktop".into(),
            role: Role::Worktop,
            material: tm.id.clone(),
            thickness: th,
            w: r1(t.len),
            h: r1(t.depth),
            grain_y: false,
            // solid / laminated tops are finished, not edge-banded
            edges: [false; 4],
            outline: rect(r1(t.len), r1(t.depth)),
            holes: vec![],
            cutouts,
            pockets: joint_pockets,
            box3d,
            more3d: vec![],
            band_segs: vec![],
            shared: vec![],
            rot: 0.0,
            shape: vec![],
        });
        if t.len > tm.sheet_length - 2.0 * s.sheet_trim {
            out.warnings.push(format!("Worktop over {} is {:.0} mm, longer than the {} blank", t.ids, t.len, tm.name));
        }
    }
    joints
}

fn build_cabinet(project: &Project, cab: &Cabinet, out: &mut Build) {
    if !cab.kind.is_carcass() {
        build_special(project, cab, out);
        return;
    }
    if cab.is_l() {
        build_lcorner(project, cab, out);
    } else {
        build_carcass(project, cab, out);
    }
    end_panels(project, cab, out);
    crate::lighting::cabinet(project, cab, out);
    crate::accessories::cabinet(project, cab, out);
}

/// Map local (x, z) points to an L panel's drawing frame (seen from
/// above, front down): x_d = x - x0, y_d = y1 - z. Counter-clockwise.
fn plan_outline(pts: &[(f64, f64)], x0: f64, z1: f64) -> Vec<[f64; 2]> {
    let mut o: Vec<[f64; 2]> = pts.iter().map(|&(x, z)| [r1(x - x0), r1(z1 - z)]).collect();
    let a2: f64 = (0..o.len())
        .map(|i| {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if a2 < 0.0 {
        o.reverse();
    }
    o
}

/// Outline segments whose both ends satisfy `on`.
fn segs_where(o: &[[f64; 2]], on: impl Fn([f64; 2], [f64; 2]) -> bool) -> Vec<usize> {
    (0..o.len()).filter(|&i| on(o[i], o[(i + 1) % o.len()])).collect()
}

/// L-shaped corner unit. Arm A runs along the back (x 0..W, z 0..D),
/// arm B along the right (x W-D..W, z 0..W2). Applied backs on both
/// walls, L bottom/top/shelves, a left side and an end side, and a
/// bi-fold door pair across the two open faces.
fn build_lcorner(project: &Project, cab: &Cabinet, out: &mut Build) {
    let s = &project.settings;
    let cm = project.material(&cab.carcass_material);
    let bm = project.material(&cab.back_material);
    let fm = project.material(&cab.front_material);
    let start = out.parts.len();
    let mut cx = Ctx { cab, out };
    let (w, h, d, w2) = (cab.width, cab.height, cab.depth, cab.width2);
    let (t, bt, ft, g) = (cm.thickness, bm.thickness, fm.thickness, s.front_gap);
    let diag = cab.is_diagonal();
    if cab.corner == Corner::Diagonal && !diag {
        cx.warn("diagonal corners are wall cabinets; built as an L");
    }
    let min_arm = if diag { 50.0 } else { 250.0 };
    if w < d + min_arm || w2 < d + min_arm || h < 2.0 * t + 50.0 || d < bt + 100.0 {
        cx.warn(format!("too small for a corner cabinet: each arm needs at least depth + {min_arm:.0} mm"));
        return;
    }
    if !cab.drawers.is_empty() {
        cx.warn("drawers are not built in L corner cabinets");
    }
    if s.back_mode == BackMode::Grooved {
        cx.warn("L corners use applied backs");
    }
    let toe = if cab.kind == CabinetKind::Wall { 0.0 } else { cab.toe_kick_height.max(0.0) };
    let tkd = cab.toe_kick_depth.clamp(0.0, d * 0.5);
    let legs = s.plinth == Plinth::Legs && toe > 0.0;
    let side_y0 = if legs { toe } else { 0.0 };
    let side_h = h - side_y0;
    let cd = d - bt; // side depth
    let xb = w - d; // arm B front face (local x)
    let xr = w - bt; // inside of back B
    let ze = w2 - t; // inside of the end side
    let sl = s.system_line;
    let (side_dia, side_depth, edge_dia, edge_depth) = match s.joinery {
        Joinery::Confirmat => (7.0, 0.0, 5.0, 45.0),
        Joinery::Dowel => (8.0, 12.0, 8.0, 25.0),
    };
    // Side holes, x from the front of each side, y from the floor.
    let mut side_l: Vec<Hole> = vec![];
    let mut side_e: Vec<Hole> = vec![];
    let at = [50.0, cd / 2.0, cd - 50.0];

    // Diagonal front: the line from the left side's front corner (0, d) to the
    // end side's (xb, w2); `dn` is its outward normal (towards the room).
    let dl = (xb * xb + (w2 - d) * (w2 - d)).sqrt();
    let dd = (xb / dl, (w2 - d) / dl);
    let dn = (-dd.1, dd.0);
    // ---------- L panels: bottom, top ----------
    let l_panel = |cx: &mut Ctx, key: &str, name: &str, role: Role, y: f64, inset: f64, notch: f64| -> usize {
        // inset: clearance from sides/backs (shelves); notch: setback from the fronts
        let (x0, x1, z0, z1) = (t + inset, xr - inset, bt + inset, ze - inset);
        if diag {
            // pentagon: the diagonal moved back by `notch`, cut by the left side line and the end side line
            let (px, pz) = (-dn.0 * notch, d - dn.1 * notch);
            let zp = pz + (x0 - px) / dd.0 * dd.1;
            let xq = px + (z1 - pz) / dd.1 * dd.0;
            let pts = [(x0, z0), (x1, z0), (x1, z1), (xq, z1), (x0, zp)];
            let idx =
                cx.part(key, name, role, &cm, x1 - x0, z1 - z0, false, [false; 4], Box3 { pos: [x0, y, z0], size: [x1 - x0, t, z1 - z0] });
            let p = &mut cx.out.parts[idx];
            p.outline = plan_outline(&pts, x0, z1);
            let (a, b) = ([r1(0.0), r1(z1 - zp)], [r1(xq - x0), r1(0.0)]);
            p.band_segs = segs_where(&p.outline, |u, v| (u == a && v == b) || (u == b && v == a));
            p.shape = pts.iter().map(|&(x, z)| [r1(x), r1(z)]).collect();
            return idx;
        }
        let (xn, zn) = (xb + notch, d - notch);
        let pts = [(x0, z0), (x1, z0), (x1, z1), (xn, z1), (xn, zn), (x0, zn)];
        let idx =
            cx.part(key, name, role, &cm, x1 - x0, z1 - z0, false, [false; 4], Box3 { pos: [x0, y, z0], size: [x1 - x0, t, zn - z0] });
        let p = &mut cx.out.parts[idx];
        p.outline = plan_outline(&pts, x0, z1);
        // banded: the two front edges (drawing: y_d = z1 - zn, x_d = xn - x0)
        let (yf, xf) = (r1(z1 - zn), r1(xn - x0));
        p.band_segs = segs_where(&p.outline, |a, b| {
            (a[1] == yf && b[1] == yf && a[0].max(b[0]) <= xf + 0.01)
                || (a[0] == xf && b[0] == xf && a[1].min(b[1]) >= -0.01 && a[1].max(b[1]) <= yf + 0.01)
        });
        p.more3d.push(Box3 { pos: [xn, y, zn], size: [x1 - xn, t, z1 - zn] });
        idx
    };
    let mut joined = vec![];
    for (key, name, y) in [("bottom", "Bottom", toe), ("top", "Top", h - t)] {
        let idx = l_panel(&mut cx, key, name, Role::Carcass, y, 0.0, 0.0);
        for &x in &at {
            let hole = Hole { x, y: y + t / 2.0, dia: side_dia, depth: side_depth, edge: None };
            side_l.push(hole.clone());
            side_e.push(hole);
        }
        joined.push(idx);
    }
    // Edge bores: left edge (x_d = 0) at z = d - x; end edge (y_d = 0) at x = xb + x.
    for idx in joined {
        let p = &mut cx.out.parts[idx];
        for &a in &at {
            p.holes.push(Hole { x: 0.0, y: r1(ze - (d - a)), dia: edge_dia, depth: edge_depth, edge: Some(3) });
            p.holes.push(Hole { x: r1(xb + a - t), y: 0.0, dia: edge_dia, depth: edge_depth, edge: Some(0) });
        }
    }

    // ---------- backs ----------
    cx.part("back", "Back", Role::Back, &bm, w, h - toe, true, [false; 4], Box3 { pos: [0.0, toe, 0.0], size: [w, h - toe, bt] });
    cx.part(
        "back2",
        "Back 2",
        Role::Back,
        &bm,
        w2 - bt,
        h - toe,
        true,
        [false; 4],
        Box3 { pos: [xr, toe, bt], size: [bt, h - toe, w2 - bt] },
    );

    // ---------- toe kicks ----------
    if toe > 0.0 {
        let tn = if legs { "Plinth" } else { "Toe kick" };
        let za = d - tkd - t; // back face of toe A
        let xa = xb + tkd; // front face of toe B
        cx.part("toe", tn, Role::Carcass, &cm, xa, toe, false, [false, false, true, false], Box3 { pos: [t, 0.0, za], size: [xa, toe, t] });
        cx.part(
            "toe2",
            &format!("{tn} 2"),
            Role::Carcass,
            &cm,
            ze - (za + t),
            toe,
            false,
            [false, false, true, false],
            Box3 { pos: [xa, 0.0, za + t], size: [t, toe, ze - (za + t)] },
        );
        if legs {
            cx.hw(format!("Adjustable leg {:.0} mm", toe), 7);
            cx.hw("Plinth clip", 4);
        }
    }

    // ---------- door across the diagonal ----------
    if diag && cab.doors > 0 && h - toe > 100.0 {
        let fh = h - toe - g;
        let fw = dl - g;
        let rot = (-dd.1).atan2(dd.0).to_degrees(); // local x along the diagonal, local z = outward
        let idx = cx.part(
            "door1",
            "Door 1",
            Role::Front,
            &fm,
            fw,
            fh,
            true,
            [true; 4],
            Box3 { pos: [dd.0 * g / 2.0, toe + g / 2.0, d + dd.1 * g / 2.0], size: [fw, fh, ft] },
        );
        cx.out.parts[idx].rot = rot;
        let hy = hinges_on_rows(fh, toe + g / 2.0, toe + t + 64.0);
        for &y in &hy {
            cx.out.parts[idx].holes.push(Hole { x: r1(fw - s.hinge_cup_inset), y: r1(y), dia: 35.0, depth: 13.0, edge: None });
            let yw = toe + g / 2.0 + y;
            side_l.push(Hole { x: sl, y: yw - 16.0, dia: 5.0, depth: 12.0, edge: None });
            side_l.push(Hole { x: sl, y: yw + 16.0, dia: 5.0, depth: 12.0, edge: None });
        }
        cx.hw("Clip hinge +45° + plate", hy.len() as u32);
        if cab.handles {
            let sp = s.handle_spacing;
            let yc = 60.0 + sp / 2.0;
            let p = &mut cx.out.parts[idx];
            p.holes.push(Hole { x: 40.0, y: r1(yc - sp / 2.0), dia: 5.0, depth: 0.0, edge: None });
            p.holes.push(Hole { x: 40.0, y: r1(yc + sp / 2.0), dia: 5.0, depth: 0.0, edge: None });
            cx.hw(format!("Handle {}mm", sp), 1);
        }
    }
    // ---------- doors: bi-fold pair ----------
    if !diag && cab.doors > 0 && h - toe > 100.0 {
        let fh = h - toe - g;
        let y0 = toe + g / 2.0;
        let wa = xb - ft - g; // leaf A, on the left side
        let wb = w2 - d - g; // leaf B, folds onto A
        let a = cx.part("door1", "Door 1", Role::Front, &fm, wa, fh, true, [true; 4], Box3 { pos: [g / 2.0, y0, d], size: [wa, fh, ft] });
        let b = cx.part(
            "door2",
            "Door 2",
            Role::Front,
            &fm,
            wb,
            fh,
            true,
            [true; 4],
            Box3 { pos: [xb - ft, y0, d + g / 2.0], size: [ft, fh, wb] },
        );
        let hy = hinges_on_rows(fh, y0, toe + t + 64.0);
        let inset = s.hinge_cup_inset;
        for &y in &hy {
            // inside face up: A's left-side hinge on the drawing right, fold edge on the left;
            // B's fold edge (near the corner) on its drawing right.
            cx.out.parts[a].holes.push(Hole { x: r1(wa - inset), y: r1(y), dia: 35.0, depth: 13.0, edge: None });
            cx.out.parts[a].holes.push(Hole { x: inset, y: r1(y), dia: 35.0, depth: 13.0, edge: None });
            cx.out.parts[b].holes.push(Hole { x: r1(wb - inset), y: r1(y), dia: 35.0, depth: 13.0, edge: None });
            let yw = y0 + y;
            side_l.push(Hole { x: sl, y: yw - 16.0, dia: 5.0, depth: 12.0, edge: None });
            side_l.push(Hole { x: sl, y: yw + 16.0, dia: 5.0, depth: 12.0, edge: None });
        }
        cx.hw("Clip hinge 110° + plate", hy.len() as u32);
        cx.hw("Bi-fold corner hinge 170°", hy.len() as u32);
        if cab.handles {
            let sp = s.handle_spacing;
            let yc = if cab.kind == CabinetKind::Wall { 60.0 + sp / 2.0 } else { fh - 60.0 - sp / 2.0 };
            let p = &mut cx.out.parts[b];
            p.holes.push(Hole { x: 40.0, y: r1(yc - sp / 2.0), dia: 5.0, depth: 0.0, edge: None });
            p.holes.push(Hole { x: 40.0, y: r1(yc + sp / 2.0), dia: 5.0, depth: 0.0, edge: None });
            cx.hw(format!("Handle {}mm", sp), 1);
        }
    }

    // ---------- shelves ----------
    let (in_lo, in_hi) = (toe + t, h - t);
    if cab.shelves > 0 && in_hi - in_lo > 150.0 {
        let mut rows = vec![];
        let mut y = in_lo + 64.0;
        while y < in_hi - 64.0 {
            rows.push(y);
            y += 32.0;
        }
        for &y in &rows {
            for x in [sl, cd - sl] {
                side_l.push(Hole { x, y, dia: 5.0, depth: 12.0, edge: None });
                side_e.push(Hole { x, y, dia: 5.0, depth: 12.0, edge: None });
            }
        }
        let n = cab.shelves;
        for i in 0..n {
            let target = in_lo + (in_hi - in_lo) * (i + 1) as f64 / (n + 1) as f64;
            let ys = rows.iter().copied().min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs())).unwrap_or(target);
            l_panel(&mut cx, &format!("shelf{}", i + 1), &format!("Shelf {}", i + 1), Role::Shelf, ys, 1.0, s.shelf_setback);
        }
        cx.hw("Shelf pin 5mm", 4 * n);
        if !diag {
            cx.hw("Shelf support bracket", n);
        }
    }

    shakerize(project, cab, cx.out, start);

    // ---------- sides ----------
    let notch = |p: &mut Part| {
        if toe > 0.0 && tkd > 0.0 && !legs {
            p.outline = vec![[0.0, r1(toe)], [r1(tkd), r1(toe)], [r1(tkd), 0.0], [r1(cd), 0.0], [r1(cd), r1(h)], [0.0, r1(h)]];
        }
    };
    for (key, name, holes, b) in [
        ("side-l", "Left side", side_l, Box3 { pos: [0.0, side_y0, bt], size: [t, side_h, cd] }),
        ("side-r", "End side", side_e, Box3 { pos: [xb, side_y0, ze], size: [cd, side_h, t] }),
    ] {
        let idx = cx.part(key, name, Role::Carcass, &cm, cd, side_h, true, [false, false, false, true], b);
        let p = &mut cx.out.parts[idx];
        notch(p);
        p.holes = holes
            .into_iter()
            .map(|mut h| {
                h.x = r1(h.x);
                h.y = r1(h.y - side_y0);
                h
            })
            .collect();
        if key == "side-r" {
            p.mirror_x();
        }
    }
    let n = cx.out.parts.len();
    cx.out.parts[start..n].rotate_right(2);
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
    let toe = if cab.kind == CabinetKind::Wall { 0.0 } else { cab.toe_kick_height.max(0.0) };
    let tkd = cab.toe_kick_depth.clamp(0.0, d * 0.5);
    // On legs the sides start above the plinth; holes are shifted to match.
    let legs = s.plinth == Plinth::Legs && toe > 0.0;
    let side_y0 = if legs { toe } else { 0.0 };
    let side_h = h - side_y0;
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
        let rb =
            cx.part("rail-b", "Top rail back", Role::Carcass, &cm, iw, rw, false, [false; 4], Box3 { pos: [t, yt, z0], size: [iw, t, rw] });
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
    if toe > 0.0 && !(s.plinth_runs && cab.on_run_plinth()) {
        cx.part(
            "toe",
            if legs { "Plinth" } else { "Toe kick" },
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
        let groove = Pocket { x: r1(back_groove_y(cd)), y: 0.0, w: gw, h: side_h, depth: gd, r: 0.0, face: 1 };
        side_pockets.push(groove);
    } else {
        cx.part("back", "Back", Role::Back, &bm, w, h - toe, true, [false; 4], Box3 { pos: [0.0, toe, 0.0], size: [w, h - toe, bt] });
    }

    // ---------- fronts ----------
    let g = s.front_gap;
    let ft = fm.thickness;
    // Appliance housing (oven, microwave): an open slot [gap_lo, gap_hi]
    // between fixed shelves. Drawers and doors below it, doors above.
    let housing = if cab.housing_height > 0.0 && cab.kind != CabinetKind::Wall {
        let (lo, hi) = (cab.housing_elevation, cab.housing_elevation + cab.housing_height);
        if lo < toe + t - 0.5 || hi > h - t + 0.5 {
            cx.warn("appliance opening does not fit between bottom and top");
            None
        } else {
            Some((lo, hi))
        }
    } else {
        None
    };
    let ceil = housing.map_or(h - t, |(lo, _)| lo - t); // underside of what is above the drawers
    let mut cursor = housing.map_or(h, |(lo, _)| lo);
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
    let slide_len = SLIDE_LENGTHS.iter().rev().copied().find(|l| *l <= idp - 10.0);
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
                let box_hi = cursor.min(ceil) - 15.0;
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
                        // corner joints: face bores near each end, above the bottom groove
                        for x in [dt / 2.0, sl_len - dt / 2.0] {
                            for &y in &box_joint_y(bh) {
                                cx.out.parts[idx].holes.push(Hole {
                                    x: r1(x),
                                    y: r1(y),
                                    dia: side_dia,
                                    depth: side_depth.min(dt - 3.0),
                                    edge: None,
                                });
                            }
                        }
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
                        for &y in &box_joint_y(bh) {
                            for (e, x) in [(3u8, 0.0), (1u8, iw2)] {
                                let depth = edge_depth.min(iw2 / 2.0 - 10.0);
                                cx.out.parts[idx].holes.push(Hole { x: r1(x), y: r1(y), dia: edge_dia, depth, edge: Some(e) });
                            }
                        }
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

    // Door zones: below the drawers, and above an appliance housing.
    let lower_hi = cursor;
    let mut zones = vec![(toe, cursor, "door", "Door")];
    if let Some((_, hi)) = housing {
        zones.push((hi, h, "udoor", "Upper door"));
    }
    for (zi, (zone_lo, zone_hi, dkey, dname)) in zones.into_iter().enumerate() {
        let blind = blind && zi == 0;
        if cab.doors > 0 && zone_hi - zone_lo > 100.0 {
            let fh = zone_hi - zone_lo - g;
            let ow = ox1 - ox0;
            // Lift-up (wall cabinets): one full-width front on a stay-lift fitting.
            let lift = cab.lift_up && cab.kind == CabinetKind::Wall && !blind;
            let leaves: Vec<(f64, f64, HingeSide)> = if lift {
                vec![(ox0 + g / 2.0, ow - g, HingeSide::Left)]
            } else if blind {
                // One leaf, hinged on the cabinet side away from the blind panel.
                let side = if cab.blind_side == BlindSide::Left { HingeSide::Right } else { HingeSide::Left };
                vec![(ox0 + g / 2.0, ow - g, side)]
            } else if cab.doors >= 2 {
                let lw = ow / 2.0 - g;
                vec![(ox0 + g / 2.0, lw, HingeSide::Left), (ox0 + ow / 2.0 + g / 2.0, lw, HingeSide::Right)]
            } else {
                vec![(ox0 + g / 2.0, ow - g, cab.hinge_side)]
            };
            if cab.doors > 2 && !lift {
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
            let hy = hinges_on_rows(fh, zone_lo + g / 2.0, housing.map_or(toe + t, |(_, hi)| hi + t) + 64.0);
            for (n, (x0, fw, side)) in leaves.into_iter().enumerate() {
                let idx = cx.part(
                    &format!("{}{}", dkey, n + 1),
                    &format!("{} {}", dname, n + 1),
                    Role::Front,
                    &fm,
                    fw,
                    fh,
                    true,
                    [true; 4],
                    Box3 { pos: [x0, zone_lo + g / 2.0, d], size: [fw, fh, ft] },
                );
                if lift {
                    // handle across the bottom edge; the fitting mounts to its own template
                    if cab.handles {
                        let sp = s.handle_spacing;
                        let p = &mut cx.out.parts[idx];
                        for x in [fw / 2.0 - sp / 2.0, fw / 2.0 + sp / 2.0] {
                            p.holes.push(Hole { x: r1(x), y: 40.0, dia: 5.0, depth: 0.0, edge: None });
                        }
                        cx.hw(format!("Handle {}mm", sp), 1);
                    }
                    let duty = if fw * fh > 600.0 * 450.0 { "heavy" } else { "standard" };
                    cx.hw(format!("Lift-up stay fitting, {duty} (pair)"), 1);
                    continue;
                }
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
    }

    // ---------- wardrobe: fixed hat shelf + hanging rail ----------
    let mut in_hi = lower_hi.min(h - t);
    let mut in_lo = toe + t;
    if let Some((lo, hi)) = housing {
        // Fixed shelves above and below the opening (unless the top or bottom
        // is already there), joined like a bottom; adjustable shelves go above.
        let hd = idp;
        let at = [50.0, hd / 2.0, hd - 50.0];
        for (key, name, y) in [("hfix-l", "Housing shelf lower", lo - t), ("hfix-u", "Housing shelf upper", hi)] {
            if y < toe + t - 0.5 || y > h - 2.0 * t + 0.5 {
                continue;
            }
            let idx = cx.part(
                key,
                name,
                Role::Carcass,
                &cm,
                iw,
                hd,
                false,
                [true, false, false, false],
                Box3 { pos: [t, y, bz], size: [iw, t, hd] },
            );
            for &a in &at {
                both(&mut side_l, &mut side_r, Hole { x: a, y: y + t / 2.0, dia: side_dia, depth: side_depth, edge: None });
            }
            let p = &mut cx.out.parts[idx];
            for a in at {
                for (e, x) in [(3u8, 0.0), (1u8, p.w)] {
                    p.holes.push(Hole { x, y: r1(a), dia: edge_dia, depth: edge_depth, edge: Some(e) });
                }
            }
        }
        cx.out.placeholders.push(Placeholder {
            cabinet: cab.id.clone(),
            label: "Appliance".into(),
            box3d: Box3 { pos: [t + 2.0, lo, bz + 20.0], size: [iw - 4.0, hi - lo - 5.0, d - bz - 20.0] },
        });
        if cab.hanging_rail {
            cx.warn("no hanging rail in an appliance housing");
        }
        in_lo = hi + t;
        in_hi = h - t;
    }
    if cab.hanging_rail && cab.kind != CabinetKind::Wall && housing.is_none() {
        let yh = h - t - cab.hat_space.max(t + 50.0) - t; // hat shelf underside
        if yh > toe + t + 900.0 {
            let hd = idp - s.shelf_setback;
            let idx = cx.part(
                "hat",
                "Hat shelf",
                Role::Carcass,
                &cm,
                iw,
                hd,
                false,
                [true, false, false, false],
                Box3 { pos: [t, yh, bz], size: [iw, t, hd] },
            );
            // joined like a bottom: side bores + edge bores. The shelf is set
            // back, so side bores (measured from the carcass front) move with it.
            let at = [50.0, hd / 2.0, hd - 50.0];
            let setback = d - (bz + hd);
            for &y in &at {
                both(&mut side_l, &mut side_r, Hole { x: setback + y, y: yh + t / 2.0, dia: side_dia, depth: side_depth, edge: None });
            }
            let p = &mut cx.out.parts[idx];
            for y in at {
                for (e, x) in [(3u8, 0.0), (1u8, p.w)] {
                    p.holes.push(Hole { x, y: r1(y), dia: edge_dia, depth: edge_depth, edge: Some(e) });
                }
            }
            let rail_y = yh - 60.0;
            cx.out.placeholders.push(Placeholder {
                cabinet: cab.id.clone(),
                label: "Hanging rail".into(),
                box3d: Box3 { pos: [t, rail_y - 12.0, bz + hd / 2.0 - 12.0], size: [iw, 24.0, 24.0] },
            });
            cx.hw(format!("Wardrobe rail Ø25 × {:.0}", iw - 2.0), 1);
            cx.hw("Wardrobe rail support", 2);
            in_hi = yh;
        } else {
            cx.warn("too low for a hat shelf and hanging rail");
        }
    }

    // ---------- shelves ----------
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
            let ys = rows.iter().copied().min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs())).unwrap_or(target);
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
    for (key, name, holes, x) in [("side-l", "Left side", side_l, 0.0), ("side-r", "Right side", side_r, w - t)] {
        let idx = cx.part(
            key,
            name,
            Role::Carcass,
            &cm,
            cd,
            side_h,
            true,
            [false, false, false, true],
            Box3 { pos: [x, side_y0, z0], size: [t, side_h, cd] },
        );
        let p = &mut cx.out.parts[idx];
        p.pockets = side_pockets.clone();
        if toe > 0.0 && tkd > 0.0 && !legs {
            p.outline = vec![[0.0, r1(toe)], [r1(tkd), r1(toe)], [r1(tkd), 0.0], [r1(cd), 0.0], [r1(cd), r1(h)], [0.0, r1(h)]];
        }
        p.holes = holes
            .into_iter()
            .map(|mut h| {
                h.x = r1(h.x);
                h.y = r1(h.y - side_y0);
                h
            })
            .collect();
        if key == "side-r" {
            p.mirror_x();
        }
    }
    if legs {
        let n_legs = if w > 900.0 { 6 } else { 4 };
        cx.hw(format!("Adjustable leg {:.0} mm", toe), n_legs);
        cx.hw("Plinth clip", 2);
        let zf = d - tkd - t - 70.0;
        let mut xs = vec![40.0, w - 80.0];
        if n_legs == 6 {
            xs.push(w / 2.0 - 20.0);
        }
        for x in xs {
            for z in [z0 + 40.0, zf] {
                cx.out.placeholders.push(Placeholder {
                    cabinet: cab.id.clone(),
                    label: "Leg".into(),
                    box3d: Box3 { pos: [x, 0.0, z], size: [40.0, toe, 40.0] },
                });
            }
        }
    }
    // Sides first in the parts list.
    let n = cx.out.parts.len();
    cx.out.parts[start..n].rotate_right(2);
}
