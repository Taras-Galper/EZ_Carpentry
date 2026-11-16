//! Casework: parametric cabinet design core.
//!
//! `Project` (JSON) -> `Report` (parts, hardware, nested sheets) -> exports.

pub mod export;
pub mod model;
pub mod nest;
pub mod parts;

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
    let build = parts::build(project);
    let nesting = nest::nest(project, &build.parts);
    let mut sheets: Vec<(String, usize)> = vec![];
    for s in &nesting.sheets {
        let name = project.material(&s.material).name;
        match sheets.iter_mut().find(|x| x.0 == name) {
            Some(x) => x.1 += 1,
            None => sheets.push((name, 1)),
        }
    }
    let band: f64 = build
        .parts
        .iter()
        .map(|p| {
            let e = p.edges;
            (if e[0] { p.w } else { 0.0 })
                + (if e[2] { p.w } else { 0.0 })
                + (if e[1] { p.h } else { 0.0 })
                + (if e[3] { p.h } else { 0.0 })
        })
        .sum::<f64>()
        / 1000.0;
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
    serde_json::from_str(json).map_err(|e| e.to_string())
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
        let r = compute(&sample_kitchen());
        for p in &r.build.parts {
            for h in &p.holes {
                assert!(h.x - h.dia / 2.0 >= -1e-6 && h.x + h.dia / 2.0 <= p.w + 1e-6, "{} {:?}", p.id, h);
                assert!(h.y - h.dia / 2.0 >= -1e-6 && h.y + h.dia / 2.0 <= p.h + 1e-6, "{} {:?}", p.id, h);
            }
        }
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
        assert_eq!(lens, vec![450.0 + 600.0 + 450.0, 600.0 + 800.0 + 1200.0]);
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
        let back = r.build.parts.iter().find(|q| q.id == "b1-worktop").unwrap();
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
        // identical drawer bases cost the same now that worktops are separate
        let price = |id: &str| q.items.iter().find(|i| i.id == id).unwrap().price;
        assert!((price("b1") - price("b5")).abs() < 1.0, "{} vs {}", price("b1"), price("b5"));
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
        assert!(d("b1").contains("3 drawers"), "{}", d("b1"));
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
        let door_holes = |r: &Report| -> usize {
            r.build.parts.iter().filter(|x| x.role == parts::Role::Front).map(|x| x.holes.len()).sum()
        };
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
}
