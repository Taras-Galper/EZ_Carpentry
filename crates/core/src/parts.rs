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
}

#[derive(Debug, Clone, Serialize)]
pub struct Hole {
    pub x: f64,
    pub y: f64,
    pub dia: f64,
    /// 0 means through.
    pub depth: f64,
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
    pub box3d: Box3,
}

impl Part {
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

#[derive(Debug, Default, Clone, Serialize)]
pub struct Build {
    pub parts: Vec<Part>,
    pub hardware: Vec<Hardware>,
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
    out
}

fn build_cabinet(project: &Project, cab: &Cabinet, out: &mut Build) {
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
    let cd = d - bt; // carcass (side) depth
    let iw = w - 2.0 * t; // interior width
    let sl = s.system_line;

    // Holes on side panels, unmirrored frame: x from front, y from floor.
    let mut side_l: Vec<Hole> = vec![];
    let mut side_r: Vec<Hole> = vec![];
    let both = |l: &mut Vec<Hole>, r: &mut Vec<Hole>, hole: Hole| {
        l.push(hole.clone());
        r.push(hole);
    };

    // ---------- carcass ----------
    let yb = toe; // bottom panel underside
    let yt = h - t; // top panel underside
    cx.part(
        "bottom",
        "Bottom",
        Role::Carcass,
        &cm,
        iw,
        cd,
        false,
        [true, false, false, false],
        Box3 { pos: [t, yb, bt], size: [iw, t, cd] },
    );
    for x in [50.0, cd / 2.0, cd - 50.0] {
        both(&mut side_l, &mut side_r, Hole { x, y: yb + t / 2.0, dia: 7.0, depth: 0.0 });
    }
    if cab.kind == CabinetKind::Base {
        let rw = 100.0_f64.min(cd / 2.0);
        cx.part(
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
        cx.part(
            "rail-b",
            "Top rail back",
            Role::Carcass,
            &cm,
            iw,
            rw,
            false,
            [false; 4],
            Box3 { pos: [t, yt, bt], size: [iw, t, rw] },
        );
        for x in [rw / 2.0, cd - rw / 2.0] {
            both(&mut side_l, &mut side_r, Hole { x, y: yt + t / 2.0, dia: 7.0, depth: 0.0 });
        }
    } else {
        cx.part(
            "top",
            "Top",
            Role::Carcass,
            &cm,
            iw,
            cd,
            false,
            [true, false, false, false],
            Box3 { pos: [t, yt, bt], size: [iw, t, cd] },
        );
        for x in [50.0, cd / 2.0, cd - 50.0] {
            both(&mut side_l, &mut side_r, Hole { x, y: yt + t / 2.0, dia: 7.0, depth: 0.0 });
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

    // ---------- fronts ----------
    let g = s.front_gap;
    let ft = fm.thickness;
    let mut cursor = h;
    let slide_len = SLIDE_LENGTHS
        .iter()
        .rev()
        .copied()
        .find(|l| *l <= cd - 10.0);
    for (i, &dh) in cab.drawers.iter().enumerate() {
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
                p.holes.push(Hole { x: fw / 2.0 - sp / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0 });
                p.holes.push(Hole { x: fw / 2.0 + sp / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0 });
            } else {
                p.holes.push(Hole { x: fw / 2.0, y: fh / 2.0, dia: 5.0, depth: 0.0 });
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
                    for (k, xs) in [("l", x0), ("r", x0 + bw - dt)] {
                        cx.part(
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
                    }
                    let iw2 = bw - 2.0 * dt;
                    for (k, zs) in [("f", d - dt), ("b", z0)] {
                        cx.part(
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
                    }
                    cx.part(
                        &format!("d{}bot", i + 1),
                        &format!("Drawer {} bottom", i + 1),
                        Role::DrawerBox,
                        &bm,
                        iw2 + 16.0,
                        sl_len - 2.0 * dt + 16.0,
                        false,
                        [false; 4],
                        Box3 { pos: [x0 + dt - 8.0, box_lo + 10.0, z0 + dt - 8.0], size: [iw2 + 16.0, bt, sl_len - 2.0 * dt + 16.0] },
                    );
                    cx.hw(format!("Drawer slide {}mm (pair)", sl_len), 1);
                    let ys = box_lo + 20.0;
                    let mut xs = sl;
                    while xs < sl_len - 10.0 {
                        both(&mut side_l, &mut side_r, Hole { x: xs, y: ys, dia: 5.0, depth: 12.0 });
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
        let leaves: Vec<(f64, f64, HingeSide)> = if cab.doors >= 2 {
            let lw = w / 2.0 - g;
            vec![
                (g / 2.0, lw, HingeSide::Left),
                (w / 2.0 + g / 2.0, lw, HingeSide::Right),
            ]
        } else {
            vec![(g / 2.0, w - g, cab.hinge_side)]
        };
        if cab.doors > 2 {
            cx.warn("more than 2 doors not supported, using 2");
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
                    p.holes.push(Hole { x: cup_x, y, dia: 35.0, depth: 13.0 });
                }
                if cab.handles {
                    let sp = s.handle_spacing;
                    let yc = match cab.kind {
                        CabinetKind::Base => fh - 60.0 - sp / 2.0,
                        CabinetKind::Wall => 60.0 + sp / 2.0,
                        CabinetKind::Tall => (1000.0 - zone_lo).clamp(sp, fh - sp),
                    };
                    p.holes.push(Hole { x: handle_x, y: yc - sp / 2.0, dia: 5.0, depth: 0.0 });
                    p.holes.push(Hole { x: handle_x, y: yc + sp / 2.0, dia: 5.0, depth: 0.0 });
                }
            }
            if cab.handles {
                cx.hw(format!("Handle {}mm", s.handle_spacing), 1);
            }
            cx.hw("Clip hinge 110° + plate", hy.len() as u32);
            let target = if side == HingeSide::Left { &mut side_l } else { &mut side_r };
            for &y in &hy {
                let yw = zone_lo + g / 2.0 + y;
                target.push(Hole { x: sl, y: yw - 16.0, dia: 5.0, depth: 12.0 });
                target.push(Hole { x: sl, y: yw + 16.0, dia: 5.0, depth: 12.0 });
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
            for x in [sl, cd - sl] {
                both(&mut side_l, &mut side_r, Hole { x, y, dia: 5.0, depth: 12.0 });
            }
        }
        let n = cab.shelves;
        let sd = cd - s.shelf_setback;
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
                Box3 { pos: [t + 1.0, ys, bt], size: [sw, t, sd] },
            );
        }
        cx.hw("Shelf pin 5mm", 4 * n);
    }

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
            Box3 { pos: [x, 0.0, bt], size: [t, h, cd] },
        );
        let p = &mut cx.out.parts[idx];
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
