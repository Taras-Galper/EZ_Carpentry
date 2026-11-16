//! Sheet nesting: MaxRects, best-short-side-fit, with kerf and grain.

use crate::model::Project;
use crate::parts::Part;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    /// Index into the build's parts list.
    pub part: usize,
    /// Lower-left of the part's footprint on the sheet.
    pub x: f64,
    pub y: f64,
    /// Footprint on the sheet (after rotation).
    pub w: f64,
    pub h: f64,
    /// Rotated 90°: drawing (px, py) -> sheet (x + py, y + part_w - px).
    pub rotated: bool,
}

impl Placement {
    pub fn map(&self, part_w: f64, px: f64, py: f64) -> (f64, f64) {
        if self.rotated {
            (self.x + py, self.y + part_w - px)
        } else {
            (self.x + px, self.y + py)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Sheet {
    pub material: String,
    pub length: f64,
    pub width: f64,
    pub trim: f64,
    pub placements: Vec<Placement>,
    /// Fraction of usable area covered by parts.
    pub yield_pct: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Nesting {
    pub sheets: Vec<Sheet>,
    /// Parts larger than any sheet.
    pub oversize: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
struct R {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl R {
    fn contains(&self, o: &R) -> bool {
        o.x >= self.x - 1e-6
            && o.y >= self.y - 1e-6
            && o.x + o.w <= self.x + self.w + 1e-6
            && o.y + o.h <= self.y + self.h + 1e-6
    }
    fn intersects(&self, o: &R) -> bool {
        o.x < self.x + self.w - 1e-6
            && o.x + o.w > self.x + 1e-6
            && o.y < self.y + self.h - 1e-6
            && o.y + o.h > self.y + 1e-6
    }
}

struct Bin {
    free: Vec<R>,
    placed: Vec<Placement>,
}

impl Bin {
    fn new(w: f64, h: f64) -> Self {
        Self { free: vec![R { x: 0.0, y: 0.0, w, h }], placed: vec![] }
    }

    /// Best short side fit; returns (score, rect, rotated).
    fn find(&self, w: f64, h: f64, can_unrot: bool, can_rot: bool) -> Option<(f64, R, bool)> {
        let mut best: Option<(f64, R, bool)> = None;
        for f in &self.free {
            for (rw, rh, rot, ok) in [(w, h, false, can_unrot), (h, w, true, can_rot)] {
                if ok && rw <= f.w + 1e-6 && rh <= f.h + 1e-6 {
                    let s = (f.w - rw).min(f.h - rh);
                    // Prefer bottom-left as tie-break for tidy sheets.
                    let score = s * 1e6 + f.y * 10.0 + f.x * 1e-3;
                    if best.as_ref().is_none_or(|b| score < b.0) {
                        best = Some((score, R { x: f.x, y: f.y, w: rw, h: rh }, rot));
                    }
                }
            }
        }
        best
    }

    fn place(&mut self, r: R) {
        let mut next = Vec::with_capacity(self.free.len() + 4);
        for f in self.free.drain(..) {
            if !f.intersects(&r) {
                next.push(f);
                continue;
            }
            if r.x > f.x {
                next.push(R { x: f.x, y: f.y, w: r.x - f.x, h: f.h });
            }
            if r.x + r.w < f.x + f.w {
                next.push(R { x: r.x + r.w, y: f.y, w: f.x + f.w - r.x - r.w, h: f.h });
            }
            if r.y > f.y {
                next.push(R { x: f.x, y: f.y, w: f.w, h: r.y - f.y });
            }
            if r.y + r.h < f.y + f.h {
                next.push(R { x: f.x, y: r.y + r.h, w: f.w, h: f.y + f.h - r.y - r.h });
            }
        }
        // Prune rects contained in others.
        let mut keep = vec![true; next.len()];
        for i in 0..next.len() {
            if !keep[i] {
                continue;
            }
            for j in 0..next.len() {
                if i != j && keep[j] && next[j].contains(&next[i]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        self.free = next
            .into_iter()
            .zip(keep)
            .filter_map(|(r, k)| k.then_some(r))
            .collect();
    }
}

pub fn nest(project: &Project, parts: &[Part]) -> Nesting {
    let s = &project.settings;
    let mut out = Nesting::default();
    let mut mats: Vec<String> = vec![];
    for p in parts {
        if !mats.contains(&p.material) {
            mats.push(p.material.clone());
        }
    }
    for mid in mats {
        let m = project.material(&mid);
        let k = s.kerf;
        let uw = m.sheet_length - 2.0 * s.sheet_trim + k;
        let uh = m.sheet_width - 2.0 * s.sheet_trim + k;
        let mut items: Vec<(usize, f64, f64)> = parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.material == mid)
            .map(|(i, p)| {
                let (w, h) = p.cut_size(s.edge_band);
                (i, w, h)
            })
            .collect();
        items.sort_by(|a, b| {
            let ka = a.1.max(a.2) * 1e6 + a.1 * a.2;
            let kb = b.1.max(b.2) * 1e6 + b.1 * b.2;
            kb.total_cmp(&ka)
        });
        let mut bins: Vec<Bin> = vec![];
        for (i, w, h) in items {
            // With grain, the part's grain must run along sheet length (x).
            let (can_unrot, can_rot) = if m.grain {
                (!parts[i].grain_y, parts[i].grain_y)
            } else {
                (true, true)
            };
            let (w2, h2) = (w + k, h + k);
            let mut done = false;
            for b in bins.iter_mut() {
                if let Some((_, r, rot)) = b.find(w2, h2, can_unrot, can_rot) {
                    b.place(r);
                    b.placed.push(Placement { part: i, x: r.x, y: r.y, w: r.w - k, h: r.h - k, rotated: rot });
                    done = true;
                    break;
                }
            }
            if done {
                continue;
            }
            let mut b = Bin::new(uw, uh);
            match b.find(w2, h2, can_unrot, can_rot) {
                Some((_, r, rot)) => {
                    b.place(r);
                    b.placed.push(Placement { part: i, x: r.x, y: r.y, w: r.w - k, h: r.h - k, rotated: rot });
                    bins.push(b);
                }
                None => out.oversize.push(i),
            }
        }
        let usable = (uw - k) * (uh - k);
        for b in bins {
            let area: f64 = b.placed.iter().map(|p| p.w * p.h).sum();
            let placements = b
                .placed
                .into_iter()
                .map(|mut p| {
                    p.x += s.sheet_trim;
                    p.y += s.sheet_trim;
                    p
                })
                .collect();
            out.sheets.push(Sheet {
                material: mid.clone(),
                length: m.sheet_length,
                width: m.sheet_width,
                trim: s.sheet_trim,
                placements,
                yield_pct: (area / usable * 1000.0).round() / 10.0,
            });
        }
    }
    out
}
