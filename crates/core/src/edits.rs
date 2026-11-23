//! Per-part edits made in the part view: banding, material, grain and
//! bores added, moved or removed. Keyed by part id and applied after the
//! cabinets are built, so exports, nesting, NC and the checks all see them.
//! Edits that no longer fit the part (the cabinet changed) are reported.

use crate::model::Project;
use crate::parts::{Build, Hole, Part};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PartEdit {
    /// Banding [bottom, right, top, left] for rectangular parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<[bool; 4]>,
    /// Banded outline segments for shaped parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band_segs: Option<Vec<usize>>,
    /// Another material of the same thickness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain_y: Option<bool>,
    /// Bores added (a moved bore is removed and added again).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<Hole>,
    /// Built bores taken out, matched on position, diameter and edge.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<Hole>,
}

impl PartEdit {
    pub fn is_empty(&self) -> bool {
        *self == PartEdit::default()
    }
}

fn same(a: &Hole, b: &Hole) -> bool {
    (a.x - b.x).abs() < 0.05 && (a.y - b.y).abs() < 0.05 && (a.dia - b.dia).abs() < 0.05 && a.edge == b.edge
}

/// The cabinet a part id belongs to ("b3-side-l" → "b3", "b3~b" → "b3").
pub fn cabinet_of(id: &str) -> &str {
    id.split(['-', '~']).next().unwrap_or(id)
}

fn inside(outline: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut c = false;
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

/// Does bore `h` sit on part `p`: a face bore inside the outline and clear
/// of cut-outs, an edge bore on its edge.
fn fits(p: &Part, h: &Hole) -> bool {
    match h.edge {
        None => {
            let r = h.dia / 2.0;
            [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].iter().all(|(dx, dy)| inside(&p.outline, h.x + dx, h.y + dy))
                && !p.cutouts.iter().any(|c| h.x + r > c.x && h.x - r < c.x + c.w && h.y + r > c.y && h.y - r < c.y + c.h)
        }
        Some(e) => {
            let (along, at, len, edge) = match e {
                0 => (h.x, h.y, p.w, 0.0),
                1 => (h.y, h.x, p.h, p.w),
                2 => (h.x, h.y, p.w, p.h),
                _ => (h.y, h.x, p.h, 0.0),
            };
            (at - edge).abs() < 0.5 && along >= h.dia / 2.0 && along <= len - h.dia / 2.0 && h.dia < p.thickness
        }
    }
}

pub fn apply(project: &Project, out: &mut Build) {
    for (id, e) in &project.part_edits {
        let Some(p) = out.parts.iter_mut().find(|p| &p.id == id) else {
            // a deleted cabinet's edits are dropped by the editor; a cabinet
            // that changed so the part is gone is worth knowing about
            if project.cabinets.iter().any(|c| c.id == cabinet_of(id)) && !e.is_empty() {
                out.warnings.push(format!("{id}: edited part no longer exists (edits not applied)"));
            }
            continue;
        };
        let who = format!("{} ({})", p.name.to_lowercase(), p.cabinet);
        if let Some(m) = &e.material {
            match project.materials.iter().find(|x| &x.id == m) {
                Some(x) if (x.thickness - p.thickness).abs() < 0.01 => p.material = m.clone(),
                Some(x) => out
                    .warnings
                    .push(format!("{who}: {} is {} mm, the part is {} mm (material not changed)", x.name, x.thickness, p.thickness)),
                None => out.warnings.push(format!("{who}: material {m} not found")),
            }
        }
        if let Some(g) = e.grain_y {
            p.grain_y = g;
        }
        if let Some(b) = e.edges {
            if p.band_segs.is_empty() {
                p.edges = b;
            }
        }
        if let Some(s) = &e.band_segs {
            let n = p.outline.len();
            p.band_segs = s.iter().copied().filter(|&i| i < n).collect();
        }
        let mut missing = 0;
        for r in &e.remove {
            match p.holes.iter().position(|h| same(h, r)) {
                Some(i) => {
                    p.holes.remove(i);
                }
                None => missing += 1,
            }
        }
        if missing > 0 {
            out.warnings.push(format!("{who}: {missing} removed bore(s) no longer match (part changed)"));
        }
        for h in &e.add {
            if !fits(p, h) {
                out.warnings.push(format!("{who}: added bore Ø{} at {}, {} is off the part", h.dia, h.x, h.y));
            } else if h.edge.is_none() && h.depth >= p.thickness {
                out.warnings.push(format!(
                    "{who}: added bore Ø{} at {}, {} is {} deep in a {} mm part (use 0 for through)",
                    h.dia, h.x, h.y, h.depth, p.thickness
                ));
            }
            p.holes.push(h.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compute, sample_kitchen};

    fn hole(x: f64, y: f64, dia: f64, depth: f64, edge: Option<u8>) -> Hole {
        Hole { x, y, dia, depth, edge }
    }

    #[test]
    fn edits_apply_and_reach_exports() {
        let mut p = sample_kitchen();
        let r0 = compute(&p);
        let side = r0.build.parts.iter().find(|q| q.id == "b2-side-l").unwrap().clone();
        let gone = side.holes[0].clone();
        let mut e = PartEdit { edges: Some([true, true, true, true]), grain_y: Some(!side.grain_y), ..Default::default() };
        e.remove.push(gone.clone());
        e.add.push(hole(100.0, 400.0, 8.0, 12.0, None));
        e.add.push(hole(side.w / 2.0, 0.0, 8.0, 30.0, Some(0)));
        p.part_edits.insert(side.id.clone(), e);
        let r = compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        let q = r.build.parts.iter().find(|q| q.id == "b2-side-l").unwrap();
        assert_eq!(q.edges, [true; 4]);
        assert_eq!(q.grain_y, !side.grain_y);
        assert_eq!(q.holes.len(), side.holes.len() + 1);
        assert!(!q.holes.iter().any(|h| same(h, &gone)));
        // the CNC program drills the new bore
        let cut = crate::export::machined_parts(&p, &r.build);
        let i = r.build.parts.iter().position(|q| q.id == "b2-side-l").unwrap();
        let cut0 = crate::export::machined_parts(&sample_kitchen(), &r0.build);
        let circles = |s: String| s.matches("\nCIRCLE\n").count();
        // one face bore out, one in: same count; the edge bore is drawn on the edge, not as a circle
        assert_eq!(
            circles(crate::export::dxf::part_dxf(&cut[i])),
            circles(crate::export::dxf::part_dxf(&cut0[i])) - usize::from(gone.edge.is_some())
        );
    }

    #[test]
    fn stale_edits_warn() {
        let mut p = sample_kitchen();
        let mut e = PartEdit::default();
        e.remove.push(hole(1.0, 1.0, 5.0, 12.0, None));
        e.add.push(hole(-50.0, 10.0, 8.0, 12.0, None));
        e.add.push(hole(100.0, 100.0, 8.0, 30.0, None));
        e.material = Some("upstand".into());
        p.part_edits.insert("b2-side-l".into(), e.clone());
        p.part_edits.insert("b2-nonesuch".into(), e.clone());
        p.part_edits.insert("zz9-side-l".into(), e);
        let w = compute(&p).build.warnings;
        assert_eq!(w.len(), 5, "{w:?}");
        assert!(w.iter().any(|s| s.contains("no longer match")));
        assert!(w.iter().any(|s| s.contains("off the part")));
        assert!(w.iter().any(|s| s.contains("30 deep")));
        assert!(w.iter().any(|s| s.contains("material not changed")));
        assert!(w.iter().any(|s| s.contains("b2-nonesuch")));
    }
}
