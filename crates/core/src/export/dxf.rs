//! DXF R12 (AC1009) ASCII writer. Opens in AlphaCAM, AutoCAD,
//! SolidWorks, LibreCAD, and most CNC nesting/CAM software.
//!
//! Layers:
//!   OUTLINE          part profile (cut through)
//!   CUTOUT           inner through profiles: sink / hob (arcs as bulges)
//!   DRILL_D{d}_Z{z}  vertical bores, diameter d, depth z (mm)
//!   DRILL_D{d}_THRU  through bores
//!   SHEET            sheet boundary (sheet files only)
//!   LABEL            part names

use crate::nest::{Placement, Sheet};
use crate::parts::{Hole, Part};
use std::fmt::Write;

pub fn hole_layer(h: &Hole) -> String {
    let d = fmt_num(h.dia);
    if h.depth <= 0.0 {
        format!("DRILL_D{d}_THRU")
    } else {
        format!("DRILL_D{d}_Z{}", fmt_num(h.depth))
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.1}", v);
    s.trim_end_matches('0').trim_end_matches('.').replace('.', "P")
}

fn layer_color(name: &str) -> i32 {
    match name {
        "OUTLINE" => 7,
        "SHEET" => 8,
        "LABEL" => 3,
        "CUTOUT" => 6,
        n if n.contains("THRU") => 1,
        n if n.starts_with("DRILL_D35") => 5,
        _ => 4,
    }
}

struct Dxf {
    body: String,
    layers: Vec<String>,
}

impl Dxf {
    fn new() -> Self {
        Self { body: String::new(), layers: vec!["0".into(), "OUTLINE".into(), "LABEL".into()] }
    }
    fn layer(&mut self, l: &str) {
        if !self.layers.iter().any(|x| x == l) {
            self.layers.push(l.into());
        }
    }
    fn poly(&mut self, layer: &str, pts: &[(f64, f64)]) {
        self.layer(layer);
        let b = &mut self.body;
        let _ = write!(b, "0\nPOLYLINE\n8\n{layer}\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n1\n");
        for (x, y) in pts {
            let _ = write!(b, "0\nVERTEX\n8\n{layer}\n10\n{x:.3}\n20\n{y:.3}\n30\n0.0\n");
        }
        let _ = write!(b, "0\nSEQEND\n8\n{layer}\n");
    }
    /// Closed polyline with per-vertex bulge (arc to the next vertex).
    fn poly_bulge(&mut self, layer: &str, pts: &[(f64, f64, f64)]) {
        self.layer(layer);
        let b = &mut self.body;
        let _ = write!(b, "0\nPOLYLINE\n8\n{layer}\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n1\n");
        for (x, y, g) in pts {
            let _ = write!(b, "0\nVERTEX\n8\n{layer}\n10\n{x:.3}\n20\n{y:.3}\n30\n0.0\n");
            if *g != 0.0 {
                let _ = write!(b, "42\n{g:.6}\n");
            }
        }
        let _ = write!(b, "0\nSEQEND\n8\n{layer}\n");
    }
    fn circle(&mut self, layer: &str, x: f64, y: f64, r: f64) {
        self.layer(layer);
        let _ = write!(self.body, "0\nCIRCLE\n8\n{layer}\n10\n{x:.3}\n20\n{y:.3}\n30\n0.0\n40\n{r:.3}\n");
    }
    fn text(&mut self, x: f64, y: f64, height: f64, rot: f64, s: &str) {
        let s: String = s.chars().map(|c| if c.is_ascii() && c != '\n' { c } else { '_' }).collect();
        let _ = write!(
            self.body,
            "0\nTEXT\n8\nLABEL\n10\n{x:.3}\n20\n{y:.3}\n30\n0.0\n40\n{height:.3}\n50\n{rot:.1}\n1\n{s}\n"
        );
    }
    fn finish(self, ext: (f64, f64)) -> String {
        let mut o = String::new();
        let _ = write!(
            o,
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n\
             9\n$EXTMIN\n10\n0.0\n20\n0.0\n30\n0.0\n9\n$EXTMAX\n10\n{:.3}\n20\n{:.3}\n30\n0.0\n0\nENDSEC\n",
            ext.0, ext.1
        );
        let _ = write!(o, "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n{}\n", self.layers.len());
        for l in &self.layers {
            let _ = write!(o, "0\nLAYER\n2\n{l}\n70\n0\n62\n{}\n6\nCONTINUOUS\n", layer_color(l));
        }
        o.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
        o.push_str(&self.body);
        o.push_str("0\nENDSEC\n0\nEOF\n");
        o
    }
}

fn draw_part(d: &mut Dxf, p: &Part, map: &dyn Fn(f64, f64) -> (f64, f64), label: bool, rot: f64) {
    let pts: Vec<(f64, f64)> = p.outline.iter().map(|q| map(q[0], q[1])).collect();
    d.poly("OUTLINE", &pts);
    for c in &p.cutouts {
        let v: Vec<(f64, f64, f64)> = c
            .contour()
            .into_iter()
            .map(|(x, y, g)| {
                let (a, b) = map(x, y);
                (a, b, g)
            })
            .collect();
        d.poly_bulge("CUTOUT", &v);
    }
    for h in &p.holes {
        let (x, y) = map(h.x, h.y);
        d.circle(&hole_layer(h), x, y, h.dia / 2.0);
    }
    if label {
        let th = (p.w.min(p.h) / 8.0).clamp(4.0, 25.0);
        let (x, y) = map(p.w * 0.08, p.h * 0.5);
        d.text(x, y, th, rot, &format!("{} {}", p.cabinet, p.name));
    }
}

/// One part in its own drawing frame, inside face up.
pub fn part_dxf(p: &Part) -> String {
    let mut d = Dxf::new();
    draw_part(&mut d, p, &|x, y| (x, y), false, 0.0);
    d.finish((p.w, p.h))
}

/// A full nested sheet.
pub fn sheet_dxf(sheet: &Sheet, parts: &[Part]) -> String {
    let mut d = Dxf::new();
    d.poly("SHEET", &[(0.0, 0.0), (sheet.length, 0.0), (sheet.length, sheet.width), (0.0, sheet.width)]);
    for pl in &sheet.placements {
        let p = &parts[pl.part];
        let pl2: Placement = pl.clone();
        let w = p.w;
        draw_part(&mut d, p, &move |x, y| pl2.map(w, x, y), true, if pl.rotated { -90.0 } else { 0.0 });
    }
    d.finish((sheet.length, sheet.width))
}
