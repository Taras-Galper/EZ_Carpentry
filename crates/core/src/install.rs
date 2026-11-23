//! Fixing hardware for putting the job in: cabinets bolted to each other,
//! hung or fixed to the wall, worktops screwed down and joined at corners.

use crate::model::*;
use crate::parts::{Build, Hardware};

fn add(out: &mut Build, cabinet: &str, name: &str, qty: u32) {
    if qty == 0 {
        return;
    }
    match out.hardware.iter_mut().find(|h| h.cabinet == cabinet && h.name == name) {
        Some(h) => h.qty += qty,
        None => out.hardware.push(Hardware { cabinet: cabinet.into(), name: name.into(), qty }),
    }
}

/// `joints`: worktop corner joints, by the cabinet whose worktop was cut back.
pub fn plan(project: &Project, out: &mut Build, joints: &[String]) {
    let cabs: Vec<&Cabinet> = project.cabinets.iter().filter(|c| c.kind.is_carcass()).collect();
    for c in &cabs {
        match c.kind {
            CabinetKind::Wall => add(out, &c.id, "Wall cabinet hanger (pair)", 1),
            CabinetKind::Tall => add(out, &c.id, "Anti-tip wall bracket", 2),
            _ => add(out, &c.id, "Wall fixing bracket", 2),
        }
        if c.under_worktop() && project.settings.worktop_thickness > 0.0 {
            add(out, &c.id, "Worktop fixing screw", if c.is_l() { 6 } else { 4 });
        }
    }
    // Side by side, facing the same way, overlapping in height: bolt them together.
    for (i, a) in cabs.iter().enumerate() {
        let (sn, cs) = a.rotation.to_radians().sin_cos();
        for b in &cabs[i + 1..] {
            if ((a.rotation - b.rotation).rem_euclid(360.0)).min((b.rotation - a.rotation).rem_euclid(360.0)) > 0.5 {
                continue;
            }
            let (pa, pb) = (a.x * cs - a.z * sn, b.x * cs - b.z * sn);
            let (qa, qb) = (a.x * sn + a.z * cs, b.x * sn + b.z * cs);
            let touch = (pa + a.width - pb).abs() < 1.5 || (pb + b.width - pa).abs() < 1.5;
            let band = (a.elevation + a.height).min(b.elevation + b.height) - a.elevation.max(b.elevation);
            if (qa - qb).abs() < 1.0 && touch && band > 100.0 {
                let n = if band > 1200.0 {
                    6
                } else if band > 500.0 {
                    4
                } else {
                    2
                };
                // half on each side of the joint, so neither cabinet carries it all
                add(out, &a.id, "Cabinet connector screw", n / 2);
                add(out, &b.id, "Cabinet connector screw", n / 2);
            }
        }
    }
    for id in joints {
        add(out, id, "Worktop connector bolt", 3);
    }
    // Applied backs: screwed every 150 mm round the edge.
    if project.settings.back_mode == BackMode::Applied {
        let backs: Vec<(String, u32)> = out
            .parts
            .iter()
            .filter(|p| p.role == crate::parts::Role::Back && !p.cabinet.is_empty())
            .map(|p| (p.cabinet.clone(), ((2.0 * (p.w + p.h)) / 150.0).ceil() as u32))
            .collect();
        for (cab, n) in backs {
            add(out, &cab, "Back panel screw 3.5×16", n);
        }
    }
    // One fastener per joint bore pair (each edge bore has its face bore).
    let name = match project.settings.joinery {
        Joinery::Confirmat => "Confirmat screw 7×50",
        Joinery::Dowel => "Dowel 8×35",
    };
    let mut per: Vec<(String, u32)> = vec![];
    for p in &out.parts {
        let n = p.holes.iter().filter(|h| h.edge.is_some()).count() as u32;
        if n == 0 {
            continue;
        }
        match per.iter_mut().find(|x| x.0 == p.cabinet) {
            Some(x) => x.1 += n,
            None => per.push((p.cabinet.clone(), n)),
        }
    }
    for (cab, n) in per {
        add(out, &cab, name, n);
    }
}
