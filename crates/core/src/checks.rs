//! Layout checks: overlapping cabinets, cabinets through walls, doors
//! hinged hard against a wall. Results are warnings, never errors.

use crate::model::*;
use crate::parts::to_world;
use crate::room::Room;

/// Plan footprint as world quads, one per footprint rectangle
/// (corners in a consistent winding).
fn corners(c: &Cabinet) -> Vec<[(f64, f64); 4]> {
    let p = |x: f64, z: f64| to_world(c.x, c.z, c.rotation, x, z);
    c.rects().iter().map(|r| [p(r[0], r[1]), p(r[2], r[1]), p(r[2], r[3]), p(r[0], r[3])]).collect()
}

/// Overlap depth of two convex quads along their separating axes (SAT);
/// <= 0 when they do not overlap.
fn overlap(a: &[(f64, f64); 4], b: &[(f64, f64); 4]) -> f64 {
    let mut min = f64::MAX;
    for poly in [a, b] {
        for i in 0..4 {
            let (p, q) = (poly[i], poly[(i + 1) % 4]);
            let (nx, nz) = (q.1 - p.1, p.0 - q.0);
            let l = (nx * nx + nz * nz).sqrt();
            if l < 1e-9 {
                continue;
            }
            let (nx, nz) = (nx / l, nz / l);
            let proj =
                |v: &[(f64, f64); 4]| v.iter().map(|p| p.0 * nx + p.1 * nz).fold((f64::MAX, f64::MIN), |a, d| (a.0.min(d), a.1.max(d)));
            let (a0, a1) = proj(a);
            let (b0, b1) = proj(b);
            min = min.min(a1.min(b1) - a0.max(b0));
        }
    }
    min
}

fn inside(room: &Room, x: f64, z: f64) -> bool {
    let w = &room.walls;
    let mut c = false;
    let n = w.len();
    for i in 0..n {
        let (a, b) = (w[i], w[(i + n - 1) % n]);
        if (a[1] > z) != (b[1] > z) && x < (b[0] - a[0]) * (z - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

/// Distance from a point to a wall face (as a segment).
fn dist_to_wall(room: &Room, i: usize, x: f64, z: f64) -> f64 {
    let Some(w) = room.wall(i) else {
        return f64::MAX;
    };
    let (t, s) = w.local(x, z);
    let t = t.clamp(0.0, w.len);
    let (px, pz) = w.point(t, 0.0);
    let _ = s;
    ((x - px).powi(2) + (z - pz).powi(2)).sqrt()
}

pub fn check(project: &Project, warnings: &mut Vec<String>) {
    let cabs = &project.cabinets;
    let name = |c: &Cabinet| format!("{} ({})", c.name, c.id);
    // 1. Overlaps between cabinets at the same height (2 mm tolerance).
    for i in 0..cabs.len() {
        for j in i + 1..cabs.len() {
            let (a, b) = (&cabs[i], &cabs[j]);
            let band = (a.elevation + a.height).min(b.elevation + b.height) - a.elevation.max(b.elevation);
            if band <= 2.0 {
                continue;
            }
            let (ca, cb) = (corners(a), corners(b));
            let o = ca.iter().flat_map(|qa| cb.iter().map(move |qb| overlap(qa, qb))).fold(f64::MIN, f64::max);
            if o > 2.0 {
                warnings.push(format!("{} and {} overlap by {:.0} mm", name(a), name(b), o));
            }
        }
    }
    let room = &project.room;
    if room.walls.len() < 3 {
        return;
    }
    // 1b. Cabinets running into columns, boxing or radiators.
    for o in &room.obstacles {
        let Some(q) = o.corners(room) else { continue };
        for c in cabs {
            if c.elevation >= o.elevation + o.height - 2.0 || c.elevation + c.height <= o.elevation + 2.0 {
                continue;
            }
            let hit = corners(c).iter().map(|qa| overlap(qa, &q)).fold(f64::MIN, f64::max);
            if hit > 2.0 {
                let what = match o.kind {
                    crate::room::ObstacleKind::Column => "column",
                    crate::room::ObstacleKind::Boxing => "boxing",
                    crate::room::ObstacleKind::Radiator => "radiator",
                };
                warnings.push(format!("{} runs into the {what} on wall {} by {:.0} mm", name(c), o.wall + 1, hit));
            }
        }
    }
    // 2. Cabinets through walls: a footprint corner outside the room.
    for c in cabs {
        let out = corners(c)
            .iter()
            .flatten()
            .any(|&(x, z)| !inside(room, x, z) && (0..room.walls.len()).all(|k| dist_to_wall(room, k, x, z) > 2.0));
        if out {
            warnings.push(format!("{} goes through a wall", name(c)));
        }
    }
    // 3. Doors hinged against a wall at right angles: they stop at 90°,
    //    hit the wall's handle side and drawers will not clear. Needs a filler.
    for c in cabs.iter().filter(|c| c.kind.is_carcass() && c.doors > 0) {
        let sides: Vec<(HingeSide, f64)> = if c.is_l() {
            vec![(HingeSide::Left, 0.0)]
        } else if c.doors >= 2 {
            vec![(HingeSide::Left, 0.0), (HingeSide::Right, c.width)]
        } else {
            vec![(c.hinge_side, if c.hinge_side == HingeSide::Left { 0.0 } else { c.width })]
        };
        if c.blind_side != BlindSide::None {
            continue;
        }
        for (side, lx) in sides {
            let (fx, fz) = to_world(c.x, c.z, c.rotation, lx, c.depth);
            let (bx, bz) = to_world(c.x, c.z, c.rotation, lx, 0.0);
            for k in 0..room.walls.len() {
                let Some(w) = room.wall(k) else { continue };
                // wall must run along the cabinet's depth (perpendicular to its front)
                let (dx, dz) = (fx - bx, fz - bz);
                let l = (dx * dx + dz * dz).sqrt().max(1e-9);
                let parallel = ((dx / l) * w.dir[0] + (dz / l) * w.dir[1]).abs() > 0.99;
                if parallel && dist_to_wall(room, k, fx, fz) < 10.0 && dist_to_wall(room, k, bx, bz) < 10.0 {
                    warnings.push(format!(
                        "{} door hinged on the {} against a wall: add a filler of 50 mm or more",
                        name(c),
                        if side == HingeSide::Left { "left" } else { "right" }
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(id: &str, x: f64, z: f64, rot: f64) -> Cabinet {
        Cabinet { id: id.into(), x, z, rotation: rot, ..Cabinet::default() }
    }

    #[test]
    fn overlap_detected_and_touching_ok() {
        let mut p =
            Project { cabinets: vec![base("a", 0.0, 0.0, 0.0), base("b", 600.0, 0.0, 0.0)], room: Room::default(), ..sample_kitchen() };
        let mut w = vec![];
        check(&p, &mut w);
        assert!(w.is_empty(), "{w:?}");
        p.cabinets[1].x = 500.0;
        check(&p, &mut w);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("overlap by 100 mm"), "{w:?}");
        // different heights: a wall unit over a base unit is fine
        let mut w = vec![];
        p.cabinets[1].elevation = 1450.0;
        p.cabinets[1].kind = CabinetKind::Wall;
        p.cabinets[1].height = 720.0;
        check(&p, &mut w);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn rotated_overlap() {
        let p =
            Project { cabinets: vec![base("a", 0.0, 0.0, 0.0), base("b", 900.0, 300.0, -90.0)], room: Room::default(), ..sample_kitchen() };
        let mut w = vec![];
        check(&p, &mut w);
        // b's footprint x 340..900, z 300..900 hits a (x 0..600, z 0..560)
        assert!(w.iter().any(|m| m.contains("overlap")), "{w:?}");
    }

    #[test]
    fn sample_is_clean_and_wall_and_hinge_checks_fire() {
        let p = sample_kitchen();
        let mut w = vec![];
        check(&p, &mut w);
        assert!(w.is_empty(), "{w:?}");
        let mut p2 = p.clone();
        p2.cabinets.iter_mut().find(|c| c.id == "b2").unwrap().z = -100.0;
        let mut w = vec![];
        check(&p2, &mut w);
        assert!(w.iter().any(|m| m.contains("Sink base (b2) goes through a wall")), "{w:?}");
        // a single door hinged left, tight against the left wall
        let mut p3 = p.clone();
        p3.cabinets.retain(|c| c.id == "b4");
        p3.room = Room::rectangle(3000.0, 3000.0);
        let c = &mut p3.cabinets[0];
        c.x = 0.0;
        c.z = 0.0;
        c.rotation = 0.0;
        c.doors = 1;
        c.hinge_side = HingeSide::Left;
        let mut w = vec![];
        check(&p3, &mut w);
        assert!(w.iter().any(|m| m.contains("hinged on the left against a wall")), "{w:?}");
        p3.cabinets[0].hinge_side = HingeSide::Right;
        let mut w = vec![];
        check(&p3, &mut w);
        assert!(w.is_empty(), "{w:?}");
    }
}

/// Machining clashes inside each part: bores, pockets and edge bores that
/// run into each other (overlapping in plan and in depth). Bores at the same
/// centre are one bore; through bores may cross pockets.
pub fn machining(build: &crate::parts::Build, warnings: &mut Vec<String>) {
    // (kind, x0, y0, x1, y1, z0, z1, through) - z from face 1 down
    for p in &build.parts {
        let t = p.thickness;
        let mut items: Vec<(&str, [f64; 4], [f64; 2], bool, (f64, f64))> = vec![];
        for h in &p.holes {
            let r = h.dia / 2.0;
            match h.edge {
                None => {
                    let d = if h.depth <= 0.0 { t } else { h.depth.min(t) };
                    items.push(("bore", [h.x - r, h.y - r, h.x + r, h.y + r], [0.0, d], h.depth <= 0.0, (h.x, h.y)));
                }
                Some(e) => {
                    let b = match e {
                        0 => [h.x - r, 0.0, h.x + r, h.depth],
                        1 => [p.w - h.depth, h.y - r, p.w, h.y + r],
                        2 => [h.x - r, p.h - h.depth, h.x + r, p.h],
                        _ => [0.0, h.y - r, h.depth, h.y + r],
                    };
                    items.push(("edge bore", b, [t / 2.0 - r, t / 2.0 + r], false, (h.x, h.y)));
                }
            }
        }
        for k in &p.pockets {
            let z = if k.face == 2 { [t - k.depth, t] } else { [0.0, k.depth] };
            items.push(("groove", [k.x, k.y, k.x + k.w, k.y + k.h], z, false, (f64::NAN, f64::NAN)));
        }
        let mut found = 0;
        'outer: for i in 0..items.len() {
            for j in i + 1..items.len() {
                let (a, b) = (&items[i], &items[j]);
                if a.4 == b.4 && a.0 == b.0 {
                    continue; // the same bore listed twice
                }
                if (a.3 && b.0 == "groove") || (b.3 && a.0 == "groove") || (a.0 == "groove" && b.0 == "groove") {
                    continue; // through bores may cross grooves; grooves may run into each other
                }
                let ov = |lo0: f64, hi0: f64, lo1: f64, hi1: f64| hi0.min(hi1) - lo0.max(lo1) > 0.3;
                if ov(a.1[0], a.1[2], b.1[0], b.1[2]) && ov(a.1[1], a.1[3], b.1[1], b.1[3]) && ov(a.2[0], a.2[1], b.2[0], b.2[1]) {
                    // round bores: check the real distance between centres
                    if a.0 == "bore" && b.0 == "bore" {
                        let d = ((a.4 .0 - b.4 .0).powi(2) + (a.4 .1 - b.4 .1).powi(2)).sqrt();
                        if d >= (a.1[2] - a.1[0]) / 2.0 + (b.1[2] - b.1[0]) / 2.0 - 0.3 {
                            continue;
                        }
                    }
                    warnings.push(format!(
                        "{} {}: {} runs into {} near ({:.0}, {:.0})",
                        p.id,
                        p.name,
                        a.0,
                        b.0,
                        a.1[0].max(b.1[0]),
                        a.1[1].max(b.1[1])
                    ));
                    found += 1;
                    if found >= 3 {
                        break 'outer;
                    }
                }
            }
        }
    }
}
