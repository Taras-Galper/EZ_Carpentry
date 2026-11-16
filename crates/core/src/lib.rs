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
    fn worktop_spans_touching_base_run() {
        let r = compute(&sample_kitchen());
        let tops: Vec<_> = r.build.parts.iter().filter(|p| p.role == parts::Role::Worktop).collect();
        assert_eq!(tops.len(), 1);
        assert_eq!(tops[0].w, 600.0 + 800.0 + 450.0);
        assert_eq!(tops[0].h, 560.0 + 20.0);
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
