//! Casework: parametric cabinet design core.
//!
//! `Project` (JSON) -> `Report` (parts, hardware, nested sheets) -> exports.

pub mod accessories;
pub mod checks;
pub mod export;
pub mod import_dxf;
pub mod install;
pub mod lighting;
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
    let mut clashes = vec![];
    checks::machining(&build, &mut clashes);
    build.warnings.extend(clashes);
    let nesting = nest::nest(project, &build.parts);
    let mut sheets: Vec<(String, usize)> = vec![];
    for s in &nesting.sheets {
        let name = project.material(&s.material).name + if s.offcut { " offcut" } else { "" };
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
                        .filter(|q| q.cabinet == c.id && q.role != parts::Role::DrawerBox)
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
        // back run d1+b2+b3, return leg b4+b5+b6 and over b6's 19 mm end panel
        let mut lens: Vec<f64> = tops.iter().map(|t| t.w).collect();
        lens.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(lens, vec![450.0 + 600.0 + 450.0 + 19.0, 600.0 + 800.0 + 1150.0]);
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
        assert_eq!(leg.w, 450.0 + 600.0 + 450.0 + 19.0 - 580.0);
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
        // (b4 also carries the corner joint's worktop bolts)
        let mut p2 = p.clone();
        p2.cabinets.iter_mut().find(|c| c.id == "b6").unwrap().end_right = false;
        let q2 = export::quote::quote(&p2, &compute(&p2));
        let price = |id: &str| q2.items.iter().find(|i| i.id == id).unwrap().price;
        let bolts = 3.0 * p.pricing.hardware_price("Worktop connector bolt") * k;
        assert!((price("b4") - bolts - price("b6")).abs() < 1.0, "{} vs {}", price("b4"), price("b6"));
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
        assert_eq!(f1.len(), 1, "strip; its toe kick is part of the run plinth");
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
        assert_eq!(get("b4-plinth").name, "Plinth");
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

    #[test]
    fn worktop_overhangs_free_ends_only() {
        let mut p = sample_kitchen();
        p.settings.worktop_end_overhang = 25.0;
        let r = compute(&p);
        let leg = r.build.parts.iter().find(|q| q.id == "b4-worktop").unwrap();
        // free end beside b6: end panel 19 + overhang 25; the corner end stays trimmed
        assert_eq!(leg.w, 450.0 + 600.0 + 450.0 + 19.0 + 25.0);
        // the back run sits between the tall pantry and the return leg: no overhang
        let back = r.build.parts.iter().find(|q| q.id == "d1-worktop").unwrap();
        assert_eq!(back.w, 600.0 + 800.0 + 1150.0);
    }

    #[test]
    fn oven_housing() {
        for back_mode in [BackMode::Applied, BackMode::Grooved] {
            let mut p = sample_kitchen();
            p.settings.back_mode = back_mode;
            let t1 = p.cabinets.iter_mut().find(|c| c.id == "t1").unwrap();
            t1.drawers = vec![375.0, 375.0];
            t1.housing_elevation = 850.0;
            t1.housing_height = 590.0;
            t1.shelves = 1;
            let c = t1.clone();
            let r = compute(&p);
            assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
            let get = |k: &str| r.build.parts.iter().find(|q| q.id == format!("t1-{k}"));
            let (lo, hi) = (get("hfix-l").unwrap(), get("hfix-u").unwrap());
            assert_eq!(lo.box3d.pos[1] + lo.box3d.size[1], 850.0);
            assert_eq!(hi.box3d.pos[1], 850.0 + 590.0);
            // drawers below, upper doors above, nothing in front of the opening
            let has = |k: &str| r.build.parts.iter().any(|q| q.id.starts_with(&format!("t1-{k}")));
            assert!(has("dfront2") && has("udoor1") && !has("door1"));
            let fronts: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "t1" && q.role == parts::Role::Front).collect();
            for f in &fronts {
                let (y0, y1) = (f.box3d.pos[1], f.box3d.pos[1] + f.box3d.size[1]);
                assert!(y1 <= 850.0 + 0.01 || y0 >= 1440.0 - 0.01, "{} covers the opening", f.id);
            }
            // the adjustable shelf sits above the opening
            assert!(get("shelf1").unwrap().box3d.pos[1] > 1440.0);
            // fixed-shelf joints: side bores at each shelf's mid height, three per side
            let side = get("side-l").unwrap();
            for q in [lo, hi] {
                let mid = q.box3d.pos[1] + 9.0 - side.box3d.pos[1];
                let n = side.holes.iter().filter(|h| h.dia == 7.0 && (h.y - mid).abs() < 0.01).count();
                assert_eq!(n, 3, "{}", q.id);
                assert_eq!(q.holes.iter().filter(|h| h.edge.is_some()).count(), 6);
            }
            let _ = c;
        }
    }

    #[test]
    fn wall_elevations_show_their_cabinets() {
        let p = sample_kitchen();
        let r = compute(&p);
        let w1 = export::drawing::wall_svg(&p, &r.build, 0).unwrap();
        for id in ["t1", "d1", "b2", "b3", "w1", "w3"] {
            assert!(w1.contains(&format!(">{id}</text>")), "{id} missing on wall 1");
        }
        assert!(w1.contains("Window 700"));
        let w2 = export::drawing::wall_svg(&p, &r.build, 1).unwrap();
        assert!(w2.contains(">b5</text>") && !w2.contains(">b2</text>"));
        assert!(export::drawing::wall_svg(&p, &r.build, 9).is_none());
        let plan = export::drawing::plan_svg(&p).unwrap();
        assert!(p.cabinets.iter().all(|c| plan.contains(&format!(">{}</text>", c.id))));
        assert!(plan.contains(">3200</text>") && plan.contains(">2600</text>"));
    }

    /// Free-standing island in the sample room: row A faces +z, row B is
    /// back to back with it (rotation 180), B has the sink.
    fn island(back_row: bool) -> Project {
        let mut p = sample_kitchen();
        let base = Cabinet { group: "island".into(), ..Cabinet::default() };
        let mut cabs = vec![
            Cabinet { id: "i1".into(), x: 900.0, z: 1300.0, ..base.clone() },
            Cabinet { id: "i2".into(), x: 1500.0, z: 1300.0, ..base.clone() },
        ];
        if back_row {
            // rotation 180: origin at the back-left seen from its own front = world right end
            cabs.push(Cabinet {
                id: "j1".into(),
                x: 2100.0,
                z: 1300.0,
                rotation: 180.0,
                cutout: CutoutKind::Sink,
                width: 1200.0,
                ..base.clone()
            });
        } else {
            for c in cabs.iter_mut() {
                c.worktop_back = 300.0;
            }
        }
        p.cabinets.retain(|c| c.elevation == 0.0 && c.z < 100.0 && c.rotation == 0.0);
        p.cabinets.extend(cabs);
        p
    }

    #[test]
    fn back_to_back_island_shares_one_worktop() {
        let p = island(true);
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let tops: Vec<_> =
            r.build.parts.iter().filter(|q| q.role == parts::Role::Worktop && (q.cabinet == "i1" || q.cabinet == "j1")).collect();
        assert_eq!(tops.len(), 1);
        let t = tops[0];
        let ov = p.settings.worktop_overhang;
        assert_eq!(t.h, 2.0 * (560.0 + ov));
        assert_eq!(t.w, 1200.0);
        // the sink sits over j1, set back from j1's front (the worktop's back edge)
        let c = &t.cutouts[0];
        assert_eq!(c.x + c.w / 2.0, 600.0);
        assert_eq!(t.h - (c.y + c.h), 60.0);
        assert_eq!(t.box3d.pos[2], -(560.0 + ov));
    }

    #[test]
    fn island_seating_overhang() {
        let mut p = island(false);
        p.settings.worktop_end_overhang = 20.0;
        let r = compute(&p);
        let t = r.build.parts.iter().find(|q| q.id == "i1-worktop").unwrap();
        assert_eq!(t.h, 560.0 + p.settings.worktop_overhang + 300.0);
        assert_eq!(t.box3d.pos[2], -300.0);
        // both ends free: end overhang on each
        assert_eq!(t.w, 1200.0 + 40.0);
    }

    #[test]
    fn peninsula_end_at_wall_gets_no_end_overhang() {
        let mut p = sample_kitchen();
        p.settings.worktop_end_overhang = 20.0;
        p.cabinets.retain(|c| c.elevation == 0.0 && c.z < 100.0 && c.rotation == 0.0 && c.x < 600.0);
        // rotation -90: local x runs +z, fronts face -x; backs on x = 1700, ending at the front wall
        for (k, z) in [(1, 1000.0), (2, 1600.0)] {
            p.cabinets.push(Cabinet { id: format!("p{k}"), x: 1700.0, z, rotation: -90.0, worktop_back: 300.0, ..Cabinet::default() });
        }
        p.cabinets.push(Cabinet {
            id: "p3".into(),
            x: 1700.0,
            z: 2200.0,
            rotation: -90.0,
            width: 400.0,
            worktop_back: 300.0,
            ..Cabinet::default()
        });
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let t = r.build.parts.iter().find(|q| q.id == "p1-worktop").unwrap();
        // run ends at the front wall (z = 2600): overhang only at the free end near z = 1000
        assert_eq!(t.w, 1600.0 + 20.0);
        assert_eq!(t.box3d.pos[0], -20.0);
    }

    #[test]
    fn run_plinths() {
        let p = sample_kitchen();
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        assert!(!r.build.parts.iter().any(|q| q.id.ends_with("-toe")), "no per-cabinet toe kicks");
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id);
        // return leg b4+b5+b6: one board, a return at the free end (b6 has an end panel: none)
        let leg = get("b4-plinth").unwrap();
        assert_eq!(leg.w, 1500.0);
        assert_eq!(leg.shared.len(), 3);
        assert!(get("b4-plinth-rr").is_none() && get("b4-plinth-rl").is_none());
        // without b6's end panel the free end gets a return
        let mut p2 = p.clone();
        p2.cabinets.iter_mut().find(|c| c.id == "b6").unwrap().end_right = false;
        let r2 = compute(&p2);
        let ret = r2.build.parts.iter().find(|q| q.id == "b4-plinth-rr").unwrap();
        assert_eq!(ret.w, 560.0 - 60.0 - 18.0 - 20.0);
        // per-cabinet toe kicks when switched off
        let mut p3 = p.clone();
        p3.settings.plinth_runs = false;
        let r3 = compute(&p3);
        assert!(r3.build.parts.iter().any(|q| q.id == "b4-toe"));
        assert!(!r3.build.parts.iter().any(|q| q.id.contains("-plinth")));
        // a run longer than a board is split evenly
        let mut p4 = p.clone();
        p4.room = crate::room::Room::rectangle(9000.0, 2600.0);
        p4.cabinets = (0..10).map(|k| Cabinet { id: format!("c{k}"), x: 100.0 + 600.0 * k as f64, ..Cabinet::default() }).collect();
        let r4 = compute(&p4);
        let pieces: Vec<_> = r4.build.parts.iter().filter(|q| q.id.starts_with("c0-plinth") && !q.id.contains("-r")).collect();
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|q| (q.w - 2000.0).abs() < 0.1));
    }

    #[test]
    fn lift_up_and_glass_fronts() {
        let mut p = sample_kitchen();
        for c in p.cabinets.iter_mut() {
            if c.id == "w1" {
                c.lift_up = true;
                c.doors = 2;
            }
            if c.id == "w3" {
                c.glass = true;
                c.door_style = DoorStyle::Slab;
            }
        }
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let hw = |cab: &str, name: &str| {
            r.build.hardware.iter().filter(|h| h.cabinet == cab && h.name.starts_with(name)).map(|h| h.qty).sum::<u32>()
        };
        // lift-up: one full-width front (as Shaker pieces), a stay fitting, no hinges
        let w1_fronts: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "w1" && q.role == parts::Role::Front).collect();
        assert!(w1_fronts.iter().all(|q| q.id.starts_with("w1-door1")));
        assert_eq!(hw("w1", "Lift-up stay fitting"), 1);
        assert_eq!(hw("w1", "Clip hinge"), 0);
        // glass: slab style still becomes a frame; the panel is glass, bought in
        let w3: Vec<_> =
            r.build.parts.iter().filter(|q| q.cabinet == "w3" && q.role == parts::Role::Front).map(|q| q.id.as_str()).collect();
        assert_eq!(w3.len(), 4, "{w3:?}");
        assert!(!w3.iter().any(|id| id.ends_with("-pnl")));
        assert_eq!(hw("w3", "Glass 4 mm"), 1);
        assert_eq!(hw("w3", "Clip hinge"), 2);
        assert!(r.build.placeholders.iter().any(|ph| ph.cabinet == "w3" && ph.label == "Glass"));
        // hinge cups moved onto a stile
        let stile_cups = r.build.parts.iter().filter(|q| q.cabinet == "w3").flat_map(|q| q.holes.iter()).filter(|h| h.dia == 35.0).count();
        assert_eq!(stile_cups, 2);
    }

    #[test]
    fn customer_quote_document() {
        let mut p = sample_kitchen();
        p.pricing.company.name = "Galper & Co".into();
        p.pricing.company.logo = "data:image/png;base64,AAAA".into();
        p.pricing.company.valid_days = 30;
        p.client.name = "Dana <Levi>".into();
        let r = compute(&p);
        let q = export::quote::quote(&p, &r);
        let html = export::quote::quote_html(&p, &q, Some("2026-12-15"), &["data:image/png;base64,BBBB".into(), "https://x/y.png".into()]);
        assert!(html.contains("Galper &amp; Co") && html.contains("Dana &lt;Levi&gt;"));
        assert!(html.contains("Valid until 2027-01-14"));
        assert_eq!(html.matches("class=pic ").count(), 1, "only data: images");
        assert!(html.contains("class=logo"));
        assert!(html.contains("50% deposit"));
    }

    /// Parts on each sheet keep the cutter spacing from every other part's
    /// real outline (so parts may sit in an L's notch, never on the L).
    fn assert_outlines_apart(p: &Project, r: &Report) {
        let k = p.settings.kerf.max(p.cnc.router.dia) - 1e-6;
        for s in &r.nesting.sheets {
            // each part's outline as sheet-space rectangles (L = 2 rects)
            let rects: Vec<Vec<[f64; 4]>> = s
                .placements
                .iter()
                .map(|pl| {
                    let q = &r.build.parts[pl.part];
                    let mut boxes = vec![[0.0, 0.0, q.w, q.h]];
                    if q.outline.len() == 6 {
                        // split the L along its inner vertex: two rectangles from the outline's x / y values
                        let xs: Vec<f64> = q.outline.iter().map(|v| v[0]).collect();
                        let ys: Vec<f64> = q.outline.iter().map(|v| v[1]).collect();
                        let xi = xs.iter().copied().find(|x| *x > 0.01 && *x < q.w - 0.01).unwrap();
                        let yi = ys.iter().copied().find(|y| *y > 0.01 && *y < q.h - 0.01).unwrap();
                        let inside = |x: f64, y: f64| {
                            let o = &q.outline;
                            let mut c = false;
                            for i in 0..o.len() {
                                let (a, b) = (o[i], o[(i + 1) % o.len()]);
                                if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                                    c = !c;
                                }
                            }
                            c
                        };
                        let quads = [[0.0, 0.0, xi, yi], [xi, 0.0, q.w, yi], [0.0, yi, xi, q.h], [xi, yi, q.w, q.h]];
                        boxes = quads.into_iter().filter(|b| inside((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)).collect();
                    }
                    boxes
                        .into_iter()
                        .map(|b| {
                            let (a, c) = (pl.map(q.w, b[0], b[1]), pl.map(q.w, b[2], b[3]));
                            [a.0.min(c.0), a.1.min(c.1), a.0.max(c.0), a.1.max(c.1)]
                        })
                        .collect()
                })
                .collect();
            for i in 0..rects.len() {
                for j in i + 1..rects.len() {
                    for a in &rects[i] {
                        for b in &rects[j] {
                            let sep = a[2] + k <= b[0] || b[2] + k <= a[0] || a[3] + k <= b[1] || b[3] + k <= a[1];
                            assert!(sep, "parts {} and {} too close on {}", s.placements[i].part, s.placements[j].part, s.material);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn small_parts_nest_into_l_notches() {
        let mut p = l_kitchen();
        // a few extra small cabinets so there are small carcass parts to fill notches
        for k in 0..4 {
            p.cabinets.push(Cabinet {
                id: format!("s{k}"),
                x: 100.0 + 300.0 * k as f64,
                width: 300.0,
                kind: CabinetKind::Wall,
                elevation: 1450.0,
                height: 400.0,
                depth: 300.0,
                ..Cabinet::default()
            });
        }
        let r = compute(&p);
        assert_outlines_apart(&p, &r);
        let bottom = r.build.parts.iter().position(|q| q.id == "lc-bottom").unwrap();
        let (s, pl) = r.nesting.sheets.iter().find_map(|s| s.placements.iter().find(|pl| pl.part == bottom).map(|pl| (s, pl))).unwrap();
        let in_notch = s
            .placements
            .iter()
            .filter(|o| {
                o.part != bottom
                    && o.x >= pl.x - 1e-6
                    && o.y >= pl.y - 1e-6
                    && o.x + o.w <= pl.x + pl.w + 1e-6
                    && o.y + o.h <= pl.y + pl.h + 1e-6
            })
            .count();
        assert!(in_notch > 0, "nothing placed in the L bottom's notch");
    }

    #[test]
    fn stock_offcuts_are_used_first_and_free() {
        let mut p = sample_kitchen();
        let r0 = compute(&p);
        let q0 = export::quote::quote(&p, &r0);
        let mi = p.materials.iter().position(|m| m.id == "carcass").unwrap();
        p.materials[mi].offcuts = vec![[1400.0, 900.0], [300.0, 100.0]];
        let r = compute(&p);
        assert_outlines_apart(&p, &r);
        let off: Vec<_> = r.nesting.sheets.iter().filter(|s| s.offcut).collect();
        assert_eq!(off.len(), 1, "the 300 x 100 offcut fits nothing and is left out");
        assert_eq!((off[0].length, off[0].width), (1400.0, 900.0));
        assert!(off[0].placements.iter().all(|pl| pl.x + pl.w <= 1400.0 - off[0].trim + 1e-6 && pl.y + pl.h <= 900.0 - off[0].trim + 1e-6));
        let q = export::quote::quote(&p, &r);
        let sheets = |q: &export::quote::Quote| {
            q.lines.iter().filter(|l| l.group == "Material" && l.item == "White melamine 18").map(|l| l.qty).sum::<f64>()
        };
        assert!(sheets(&q) <= sheets(&q0), "{} vs {}", sheets(&q), sheets(&q0));
    }

    #[test]
    fn diagonal_wall_corner() {
        for style in [DoorStyle::Slab, DoorStyle::Shaker] {
            let mut p = l_kitchen();
            p.cabinets.retain(|c| c.id == "wl");
            p.cabinets[0].corner = Corner::Diagonal;
            p.cabinets[0].door_style = style;
            let r = compute(&p);
            assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
            let parts: Vec<_> = r.build.parts.iter().filter(|q| q.cabinet == "wl").collect();
            let get = |k: &str| parts.iter().find(|q| q.id == format!("wl-{k}")).unwrap();
            let top = get("top");
            assert_eq!(top.outline.len(), 5);
            assert_eq!(top.shape.len(), 5);
            assert_eq!(top.band_segs.len(), 1);
            // the banded edge is the diagonal: (600-320)·√2 less the side thickness share
            let diag = top.band_length();
            assert!(diag > 300.0 && diag < 400.0, "{diag}");
            for q in &parts {
                for h in q.holes.iter().filter(|h| h.edge.is_none()) {
                    assert!(inside_outline(&q.outline, h.x, h.y), "{} {:?}", q.id, h);
                }
            }
            // one door (or its Shaker pieces) turned -45° (local x running +x +z) across the corner, in front of the carcass line
            let doors: Vec<_> = parts.iter().filter(|q| q.role == parts::Role::Front).collect();
            assert!(!doors.is_empty());
            for q in &doors {
                assert!((q.rot + 45.0).abs() < 1e-6, "{} {}", q.id, q.rot);
                let [x, _, z] = q.box3d.pos;
                assert!(z - 320.0 - x >= -0.01, "{} behind the diagonal", q.id);
                // line z = 320 + x
            }
            assert!(r.build.hardware.iter().any(|h| h.cabinet == "wl" && h.name.starts_with("Clip hinge +45°")));
            let obj = export::obj(&p, &r.build);
            assert!(obj.contains("o wl_Top"));
        }
    }

    #[test]
    fn cornice_and_light_rail() {
        let mut p = sample_kitchen();
        p.settings.cornice = 60.0;
        p.settings.light_rail = 40.0;
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id);
        // w1 sits against the tall pantry (closed left end) with a free right end
        let c = get("w1-cornice").unwrap();
        assert_eq!(c.w, 600.0 + 19.0);
        assert_eq!(c.box3d.pos[1], 720.0); // cabinet-local: on top
        assert!(get("w1-cornice-rr").is_some() && get("w1-cornice-rl").is_none());
        assert_eq!(get("w1-cornice-rr").unwrap().w, 320.0);
        // light rail under it, same returns
        let l = get("w1-lightrail").unwrap();
        assert_eq!(l.box3d.pos[1], -40.0); // just under the bottom
        assert!(get("w1-lightrail-rr").is_some());
        // the tall pantry has its own cornice; no light rail
        assert!(get("f1-cornice").is_some() || get("t1-cornice").is_some());
        assert!(get("t1-lightrail").is_none() && get("f1-lightrail").is_none());
        // off by default
        let r0 = compute(&sample_kitchen());
        assert!(!r0.build.parts.iter().any(|q| q.id.contains("cornice") || q.id.contains("lightrail")));
    }

    #[test]
    fn led_lighting() {
        let mut p = sample_kitchen();
        for c in p.cabinets.iter_mut() {
            match c.id.as_str() {
                "w1" => {
                    c.led_under = true;
                    c.led_shelves = true;
                }
                "w3" => c.led_under = true,
                "b4" | "b5" | "b6" => c.led_plinth = true,
                _ => {}
            }
        }
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id).unwrap();
        // the wall cabinet's bottom: groove on the underside near the front, cable hole near the back
        let bot = get("w1-bottom");
        let g = bot.pockets.iter().find(|k| k.face == 2).unwrap();
        assert_eq!((g.y, g.h), (30.0, 17.0));
        assert_eq!(g.w, bot.w - 100.0);
        // clear of the joint bores in the panel ends
        assert!(bot.holes.iter().filter(|h| h.edge.is_some()).all(|h| h.depth < g.x));
        let cable = bot.holes.iter().find(|h| h.dia == 10.0).unwrap();
        assert!(bot.holes.iter().filter(|h| h.edge == Some(3)).all(|h| cable.x - cable.dia / 2.0 > h.depth + 5.0));
        assert!(g.depth <= bot.thickness - 6.0);
        assert!(bot.holes.iter().any(|h| h.dia == 10.0 && h.depth == 0.0 && h.y > bot.h - 50.0));
        assert!(get("w1-shelf1").pockets.iter().any(|k| k.face == 2));
        // strips: w1 bottom + 2 shelves, w3 bottom, one plinth run
        assert_eq!(r.build.lights.len(), 5);
        // circuits: w1 and w3 are apart on the wall, the plinth has its own
        assert_eq!(r.build.circuits.len(), 3);
        for c in &r.build.circuits {
            assert!(c.driver >= c.watts * 1.2, "{c:?}");
        }
        let plinth = r.build.lights.iter().find(|l| l.place == "Plinth").unwrap();
        assert_eq!(plinth.length, 1500.0 - 40.0);
        let hw = |name: &str| r.build.hardware.iter().filter(|h| h.name.starts_with(name)).map(|h| h.qty).sum::<u32>();
        assert_eq!(hw("LED driver"), 3);
        assert_eq!(hw("LED profile"), 4);
        // the groove reaches the second-face program
        let files = export::files(&p, &r);
        assert!(files.iter().any(|(n, _)| n.starts_with("nc/face2/") && n.contains("w1-bottom")));
    }

    #[test]
    fn accessories_listed_and_checked() {
        let mut p = sample_kitchen();
        for c in p.cabinets.iter_mut() {
            match c.id.as_str() {
                "b5" => c.accessories = vec!["cutlery".into()],
                "b2" => c.accessories = vec!["bin".into()],
                "b3" => c.accessories = vec!["blind_pullout".into()],
                "w1" => c.accessories = vec!["plate_rack".into(), "larder".into()],
                _ => {}
            }
        }
        let r = compute(&p);
        let hw = |cab: &str| r.build.hardware.iter().filter(|h| h.cabinet == cab).map(|h| h.name.clone()).collect::<Vec<_>>();
        assert!(hw("b5").iter().any(|n| n == "Cutlery insert 550"), "{:?}", hw("b5"));
        assert!(hw("b2").iter().any(|n| n == "Waste bin pull-out 750"));
        assert!(hw("b3").iter().any(|n| n.starts_with("Blind corner pull-out")));
        assert!(hw("w1").iter().any(|n| n == "Plate rack 550"));
        // a larder pull-out does not go in a wall cabinet: warned, not listed
        assert!(!hw("w1").iter().any(|n| n.starts_with("Larder")));
        assert_eq!(r.build.warnings.len(), 1, "{:?}", r.build.warnings);
        assert!(r.build.warnings[0].contains("larder pull-out is for tall cabinets"));
        assert!(accessories::fitting(&p.cabinets.iter().find(|c| c.id == "t1").unwrap().clone()).contains(&"larder"));
    }

    #[test]
    fn obstacles_block_fill_and_warn() {
        let mut p = sample_kitchen();
        p.cabinets.clear();
        p.room.obstacles.push(room::Obstacle { wall: 0, offset: 1200.0, width: 400.0, depth: 300.0, ..Default::default() });
        // a radiator under the window height on wall 1 does not stop wall cabinets
        p.room.obstacles.push(room::Obstacle {
            wall: 1,
            kind: room::ObstacleKind::Radiator,
            offset: 800.0,
            width: 800.0,
            depth: 100.0,
            height: 600.0,
            elevation: 150.0,
        });
        let base = room::fill_wall(&p, 0, room::Tier::Base).unwrap();
        assert!(!base.is_empty());
        for c in &base {
            assert!(c.x + c.width <= 1200.0 + 0.5 || c.x >= 1600.0 - 0.5, "{} at {}..{}", c.id, c.x, c.x + c.width);
        }
        let up = room::fill_wall(&p, 1, room::Tier::Wall).unwrap();
        let down = room::fill_wall(&p, 1, room::Tier::Base).unwrap();
        let span = |v: &Vec<Cabinet>| v.iter().map(|c| c.width).sum::<f64>();
        assert!(span(&up) > span(&down) + 700.0, "radiator blocks base units only");
        p.cabinets = base;
        assert!(compute(&p).build.warnings.is_empty());
        p.cabinets.push(Cabinet { id: "x1".into(), x: 1300.0, ..Cabinet::default() });
        let w = compute(&p).build.warnings;
        assert!(w.iter().any(|m| m.contains("runs into the column on wall 1")), "{w:?}");
    }

    #[test]
    fn upstands_along_walls() {
        let mut p = sample_kitchen();
        p.settings.upstand = 100.0;
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id);
        let back = get("d1-upstand").unwrap();
        assert_eq!(back.w, get("d1-worktop").unwrap().w);
        assert_eq!((back.h, back.thickness), (100.0, 20.0));
        // the back run ends at the right wall; its left end meets the tall pantry
        assert!(get("d1-upstand-r").is_some() && get("d1-upstand-l").is_none());
        // the return leg: along the wall, nothing at its free end or at the corner
        assert!(get("b4-upstand").is_some());
        assert!(get("b4-upstand-l").is_none() && get("b4-upstand-r").is_none());
        let q = export::quote::quote(&p, &r);
        assert!(q.items.iter().any(|i| i.name == "Upstand"));
        assert!(compute(&sample_kitchen()).build.parts.iter().all(|q| !q.id.contains("upstand")));
    }

    #[test]
    fn led_in_l_corner() {
        let mut p = l_kitchen();
        let wl = p.cabinets.iter_mut().find(|c| c.id == "wl").unwrap();
        wl.led_under = true;
        wl.led_shelves = true;
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let bot = r.build.parts.iter().find(|q| q.id == "wl-bottom").unwrap();
        let grooves: Vec<_> = bot.pockets.iter().filter(|k| k.face == 2).collect();
        assert_eq!(grooves.len(), 2);
        // both grooves lie in the L, not in its notch
        for k in &grooves {
            for (x, y) in [(k.x, k.y), (k.x + k.w, k.y), (k.x, k.y + k.h), (k.x + k.w, k.y + k.h)] {
                let (x, y) = (x.clamp(0.1, bot.w - 0.1), y.clamp(0.1, bot.h - 0.1));
                assert!(inside_outline(&bot.outline, x, y), "{k:?}");
            }
        }
        let strips = r.build.lights.iter().filter(|l| l.cabinet == "wl").count();
        assert_eq!(strips, 2 * (1 + 1), "two strips per panel: bottom + 1 shelf");
        assert_eq!(r.build.circuits.len(), 1);
    }

    #[test]
    fn machined_parts_allow_for_edge_band() {
        let mut p = l_kitchen();
        p.settings.edge_band = 1.0;
        let r = compute(&p);
        let cut = export::machined_parts(&p, &r.build);
        for (f, c) in r.build.parts.iter().zip(&cut) {
            // cut size matches the cut list; the band puts back what was taken off
            let (cw, ch) = f.cut_size(1.0);
            if f.band_segs.is_empty() {
                assert!((c.w - cw).abs() < 0.11 && (c.h - ch).abs() < 0.11, "{} {}x{} vs {}x{}", f.id, c.w, c.h, cw, ch);
            }
            // every bore keeps its distance from the finished edges
            let dx = if f.edges[3] { 1.0 } else { 0.0 };
            let dy = if f.edges[0] { 1.0 } else { 0.0 };
            for (a, b) in f.holes.iter().zip(&c.holes) {
                assert!((a.x - dx - b.x).abs() < 0.11 && (a.y - dy - b.y).abs() < 0.11, "{} {:?} {:?}", f.id, a, b);
            }
        }
        // L bottom: the two banded front edges move in by 1 mm, its outer size stays
        let i = r.build.parts.iter().position(|q| q.id == "lc-bottom").unwrap();
        let (f, c) = (&r.build.parts[i], &cut[i]);
        assert_eq!((c.w, c.h), (f.w, f.h));
        let area = |o: &Vec<[f64; 2]>| {
            (0..o.len()).map(|k| o[k][0] * o[(k + 1) % o.len()][1] - o[(k + 1) % o.len()][0] * o[k][1]).sum::<f64>().abs() / 2.0
        };
        let lost = area(&f.outline) - area(&c.outline);
        let fronts = f.band_length();
        assert!((lost - fronts).abs() < 2.0, "lost {lost} vs {fronts}");
        // a door banded all round is 2 mm smaller each way; its hinge cups 1 mm closer to the cut edge
        let d = r.build.parts.iter().position(|q| q.id == "a1-door1").unwrap();
        assert_eq!((cut[d].w, cut[d].h), (r.build.parts[d].w - 2.0, r.build.parts[d].h - 2.0));
        // with no band nothing changes
        p.settings.edge_band = 0.0;
        let r0 = compute(&p);
        let c0 = export::machined_parts(&p, &r0.build);
        assert!(r0.build.parts.iter().zip(&c0).all(|(a, b)| a.outline == b.outline && a.w == b.w));
    }

    #[test]
    fn fixing_hardware() {
        let p = sample_kitchen();
        let r = compute(&p);
        let qty =
            |cab: &str, name: &str| r.build.hardware.iter().filter(|h| h.cabinet == cab && h.name == name).map(|h| h.qty).sum::<u32>();
        let total = |name: &str| r.build.hardware.iter().filter(|h| h.name == name).map(|h| h.qty).sum::<u32>();
        assert_eq!(qty("w1", "Wall cabinet hanger (pair)"), 1);
        assert_eq!(qty("t1", "Anti-tip wall bracket"), 2);
        assert_eq!(qty("b2", "Wall fixing bracket"), 2);
        assert_eq!(qty("b2", "Worktop fixing screw"), 4);
        // one worktop corner joint (back run / return leg)
        assert_eq!(total("Worktop connector bolt"), 3);
        // b2-b3, b4-b5, b5-b6 and the tall pantry with the wall cabinet beside it
        // (d1 is a dishwasher, f1 a filler): 4 screws each
        assert_eq!(total("Cabinet connector screw"), 16);
    }

    #[test]
    fn shaker_drawer_handles_go_on_the_rail() {
        let p = sample_kitchen(); // Shaker fronts; hob base has a drawer stack
        let r = compute(&p);
        // no bore of any kind lands in a floating panel
        for q in r.build.parts.iter().filter(|q| q.name.ends_with(" panel")) {
            assert!(q.holes.is_empty(), "{} has {} bores", q.id, q.holes.len());
        }
        let rail = r.build.parts.iter().find(|q| q.id == "b5-dfront2-rt").unwrap();
        assert_eq!(rail.holes.len(), 2);
        assert!(rail.holes.iter().all(|h| (h.y - 30.0).abs() < 0.11));
        let packet = export::packet::packet_html(&p, &r, None);
        assert!(packet.contains("Frame fronts"));
    }

    #[test]
    fn no_machining_clashes_in_any_construction() {
        for joinery in [Joinery::Confirmat, Joinery::Dowel] {
            for back_mode in [BackMode::Applied, BackMode::Grooved] {
                for plinth in [Plinth::Board, Plinth::Legs] {
                    for style in [DoorStyle::Slab, DoorStyle::Shaker, DoorStyle::ShakerRouted] {
                        for mut p in [sample_kitchen(), l_kitchen()] {
                            p.settings.joinery = joinery;
                            p.settings.back_mode = back_mode;
                            p.settings.plinth = plinth;
                            for c in p.cabinets.iter_mut() {
                                c.door_style = style;
                                c.led_under = c.kind == CabinetKind::Wall && !c.is_diagonal();
                                c.led_shelves = c.shelves > 0 && !c.is_diagonal();
                            }
                            let r = compute(&p);
                            let clashes: Vec<_> = r.build.warnings.iter().filter(|w| w.contains("runs into")).collect();
                            assert!(clashes.is_empty(), "{joinery:?} {back_mode:?} {plinth:?} {style:?}: {clashes:?}");
                        }
                    }
                }
            }
        }
        // and the checker does catch one
        let p = sample_kitchen();
        let mut b = compute(&p).build;
        let side = b.parts.iter_mut().find(|q| q.id == "b2-side-l").unwrap();
        let h = side.holes[0].clone();
        side.holes.push(parts::Hole { x: h.x + 3.0, ..h });
        let mut w = vec![];
        checks::machining(&b, &mut w);
        assert!(w.iter().any(|m| m.starts_with("b2-side-l")), "{w:?}");
    }

    #[test]
    fn drawer_boxes_are_drilled_and_fastened() {
        let mut p = sample_kitchen();
        for joinery in [Joinery::Confirmat, Joinery::Dowel] {
            p.settings.joinery = joinery;
            let r = compute(&p);
            let get = |id: &str| r.build.parts.iter().find(|q| q.id == id).unwrap();
            let (side, front) = (get("b4-d1sidel"), get("b4-d1f"));
            let ys = |hs: Vec<f64>| {
                let mut v: Vec<i64> = hs.into_iter().map(|y| y.round() as i64).collect();
                v.sort();
                v.dedup();
                v
            };
            // the side's bores at each end meet the front / back edge bores
            assert_eq!(
                ys(side.holes.iter().map(|h| h.y).collect()),
                ys(front.holes.iter().filter(|h| h.edge.is_some()).map(|h| h.y).collect())
            );
            let dt = side.thickness;
            assert!(side.holes.iter().all(|h| (h.x - dt / 2.0).abs() < 0.11 || (h.x - (side.w - dt / 2.0)).abs() < 0.11));
            // one fastener per edge bore
            let bores: usize = r.build.parts.iter().map(|q| q.holes.iter().filter(|h| h.edge.is_some()).count()).sum();
            let name = if joinery == Joinery::Confirmat { "Confirmat screw 7×50" } else { "Dowel 8×35" };
            let n: u32 = r.build.hardware.iter().filter(|h| h.name == name).map(|h| h.qty).sum();
            assert_eq!(n as usize, bores);
        }
    }

    #[test]
    fn worktop_joint_bolt_recesses() {
        let p = sample_kitchen();
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let get = |id: &str| r.build.parts.iter().find(|q| q.id == id).unwrap();
        let (back, leg) = (get("d1-worktop"), get("b4-worktop"));
        // three holes + three slots on each side of the joint, on the underside
        for t in [back, leg] {
            assert_eq!(t.pockets.len(), 6, "{}", t.id);
            assert!(t.pockets.iter().all(|k| k.face == 2 && k.depth < t.thickness));
            assert!(
                t.pockets.iter().all(|k| k.x >= -0.01 && k.y >= -0.01 && k.x + k.w <= t.w + 0.01 && k.y + k.h <= t.h + 0.01),
                "{:?}",
                t.pockets
            );
        }
        // the leg's slots run out to its start (the joint); the back run's to its front edge
        assert!(leg.pockets.iter().filter(|k| k.h == 15.0).all(|k| k.x == 0.0));
        assert!(back.pockets.iter().filter(|k| k.w == 15.0).all(|k| k.y == 0.0));
        // holes face each other across the joint: same three bolt lines
        let holes = |t: &parts::Part| t.pockets.iter().filter(|k| k.r > 0.0).count();
        assert_eq!((holes(back), holes(leg)), (3, 3));
        // in the room, each hole on one side has its partner 150 mm away on the other
        let world = |t: &parts::Part| -> Vec<(f64, f64)> {
            let c = p.cabinets.iter().find(|c| c.id == t.cabinet).unwrap();
            t.pockets
                .iter()
                .filter(|k| k.r > 0.0)
                .map(|k| {
                    let (xd, yd) = (k.x + 17.5, k.y + 17.5);
                    let (lx, lz) = (t.box3d.pos[0] + xd, t.box3d.pos[2] + t.box3d.size[2] - yd);
                    parts::to_world(c.x, c.z, c.rotation, lx, lz)
                })
                .collect()
        };
        let (a, b) = (world(back), world(leg));
        for pa in &a {
            let near = b.iter().map(|pb| ((pa.0 - pb.0).powi(2) + (pa.1 - pb.1).powi(2)).sqrt()).fold(f64::MAX, f64::min);
            assert!((near - 150.0).abs() < 0.5, "{near}");
        }
        let files = export::files(&p, &r);
        assert!(files.iter().any(|(n, _)| n == "nc/face2/d1-worktop.nc"));
    }
}
