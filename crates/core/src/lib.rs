//! Casework: parametric cabinet design core.
//!
//! `Project` (JSON) -> `Report` (parts, hardware, nested sheets) -> exports.

pub mod checks;
pub mod export;
pub mod import_dxf;
pub mod model;
pub mod nest;
pub mod parts;
pub mod room;

pub use model::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub cabinets: usize,
    pub parts: usize,
    pub sheets: Vec<(String, usize)>,
    pub edge_band_m: f64,
    pub holes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub build: parts::Build,
    pub nesting: nest::Nesting,
    pub summary: Summary,
}

pub fn compute(project: &Project) -> Report {
    let mut build = parts::build(project);
    checks::check(project, &mut build.warnings);
    let nesting = nest::nest(project, &build.parts);
    let mut sheets: Vec<(String, usize)> = vec![];
    for s in &nesting.sheets {
        let name = project.material(&s.material).name;
        match sheets.iter_mut().find(|x| x.0 == name) {
            Some(x) => x.1 += 1,
            None => sheets.push((name, 1)),
        }
    }
    let band: f64 = build.parts.iter().map(|p| p.band_length()).sum::<f64>() / 1000.0;
    let summary = Summary {
        cabinets: project.cabinets.len(),
        parts: build.parts.len(),
        sheets,
        edge_band_m: (band * 10.0).round() / 10.0,
        holes: build.parts.iter().map(|p| p.holes.len()).sum(),
    };
    Report { build, nesting, summary }
}

pub fn parse(json: &str) -> Result<Project, String> {
    let mut p: Project = serde_json::from_str(json).map_err(|e| e.to_string())?;
    p.migrate();
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_builds_and_nests() {
        let p = sample_kitchen();
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        assert!(r.build.parts.len() > 40);
        assert!(r.nesting.oversize.is_empty());
        // Every part placed exactly once.
        let placed: usize = r.nesting.sheets.iter().map(|s| s.placements.len()).sum();
        assert_eq!(placed, r.build.parts.len());
    }

    #[test]
    fn no_overlaps_and_inside_sheet() {
        let p = sample_kitchen();
        let r = compute(&p);
        let k = p.settings.kerf;
        for s in &r.nesting.sheets {
            for (i, a) in s.placements.iter().enumerate() {
                assert!(a.x >= s.trim - 1e-6 && a.y >= s.trim - 1e-6);
                assert!(a.x + a.w <= s.length - s.trim + 1e-6);
                assert!(a.y + a.h <= s.width - s.trim + 1e-6);
                for b in &s.placements[i + 1..] {
                    let sep = a.x + a.w + k <= b.x + 1e-6
                        || b.x + b.w + k <= a.x + 1e-6
                        || a.y + a.h + k <= b.y + 1e-6
                        || b.y + b.h + k <= a.y + 1e-6;
                    assert!(sep, "overlap on {}", s.material);
                }
            }
        }
    }

    #[test]
    fn holes_inside_parts() {
        let mut p = sample_kitchen();
        p.settings.back_mode = BackMode::Grooved;
        for c in p.cabinets.iter_mut().take(3) {
            c.door_style = DoorStyle::ShakerRouted;
        }
        for r in [compute(&sample_kitchen()), compute(&p)] {
            for q in &r.build.parts {
                for h in &q.holes {
                    match h.edge {
                        None => {
                            assert!(h.x - h.dia / 2.0 >= -1e-6 && h.x + h.dia / 2.0 <= q.w + 1e-6, "{} {:?}", q.id, h);
                            assert!(h.y - h.dia / 2.0 >= -1e-6 && h.y + h.dia / 2.0 <= q.h + 1e-6, "{} {:?}", q.id, h);
                        }
                        Some(e) => {
                            // on its edge, within the edge's length, not deeper than the part
                            let (on, along, len, across) = match e {
                                0 => (h.y == 0.0, h.x, q.w, q.h),
                                2 => (h.y == q.h, h.x, q.w, q.h),
                                1 => (h.x == q.w, h.y, q.h, q.w),
                                _ => (h.x == 0.0, h.y, q.h, q.w),
                            };
                            assert!(on, "{} edge bore off its edge {:?}", q.id, h);
                            assert!(along - h.dia / 2.0 >= 0.0 && along + h.dia / 2.0 <= len, "{} {:?}", q.id, h);
                            assert!(h.depth < across, "{} {:?}", q.id, h);
                            assert!(h.dia < q.thickness, "{} {:?}", q.id, h);
                        }
                    }
                }
                for k in &q.pockets {
                    assert!(k.x >= -1e-6 && k.y >= -1e-6 && k.x + k.w <= q.w + 1e-6 && k.y + k.h <= q.h + 1e-6, "{} {:?}", q.id, k);
                    assert!(k.depth < q.thickness, "{} {:?}", q.id, k);
                }
            }
        }
    }

    /// Every side bore of a joint lines up with an edge bore in the panel it holds.
    #[test]
    fn joint_bores_line_up() {
        for (joinery, plinth) in [(Joinery::Confirmat, Plinth::Board), (Joinery::Dowel, Plinth::Legs)] {
            for back_mode in [BackMode::Applied, BackMode::Grooved] {
                let mut p = sample_kitchen();
                p.settings.joinery = joinery;
                p.settings.back_mode = back_mode;
                p.settings.plinth = plinth;
                // a wardrobe adds a hat shelf joint
                p.cabinets.iter_mut().find(|c| c.id == "t1").unwrap().hanging_rail = true;
                let r = compute(&p);
                for c in p.cabinets.iter().filter(|c| c.kind.is_carcass()) {
                    let side = r.build.parts.iter().find(|q| q.id == format!("{}-side-l", c.id)).unwrap();
                    let joint_dia = if joinery == Joinery::Confirmat { 7.0 } else { 8.0 };
                    // side drawing: x from the front -> cabinet z = depth - x
                    let mut from_side: Vec<(i64, i64)> = side
                        .holes
                        .iter()
                        .filter(|h| h.dia == joint_dia)
                        .map(|h| ((c.depth - h.x).round() as i64, (h.y + side.box3d.pos[1]).round() as i64))
                        .collect();
                    // panel left-edge bores: drawing y from the front -> z = depth - y; height = panel mid
                    let mut from_panels: Vec<(i64, i64)> = r
                        .build
                        .parts
                        .iter()
                        .filter(|q| q.cabinet == c.id)
                        .flat_map(|q| {
                            let front_z = q.box3d.pos[2] + q.box3d.size[2];
                            let mid_y = q.box3d.pos[1] + q.box3d.size[1] / 2.0;
                            q.holes
                                .iter()
                                .filter(|h| h.edge == Some(3))
                                .map(move |h| ((front_z - h.y).round() as i64, mid_y.round() as i64))
                        })
                        .collect();
                    from_side.sort();
                    from_panels.sort();
                    assert!(!from_side.is_empty());
                    assert_eq!(from_side, from_panels, "{} {:?} {:?}", c.id, joinery, back_mode);
                }
            }
        }
    }

    #[test]
    fn grooved_back_fits_its_grooves() {
        let mut p = sample_kitchen();
        p.settings.back_mode = BackMode::Grooved;
        let s = p.settings.clone();
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let c = p.cabinets.iter().find(|c| c.id == "w1").unwrap();
        let get = |k: &str| r.build.parts.iter().find(|q| q.id == format!("w1-{k}")).unwrap();
        let (side, back, top) = (get("side-l"), get("back"), get("top"));
        let t = 18.0;
        // back is as wide as the interior plus both groove depths, less 1 mm play
        assert_eq!(back.w, c.width - 2.0 * t + 2.0 * s.groove_depth - 1.0);
        // sides are full depth and grooved for the back
        assert_eq!(side.w, c.depth);
        let g = &side.pockets[0];
        assert_eq!(g.w, 6.0 + 0.5);
        assert_eq!(g.x, c.depth - s.back_inset - 6.0 - 0.5);
        assert_eq!(top.pockets.len(), 1);
        // back panel box sits inside the groove band
        assert_eq!(back.box3d.pos[2], s.back_inset);
    }

    #[test]
    fn drawer_bottom_fits_grooves() {
        let r = compute(&sample_kitchen());
        let side = r.build.parts.iter().find(|q| q.id == "b5-d1sidel").unwrap();
        let front = r.build.parts.iter().find(|q| q.id == "b5-d1f").unwrap();
        let bottom = r.build.parts.iter().find(|q| q.id == "b5-d1bot").unwrap();
        let gd = side.pockets[0].depth;
        assert_eq!(bottom.w, front.w + 2.0 * gd - 1.0);
        assert_eq!(bottom.h, side.w - 2.0 * 15.0 + 2.0 * gd - 1.0);
        assert!(side.pockets[0].h > bottom.thickness);
    }

    #[test]
    fn routed_shaker_is_one_piece_with_face2_recess() {
        let mut p = sample_kitchen();
        p.cabinets.retain(|c| c.id == "w1");
        p.cabinets[0].door_style = DoorStyle::ShakerRouted;
        let r = compute(&p);
        let door = r.build.parts.iter().find(|q| q.id == "w1-door1").unwrap();
        assert_eq!(door.pockets.len(), 1);
        assert_eq!(door.pockets[0].face, 2);
        assert!(door.holes.iter().any(|h| h.dia == 35.0), "hinge cups stay on the inside face");
        assert!(r.build.parts.iter().all(|q| !q.id.contains("door1-")), "no stiles/rails");
    }

    #[test]
    fn grain_respected() {
        let p = sample_kitchen();
        let r = compute(&p);
        for s in &r.nesting.sheets {
            if !p.material(&s.material).grain {
                continue;
            }
            for pl in &s.placements {
                assert_eq!(pl.rotated, r.build.parts[pl.part].grain_y);
            }
        }
    }

    #[test]
    fn l_kitchen_worktops_meet_without_overlap() {
        let p = sample_kitchen();
        let r = compute(&p);
        let tops: Vec<_> = r.build.parts.iter().filter(|p| p.role == parts::Role::Worktop).collect();
        assert_eq!(tops.len(), 2, "back run + return leg");
        // back run b1+b2+b3, return leg b4+b5
        let mut lens: Vec<f64> = tops.iter().map(|t| t.w).collect();
        lens.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(lens, vec![450.0 + 600.0 + 450.0, 600.0 + 800.0 + 1150.0]);
        // world footprints touch but do not overlap
        let rect = |t: &parts::Part| {
            let c = p.cabinets.iter().find(|c| c.id == t.cabinet).unwrap();
            let [x0, _, z0] = t.box3d.pos;
            let [sx, _, sz] = t.box3d.size;
            let pts: Vec<_> = [(x0, z0), (x0 + sx, z0), (x0, z0 + sz), (x0 + sx, z0 + sz)]
                .iter()
                .map(|&(x, z)| parts::to_world(c.x, c.z, c.rotation, x, z))
                .collect();
            let f = |g: fn(&(f64, f64)) -> f64| {
                let v: Vec<f64> = pts.iter().map(g).collect();
                (v.iter().cloned().fold(f64::MAX, f64::min), v.iter().cloned().fold(f64::MIN, f64::max))
            };
            (f(|p| p.0), f(|p| p.1))
        };
        let (a, b) = (rect(tops[0]), rect(tops[1]));
        let ox = a.0 .1.min(b.0 .1) - a.0 .0.max(b.0 .0);
        let oz = a.1 .1.min(b.1 .1) - a.1 .0.max(b.1 .0);
        assert!(ox <= 0.5 || oz <= 0.5, "overlap {ox} x {oz}");
    }

    #[test]
    fn overlapping_corner_worktop_is_trimmed() {
        let mut p = sample_kitchen();
        // push the return leg back into the corner so the worktops overlap
        for c in p.cabinets.iter_mut().filter(|c| ["b4", "b5", "b6"].contains(&c.id.as_str())) {
            c.z -= 580.0;
        }
        let r = compute(&p);
        let leg = r.build.parts.iter().find(|q| q.id == "b4-worktop").unwrap();
        assert_eq!(leg.box3d.pos[0], 580.0);
        assert_eq!(leg.w, 450.0 + 600.0 + 450.0 - 580.0);
    }

    #[test]
    fn worktop_cutouts_sit_over_their_cabinets() {
        let p = sample_kitchen();
        let r = compute(&p);
        let back = r.build.parts.iter().find(|q| q.id == "d1-worktop").unwrap();
        let leg = r.build.parts.iter().find(|q| q.id == "b4-worktop").unwrap();
        assert_eq!(back.cutouts.len(), 1);
        assert_eq!(leg.cutouts.len(), 1);
        // sink centred on b2 (600 mm into the back run, 800 wide)
        let sink = &back.cutouts[0];
        assert_eq!(sink.kind, CutoutKind::Sink);
        assert_eq!(sink.x + sink.w / 2.0, 600.0 + 400.0);
        assert_eq!(sink.y, 60.0);
        // hob centred on b5 (450 mm into the leg, 600 wide)
        let hob = &leg.cutouts[0];
        assert_eq!(hob.x + hob.w / 2.0, 450.0 + 300.0);
        for t in [back, leg] {
            for c in &t.cutouts {
                assert!(c.x > 0.0 && c.x + c.w < t.w && c.y > 0.0 && c.y + c.h < t.h);
            }
        }
    }

    #[test]
    fn cutout_contour_is_closed_rounded_rect() {
        let c = parts::Cutout { kind: CutoutKind::Sink, x: 0.0, y: 0.0, w: 100.0, h: 50.0, r: 10.0 };
        let v = c.contour();
        assert_eq!(v.len(), 8);
        assert_eq!(v.iter().filter(|p| p.2 > 0.0).count(), 4);
        let pts = c.points(4);
        assert!(pts.iter().all(|p| p.0 >= -1e-9 && p.0 <= 100.0 + 1e-9 && p.1 >= -1e-9 && p.1 <= 50.0 + 1e-9));
    }

    #[test]
    fn quote_adds_up() {
        let p = sample_kitchen();
        let r = compute(&p);
        let q = export::quote::quote(&p, &r);
        let line_sum: f64 = q.lines.iter().map(|l| l.total).sum();
        assert!((line_sum - q.cost).abs() < 0.05 * q.lines.len() as f64, "{line_sum} vs {}", q.cost);
        // cabinets + worktops share out the whole cost
        let item_cost: f64 = q.items.iter().map(|c| c.cost).sum();
        assert!((item_cost - q.cost).abs() < 0.01 * q.items.len() as f64 + 0.5, "{item_cost} vs {}", q.cost);
        let k = 1.0 + p.pricing.markup_pct / 100.0;
        assert!((q.net - q.cost * k).abs() < 0.01 * q.items.len() as f64 + 0.5);
        assert_eq!(q.items.iter().filter(|i| i.name == "Worktop").count(), 2);
        // b4 and b6 are the same cabinet (b6 mirrored); with b6's end panel
        // off they must cost the same, although only b4 heads a worktop run
        let mut p2 = p.clone();
        p2.cabinets.iter_mut().find(|c| c.id == "b6").unwrap().end_right = false;
        let q2 = export::quote::quote(&p2, &compute(&p2));
        let price = |id: &str| q2.items.iter().find(|i| i.id == id).unwrap().price;
        assert!((price("b4") - price("b6")).abs() < 1.0, "{} vs {}", price("b4"), price("b6"));
        assert!((q.total - q.net * (1.0 + p.pricing.vat_pct / 100.0)).abs() < 0.02);
        // sheets priced whole
        let sheets: f64 = q.lines.iter().filter(|l| l.group == "Material").map(|l| l.qty).sum();
        assert_eq!(sheets as usize, r.nesting.sheets.len());
    }

    #[test]
    fn quote_descriptions() {
        let p = sample_kitchen();
        let r = compute(&p);
        let d = |id: &str| export::quote::describe(&p, &r, id);
        assert!(d("b5").contains("3 drawers"), "{}", d("b5"));
        assert!(d("d1").starts_with("Integrated dishwasher panel 600"), "{}", d("d1"));
        assert!(d("b2").contains("2 doors"), "{}", d("b2"));
        assert!(d("t1").starts_with("Tall cabinet 600×2200×580") && d("t1").contains("2 doors"), "{}", d("t1"));
        assert!(d("b3").starts_with("Blind corner") && d("b3").contains("1 door,"), "{}", d("b3"));
    }

    #[test]
    fn hardware_price_override() {
        let mut p = sample_kitchen();
        let before = export::quote::quote(&p, &compute(&p)).hardware;
        p.pricing.hardware.insert("Shelf pin 5mm".into(), 10.5);
        let r = compute(&p);
        let pins: u32 = r.build.hardware.iter().filter(|h| h.name == "Shelf pin 5mm").map(|h| h.qty).sum();
        let after = export::quote::quote(&p, &r).hardware;
        assert!((after - before - pins as f64 * 10.0).abs() < 0.01);
    }

    #[test]
    fn dishwasher_filler_appliance_end_panel() {
        let mut p = sample_kitchen();
        // dishwasher: one decor panel (Shaker = 5 pieces), toe kick, body placeholder, joins the worktop
        let r = compute(&p);
        let d1: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "d1").collect();
        assert_eq!(d1.iter().filter(|q| q.role == parts::Role::Front).count(), 5);
        assert!(d1.iter().any(|q| q.name == "Toe kick"));
        assert!(r.build.placeholders.iter().any(|x| x.cabinet == "d1"));
        let back = r.build.parts.iter().find(|q| q.id == "d1-worktop").unwrap();
        assert_eq!(back.w, 600.0 + 800.0 + 1150.0);
        // end panel on b6: depth covers the door, full height
        let e = r.build.parts.iter().find(|q| q.id == "b6-end-r").unwrap();
        assert_eq!((e.w, e.h), (560.0 + 19.0, 870.0));
        // filler and a fridge space added to the back run
        // widen the room so both fit beyond the corner cabinet
        p.room = crate::room::Room::rectangle(4000.0, 2600.0);
        p.cabinets.push(Cabinet { id: "f9".into(), kind: CabinetKind::Filler, width: 50.0, x: 3200.0, ..Cabinet::default() });
        p.cabinets.push(Cabinet {
            id: "a1".into(),
            name: "Fridge".into(),
            kind: CabinetKind::Appliance,
            width: 600.0,
            height: 2000.0,
            depth: 650.0,
            x: 3250.0,
            ..Cabinet::default()
        });
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let f1: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "f9").collect();
        assert_eq!(f1.len(), 2, "strip + toe kick");
        assert!(r.build.parts.iter().all(|q| q.cabinet != "a1"), "appliance space cuts nothing");
        // filler (under-worktop height) extends the back worktop; the tall fridge space does not
        let back = r.build.parts.iter().find(|q| q.id == "d1-worktop").unwrap();
        assert_eq!(back.w, 600.0 + 800.0 + 1150.0 + 50.0);
        let q = export::quote::quote(&p, &r);
        assert!(q.items.iter().all(|i| i.id != "a1"), "nothing to charge for an empty space");
        let files = export::files(&p, &r);
        assert!(files.iter().all(|f| !f.0.starts_with("drawings/f9") && !f.0.starts_with("drawings/a1")));
    }

    #[test]
    fn old_rectangular_room_migrates() {
        let mut v: serde_json::Value = serde_json::to_value(sample_kitchen()).unwrap();
        v.as_object_mut().unwrap().remove("room");
        v["settings"]["room_width"] = 3200.0.into();
        v["settings"]["room_depth"] = 2600.0.into();
        let p = parse(&v.to_string()).unwrap();
        assert_eq!(p.room.walls, vec![[0.0, 0.0], [3200.0, 0.0], [3200.0, 2600.0], [0.0, 2600.0]]);
        assert_eq!(p.settings.room_width, 0.0);
    }

    /// Parse a program into (tool, x, y, z) positions after every move.
    fn trace(g: &str) -> Vec<(u32, f64, f64, f64)> {
        let (mut t, mut x, mut y, mut z) = (0u32, 0.0, 0.0, 50.0);
        let mut out = vec![];
        for line in g.lines() {
            if line.starts_with('(') || line.starts_with('%') {
                continue;
            }
            for w in line.split_whitespace() {
                let (k, v) = w.split_at(1);
                let v: f64 = v.parse().unwrap_or(0.0);
                match k {
                    "T" => t = v as u32,
                    "X" => x = v,
                    "Y" => y = v,
                    "Z" => z = v,
                    _ => {}
                }
            }
            if line.starts_with("G0") || line.starts_with("G1") || line.starts_with("G3") {
                out.push((t, x, y, z));
            }
        }
        out
    }

    #[test]
    fn gcode_stays_on_sheet_and_cuts_every_part() {
        let mut p = sample_kitchen();
        p.settings.back_mode = BackMode::Grooved;
        p.cabinets.iter_mut().find(|c| c.id == "t1").unwrap().door_style = DoorStyle::ShakerRouted;
        let r = compute(&p);
        let cnc = &p.cnc;
        let rad = cnc.router.dia / 2.0;
        for (i, s) in r.nesting.sheets.iter().enumerate() {
            let g = export::gcode::sheet_program(&p, s, &r.build.parts, i);
            let th = p.material(&s.material).thickness;
            let moves = trace(&g);
            for &(_, x, y, z) in &moves {
                assert!(x >= -rad - 1.0 && x <= s.length + rad + 1.0, "x {x} off sheet {}", s.material);
                assert!(y >= -rad - 1.0 && y <= s.width + rad + 1.0, "y {y} off sheet");
                assert!(z >= -(th + cnc.through) - 1e-6, "z {z} too deep on {}", s.material);
            }
            let profiles = g.lines().filter(|l| l.starts_with("(Profile ")).count();
            assert_eq!(profiles, s.placements.len());
            assert!(g.trim_end().ends_with('%') && g.contains("M30"));
        }
    }

    #[test]
    fn gcode_profile_is_offset_by_tool_radius() {
        let p = sample_kitchen();
        let r = compute(&p);
        let s = &r.nesting.sheets[0];
        let pl = &s.placements[0];
        let part = &r.build.parts[pl.part];
        let pts: Vec<(f64, f64)> = part.outline.iter().map(|q| pl.map(part.w, q[0], q[1])).collect();
        let path = export::gcode::offset(&pts, 4.0);
        let bb = |v: &[(f64, f64)]| {
            v.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |a, p| (a.0.min(p.0), a.1.min(p.1), a.2.max(p.0), a.3.max(p.1)))
        };
        let (a, b) = (bb(&pts), bb(&path));
        for (u, v) in [(a.0 - 4.0, b.0), (a.1 - 4.0, b.1), (a.2 + 4.0, b.2), (a.3 + 4.0, b.3)] {
            assert!((u - v).abs() < 1e-6, "{u} vs {v}");
        }
    }

    /// No part's router path cuts into a neighbouring part.
    #[test]
    fn profiles_never_cut_into_neighbours() {
        let p = sample_kitchen();
        let r = compute(&p);
        let rad = p.cnc.router.dia / 2.0;
        for s in &r.nesting.sheets {
            for (i, a) in s.placements.iter().enumerate() {
                // tool-path envelope of part a
                let (ax0, ay0, ax1, ay1) = (a.x - rad, a.y - rad, a.x + a.w + rad, a.y + a.h + rad);
                for (j, b) in s.placements.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let ox = ax1.min(b.x + b.w) - ax0.max(b.x);
                    let oy = ay1.min(b.y + b.h) - ay0.max(b.y);
                    assert!(ox <= 1e-6 || oy <= 1e-6, "path of {} enters {}", r.build.parts[a.part].id, r.build.parts[b.part].id);
                }
            }
        }
    }

    /// A back-panel groove that runs out through a panel's ends is cut right to both ends.
    #[test]
    fn open_grooves_reach_the_edge() {
        let mut p = sample_kitchen();
        p.settings.back_mode = BackMode::Grooved;
        p.cabinets.retain(|c| c.id == "w1");
        let r = compute(&p);
        let bottom_i = r.build.parts.iter().position(|q| q.id == "w1-bottom").unwrap();
        let (si, s) = r.nesting.sheets.iter().enumerate().find(|(_, s)| s.placements.iter().any(|pl| pl.part == bottom_i)).unwrap();
        let pl = s.placements.iter().find(|pl| pl.part == bottom_i).unwrap();
        let g = export::gcode::sheet_program(&p, s, &r.build.parts, si);
        // groove router path along the groove must span the part's full footprint
        let moves = trace(&g);
        let gr = p.cnc.groove_router.number;
        let (x0, y0, x1, y1) = (pl.x, pl.y, pl.x + pl.w, pl.y + pl.h);
        let on_part: Vec<_> = moves
            .iter()
            .filter(|m| m.0 == gr && m.3 < 0.0 && m.1 >= x0 - 5.0 && m.1 <= x1 + 5.0 && m.2 >= y0 - 5.0 && m.2 <= y1 + 5.0)
            .collect();
        assert!(!on_part.is_empty());
        let (lo, hi) = if pl.w > pl.h { (x0, x1) } else { (y0, y1) };
        let pos = |m: &&(u32, f64, f64, f64)| if pl.w > pl.h { m.1 } else { m.2 };
        let min = on_part.iter().map(pos).fold(f64::MAX, f64::min);
        let max = on_part.iter().map(pos).fold(f64::MIN, f64::max);
        assert!(min <= lo + 1e-6 && max >= hi - 1e-6, "groove {min}..{max} vs part {lo}..{hi}");
    }

    #[test]
    fn small_parts_get_tabs_and_face2_programs_exist() {
        let mut p = sample_kitchen();
        p.cabinets.iter_mut().find(|c| c.id == "t1").unwrap().door_style = DoorStyle::ShakerRouted;
        let r = compute(&p);
        let files = export::files(&p, &r);
        assert!(files.iter().any(|f| f.0.starts_with("nc/face2/t1-door1")));
        // toe kicks are 100 mm wide: tabs leave the cutter above the bottom
        let (si, s) = r
            .nesting
            .sheets
            .iter()
            .enumerate()
            .find(|(_, s)| s.placements.iter().any(|pl| r.build.parts[pl.part].name == "Toe kick"))
            .unwrap();
        let g = export::gcode::sheet_program(&p, s, &r.build.parts, si);
        let th = p.material(&s.material).thickness;
        let tab_z = -(th + p.cnc.through) + p.cnc.through + p.cnc.tab_height;
        assert!(g.contains(&format!("Z{}", (tab_z * 1000.0).round() / 1000.0)), "no tab lifts");
        assert!(g.contains("NOT IN THIS PROGRAM:") && g.contains("edge bores"));
    }

    #[test]
    fn packet_and_labels_cover_every_part() {
        let p = sample_kitchen();
        let r = compute(&p);
        let packet = export::packet::packet_html(&p, &r, Some("2026-11-22"));
        let labels = export::packet::labels_html(&p, &r);
        for q in &r.build.parts {
            assert!(packet.contains(&format!("<b>{}</b>", q.id)), "{} missing from cut list", q.id);
        }
        assert_eq!(labels.matches("<div class=label>").count(), r.build.parts.len());
        assert_eq!(labels.matches("<div class=page>").count(), r.build.parts.len().div_ceil(24));
        assert_eq!(packet.matches("<section class=land>").count(), r.nesting.sheets.len());
        assert!(packet.contains("2026-11-22"));
        // every edge-bored part is in the edge boring list
        let edge = &packet[packet.find("<h2>Edge boring</h2>").expect("edge boring section")..];
        for q in r.build.parts.iter().filter(|q| q.holes.iter().any(|h| h.edge.is_some())) {
            assert!(edge.contains(&format!("<b>{}</b>", q.id)), "{} missing from edge boring", q.id);
        }
    }

    #[test]
    fn legs_wardrobe_and_finished_back() {
        let mut p = sample_kitchen();
        p.settings.plinth = Plinth::Legs;
        p.cabinets.retain(|c| c.id == "t1" || c.id == "b4");
        p.cabinets[0].hanging_rail = true;
        p.cabinets[0].shelves = 0;
        p.cabinets[1].back_finished = true;
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id).unwrap();
        // on legs: plain rectangular sides that stop at the plinth
        let side = get("b4-side-l");
        assert_eq!(side.outline.len(), 4);
        assert_eq!(side.h, 870.0 - 100.0);
        assert_eq!(get("b4-toe").name, "Plinth");
        assert!(r.build.hardware.iter().any(|h| h.cabinet == "b4" && h.name.starts_with("Adjustable leg") && h.qty == 4));
        // wardrobe: hat shelf 300 below the top, rail hardware
        let hat = get("t1-hat");
        assert_eq!(hat.box3d.pos[1] + 18.0, 2200.0 - 18.0 - 300.0);
        assert!(r.build.hardware.iter().any(|h| h.cabinet == "t1" && h.name.starts_with("Wardrobe rail Ø25")));
        // finished back covers the whole back
        let fb = get("b4-back-fin");
        assert_eq!((fb.w, fb.h), (450.0, 870.0));
    }

    #[test]
    fn blind_corner_front() {
        let r = compute(&sample_kitchen());
        let b3: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "b3" && q.role == parts::Role::Front).collect();
        // shaker door (5 pieces) + shaker blind panel (5 pieces)
        assert_eq!(b3.iter().filter(|q| q.id.contains("door1")).count(), 5);
        assert_eq!(b3.iter().filter(|q| q.id.contains("blind")).count(), 5);
        let door_w: f64 = b3.iter().filter(|q| q.id.contains("door1")).map(|q| q.box3d.pos[0] + q.box3d.size[0]).fold(0.0, f64::max);
        assert!(door_w <= 1200.0 - 600.0, "door stays out of the blind part");
    }

    #[test]
    fn shaker_door_has_five_pieces_and_keeps_holes() {
        let mut p = sample_kitchen();
        p.cabinets.retain(|c| c.id == "w1");
        let slab = {
            let mut q = p.clone();
            q.cabinets[0].door_style = DoorStyle::Slab;
            compute(&q)
        };
        let shaker = compute(&p);
        let door_holes =
            |r: &Report| -> usize { r.build.parts.iter().filter(|x| x.role == parts::Role::Front).map(|x| x.holes.len()).sum() };
        let pieces = shaker.build.parts.iter().filter(|x| x.id.starts_with("w1-door1-")).count();
        assert_eq!(pieces, 5);
        assert_eq!(door_holes(&slab), door_holes(&shaker));
    }

    #[test]
    fn drawings_render() {
        let p = sample_kitchen();
        let r = compute(&p);
        for c in &p.cabinets {
            let s = export::drawing::cabinet_svg(&p, &r.build, c);
            assert!(s.starts_with("<svg") && s.ends_with("</svg>"));
        }
    }

    #[test]
    fn json_roundtrip_and_zip() {
        let p = sample_kitchen();
        let j = serde_json::to_string(&p).unwrap();
        let p2 = parse(&j).unwrap();
        let z = export::bundle(&p2, &compute(&p2));
        assert_eq!(&z[0..4], b"PK\x03\x04");
    }

    /// Back-right corner of the sample room: base run, L corner, return run,
    /// and a wall L corner above.
    fn l_kitchen() -> Project {
        let mut p = sample_kitchen();
        let base = Cabinet::default();
        p.cabinets = vec![
            Cabinet { id: "a1".into(), x: 1600.0, ..base.clone() },
            Cabinet {
                id: "lc".into(),
                name: "L corner".into(),
                x: 2200.0,
                width: 1000.0,
                width2: 1000.0,
                corner: Corner::L,
                ..base.clone()
            },
            Cabinet { id: "b1".into(), x: 3200.0, z: 1000.0, rotation: -90.0, ..base.clone() },
            Cabinet {
                id: "wl".into(),
                name: "Wall L".into(),
                kind: CabinetKind::Wall,
                x: 2600.0,
                elevation: 1450.0,
                width: 600.0,
                width2: 600.0,
                height: 720.0,
                depth: 320.0,
                corner: Corner::L,
                ..base
            },
        ];
        p
    }

    fn inside_outline(o: &[[f64; 2]], x: f64, y: f64) -> bool {
        let mut c = false;
        for i in 0..o.len() {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                c = !c;
            }
        }
        c
    }

    #[test]
    fn l_corner_builds_cleanly() {
        for style in [DoorStyle::Slab, DoorStyle::Shaker] {
            for plinth in [Plinth::Board, Plinth::Legs] {
                let mut p = l_kitchen();
                p.settings.plinth = plinth;
                for c in p.cabinets.iter_mut() {
                    c.door_style = style;
                }
                let r = compute(&p);
                assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
                let parts: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "lc").collect();
                let get = |k: &str| parts.iter().find(|q| q.id == format!("lc-{k}")).unwrap();
                // L bottom: six corners, two banded front edges of 1000-18-560 and 1000-18-560
                let bot = get("bottom");
                assert_eq!(bot.outline.len(), 6);
                assert_eq!(bot.band_segs.len(), 2);
                assert!((bot.band_length() - 2.0 * (1000.0 - 18.0 - 560.0)).abs() < 0.5, "{}", bot.band_length());
                assert_eq!(bot.more3d.len(), 1);
                // every hole inside its outline; edge bores on an outline segment
                for q in &parts {
                    for h in &q.holes {
                        if h.edge.is_none() {
                            assert!(inside_outline(&q.outline, h.x, h.y), "{} {:?}", q.id, h);
                        } else {
                            let on = (0..q.outline.len()).any(|i| {
                                let (a, b) = (q.outline[i], q.outline[(i + 1) % q.outline.len()]);
                                let cross = (b[0] - a[0]) * (h.y - a[1]) - (b[1] - a[1]) * (h.x - a[0]);
                                let within = h.x >= a[0].min(b[0]) - 1e-6
                                    && h.x <= a[0].max(b[0]) + 1e-6
                                    && h.y >= a[1].min(b[1]) - 1e-6
                                    && h.y <= a[1].max(b[1]) + 1e-6;
                                cross.abs() < 1e-6 && within
                            });
                            assert!(on, "{} edge bore off the outline {:?}", q.id, h);
                        }
                    }
                }
                // joints: side face bores meet panel edge bores (cabinet-local x/z, height)
                let c = &p.cabinets[1];
                let (t, bt) = (18.0, 6.0);
                let (cd, xb, ze) = (c.depth - bt, c.width - c.depth, c.width2 - t);
                let jd = 7.0;
                let side_l = get("side-l");
                let side_e = get("side-r");
                let mut from_sides: Vec<(i64, i64, i64)> = side_l
                    .holes
                    .iter()
                    .filter(|h| h.dia == jd)
                    .map(|h| (0, (c.depth - h.x).round() as i64, (h.y + side_l.box3d.pos[1]).round() as i64))
                    .chain(
                        side_e
                            .holes
                            .iter()
                            .filter(|h| h.dia == jd)
                            .map(|h| (1, (xb + cd - h.x).round() as i64, (h.y + side_e.box3d.pos[1]).round() as i64)),
                    )
                    .collect();
                let mut from_panels: Vec<(i64, i64, i64)> = parts
                    .iter()
                    .flat_map(|q| {
                        let mid = (q.box3d.pos[1] + t / 2.0).round() as i64;
                        q.holes.iter().filter_map(move |h| match h.edge {
                            Some(3) => Some((0, (ze - h.y).round() as i64, mid)),
                            Some(0) => Some((1, (h.x + t).round() as i64, mid)),
                            _ => None,
                        })
                    })
                    .collect();
                from_sides.sort();
                from_panels.sort();
                assert_eq!(from_sides.len(), 12);
                assert_eq!(from_sides, from_panels);
                // bi-fold pair: the return leaf (or all its Shaker pieces) lies in the arm B front plane
                let leaf_b: Vec<_> = parts.iter().filter(|q| q.id.starts_with("lc-door2")).collect();
                assert!(!leaf_b.is_empty());
                for q in leaf_b {
                    assert!(q.box3d.pos[0] >= xb - 19.0 - 1e-6 && q.box3d.pos[0] + q.box3d.size[0] <= xb + 1e-6, "{} {:?}", q.id, q.box3d);
                }
                assert!(r.build.hardware.iter().any(|h| h.cabinet == "lc" && h.name.starts_with("Bi-fold")));
                // worktops: one along the back (a1 + arm A), one down the side (arm B + b1)
                let tops: Vec<_> = r.build.parts.iter().filter(|q| q.role == crate::parts::Role::Worktop).collect();
                assert_eq!(tops.len(), 2, "{:?}", tops.iter().map(|q| &q.id).collect::<Vec<_>>());
                let mut lens: Vec<i64> = tops.iter().map(|q| q.w.round() as i64).collect();
                lens.sort();
                let ov = p.settings.worktop_overhang;
                assert_eq!(lens, vec![(440.0 + 600.0 - ov) as i64, 1600]);
            }
        }
    }

    #[test]
    fn l_corner_blocks_both_walls_when_filling() {
        let mut p = l_kitchen();
        p.cabinets.retain(|c| c.id == "lc");
        // the L sits in the corner of walls 0 and 1: neither fill may place a cabinet over it
        for wi in [0, 1] {
            let filled = room::fill_wall(&p, wi, room::Tier::Base).unwrap();
            let mut q = p.clone();
            q.cabinets.extend(filled.into_iter());
            let mut w = vec![];
            checks::check(&q, &mut w);
            assert!(!w.iter().any(|m| m.contains("overlap")), "wall {wi}: {w:?}");
        }
    }
}
