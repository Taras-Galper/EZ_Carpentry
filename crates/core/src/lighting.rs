//! LED lighting: routed profile grooves and cable holes in the panels that
//! carry strips (under wall cabinets, under shelves), strips along run
//! plinths, and a plan that groups strips into circuits, each with its own
//! driver sized from the strip power plus headroom.

use crate::model::*;
use crate::parts::{Box3, Build, Hardware, Hole, Pocket};
use serde::{Deserialize, Serialize};

/// Shop settings for LED work.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Led {
    /// Strip voltage (12 or 24).
    pub voltage: f64,
    /// Strip power per metre.
    pub watts_per_m: f64,
    /// Aluminium profile width = routed groove width.
    pub profile_width: f64,
    /// Groove depth (capped 6 mm short of the panel thickness).
    pub groove_depth: f64,
    /// Groove distance from the panel's front edge.
    pub setback: f64,
    /// Cable hole diameter, drilled through near the back.
    pub cable_hole: f64,
    /// Driver headroom over the strip power, percent.
    pub headroom_pct: f64,
}

impl Default for Led {
    fn default() -> Self {
        Self {
            voltage: 24.0,
            watts_per_m: 9.6,
            profile_width: 17.0,
            groove_depth: 8.0,
            setback: 30.0,
            cable_hole: 10.0,
            headroom_pct: 20.0,
        }
    }
}

/// One LED strip, in its cabinet's local frame (for 3D).
#[derive(Debug, Clone, Serialize)]
pub struct Light {
    pub cabinet: String,
    /// "Under cabinet", "Shelf 1", "Plinth".
    pub place: String,
    /// Strip length, mm.
    pub length: f64,
    pub box3d: Box3,
    /// Circuit id (C1, C2, ...), set by `plan`.
    pub circuit: String,
}

/// Strips that share one driver and switch.
#[derive(Debug, Clone, Serialize)]
pub struct Circuit {
    pub id: String,
    /// Cabinet ids it serves.
    pub cabinets: Vec<String>,
    pub strips: usize,
    /// Total strip length, mm.
    pub length: f64,
    pub watts: f64,
    /// Driver rating chosen (W).
    pub driver: f64,
}

const DRIVERS: [f64; 6] = [20.0, 36.0, 60.0, 100.0, 150.0, 200.0];

/// Groove and cable hole on a horizontal panel (drawn with y = 0 at its
/// front): the groove on the underside (face 2), 20 mm in from each end.
/// Returns the strip length, or None when the panel is too small.
fn rout(s: &Led, p: &mut crate::parts::Part) -> Option<f64> {
    let len = p.w - 40.0;
    if len < 100.0 || p.h < s.setback + s.profile_width + 60.0 {
        return None;
    }
    let depth = s.groove_depth.clamp(1.0, p.thickness - 6.0);
    p.pockets.push(Pocket { x: 20.0, y: s.setback, w: (len * 10.0).round() / 10.0, h: s.profile_width, depth, r: 0.0, face: 2 });
    p.holes.push(Hole { x: 40.0, y: ((p.h - 40.0) * 10.0).round() / 10.0, dia: s.cable_hole, depth: 0.0, edge: None });
    Some(len)
}

/// Machine the panels of one cabinet for its LEDs and record the strips.
pub fn cabinet(project: &Project, cab: &Cabinet, out: &mut Build) {
    let s = &project.settings.led;
    if !(cab.led_under || cab.led_shelves) {
        return;
    }
    if cab.is_l() {
        out.warnings.push(format!("{} ({}): LEDs are not routed in corner cabinets", cab.name, cab.id));
        return;
    }
    let mut lights = vec![];
    let mut add = |out: &mut Build, key: &str, place: &str| {
        let id = format!("{}-{}", cab.id, key);
        let Some(p) = out.parts.iter_mut().find(|p| p.id == id) else { return };
        match rout(s, p) {
            Some(len) => {
                let [x, y, z] = p.box3d.pos;
                let [_, _, sz] = p.box3d.size;
                // strip on the underside, behind the setback from the panel's front
                let zf = z + sz - s.setback - s.profile_width;
                lights.push(Light {
                    cabinet: cab.id.clone(),
                    place: place.into(),
                    length: len,
                    box3d: Box3 { pos: [x + 20.0, y - 1.0, zf], size: [len, 2.0, s.profile_width] },
                    circuit: String::new(),
                });
            }
            None => out.warnings.push(format!("{} ({}): {} too small for an LED profile", cab.name, cab.id, place.to_lowercase())),
        }
    };
    if cab.led_under {
        if cab.kind == CabinetKind::Wall {
            add(out, "bottom", "Under cabinet");
        } else {
            out.warnings.push(format!("{} ({}): under-cabinet LEDs are for wall cabinets", cab.name, cab.id));
        }
    }
    if cab.led_shelves {
        for i in 1..=cab.shelves {
            add(out, &format!("shelf{i}"), &format!("Shelf {i}"));
        }
    }
    out.lights.extend(lights);
}

/// Strip along the foot of a run plinth (cabinet-local of the run's first).
pub fn plinth(first: &Cabinet, len: f64, zf: f64, out: &mut Build) {
    out.lights.push(Light {
        cabinet: first.id.clone(),
        place: "Plinth".into(),
        length: ((len - 40.0) * 10.0).round() / 10.0,
        box3d: Box3 { pos: [20.0, 4.0, zf + 1.0], size: [len - 40.0, 6.0, 2.0] },
        circuit: String::new(),
    });
}

/// Group strips into circuits (one per run of upper cabinets that lines up,
/// one per plinth run), size a driver for each, and list the hardware.
pub fn plan(project: &Project, out: &mut Build) {
    if out.lights.is_empty() {
        return;
    }
    let s = &project.settings.led;
    let cab = |id: &str| project.cabinets.iter().find(|c| c.id == id);
    // circuit key: plinth strips by their own run head; others by the line their cabinet sits on
    let r2 = |v: f64| (v * 2.0).round() / 2.0;
    let mut keys: Vec<(String, (f64, f64, f64), Vec<usize>)> = vec![];
    for (i, l) in out.lights.iter().enumerate() {
        let Some(c) = cab(&l.cabinet) else { continue };
        let (sn, cs) = c.rotation.to_radians().sin_cos();
        let k = if l.place == "Plinth" {
            (f64::NAN, f64::NAN, f64::NAN) // own circuit
        } else {
            (r2(c.rotation.rem_euclid(360.0)), r2(c.x * sn + c.z * cs), r2(c.elevation))
        };
        // cabinets on one line join the circuit only when the run is continuous
        let along = c.x * cs - c.z * sn;
        let found = keys.iter_mut().find(|(_, kk, members)| {
            !k.0.is_nan()
                && *kk == k
                && members.iter().any(|&j| {
                    let o = cab(&out.lights[j].cabinet).unwrap();
                    let oa = o.x * cs - o.z * sn;
                    o.id == c.id || (along - (oa + o.width)).abs() < 1.0 || (oa - (along + c.width)).abs() < 1.0
                })
        });
        match found {
            Some((_, _, m)) => m.push(i),
            None => keys.push((String::new(), k, vec![i])),
        }
    }
    let mut circuits = vec![];
    for (n, (_, _, members)) in keys.into_iter().enumerate() {
        let id = format!("C{}", n + 1);
        let length: f64 = members.iter().map(|&i| out.lights[i].length).sum();
        let watts = length / 1000.0 * s.watts_per_m;
        let need = watts * (1.0 + s.headroom_pct / 100.0);
        let driver = DRIVERS.iter().copied().find(|d| *d >= need).unwrap_or_else(|| (need / 50.0).ceil() * 50.0);
        let mut cabinets: Vec<String> = vec![];
        for &i in &members {
            out.lights[i].circuit = id.clone();
            if !cabinets.contains(&out.lights[i].cabinet) {
                cabinets.push(out.lights[i].cabinet.clone());
            }
        }
        circuits.push(Circuit {
            id,
            cabinets,
            strips: members.len(),
            length: (length * 10.0).round() / 10.0,
            watts: (watts * 10.0).round() / 10.0,
            driver,
        });
    }
    // hardware, on each circuit's first cabinet
    let mut hw = |cabinet: &str, name: String, qty: u32| match out.hardware.iter_mut().find(|h| h.cabinet == cabinet && h.name == name) {
        Some(h) => h.qty += qty,
        None => out.hardware.push(Hardware { cabinet: cabinet.into(), name, qty }),
    };
    let v = s.voltage;
    for c in &circuits {
        let head = &c.cabinets[0];
        hw(head, format!("LED strip {v:.0} V {:.1} W/m (m)", s.watts_per_m), (c.length / 1000.0).ceil() as u32);
        hw(head, format!("LED driver {v:.0} V {:.0} W", c.driver), 1);
        hw(head, "LED switch / sensor".into(), 1);
        hw(head, "LED connection cable".into(), c.strips as u32);
    }
    let lights = out.lights.clone();
    for l in lights.iter().filter(|l| l.place != "Plinth") {
        hw(&l.cabinet, format!("LED profile {:.0} mm × {:.0}", s.profile_width, l.length), 1);
    }
    out.circuits = circuits;
}
