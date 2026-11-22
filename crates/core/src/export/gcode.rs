//! G-code for a 3-axis CNC router, one program per nested sheet.
//!
//! Origin: sheet lower-left corner, Z0 = top of the material, units mm.
//! Order per sheet: vertical drilling (per tool) -> pockets and grooves ->
//! inner cut-outs -> part profiles, smallest parts first, in depth passes,
//! with holding tabs on small parts. Edge (horizontal) bores and
//! opposite-face pockets cannot be cut from above on the sheet; they are
//! listed in the header, and face-2 pockets get their own per-part program.
//!
//! The output is plain ISO (Fanuc-style) G-code: G21 G90, M6 tool
//! changes, M3/M5 spindle. Check it against your post-processor and
//! simulate before cutting.

use crate::model::Project;
use crate::nest::{Placement, Sheet};
use crate::parts::{Cutout, Part, Pocket};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tool {
    pub number: u32,
    pub name: String,
    pub dia: f64,
    /// Cutting feed, mm/min.
    pub feed: f64,
    /// Plunge feed, mm/min.
    pub plunge: f64,
    pub rpm: f64,
}

impl Default for Tool {
    fn default() -> Self {
        Self { number: 1, name: "Router Ø8 compression".into(), dia: 8.0, feed: 9000.0, plunge: 2500.0, rpm: 18000.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cnc {
    /// Profiles, cut-outs and large pockets.
    pub router: Tool,
    /// Narrow grooves (back panel, drawer bottoms).
    pub groove_router: Tool,
    /// Vertical drills, matched to hole diameters.
    pub drills: Vec<Tool>,
    /// Depth per profile pass.
    pub pass_depth: f64,
    /// How far through-cuts go below the material (into the spoilboard).
    pub through: f64,
    pub safe_z: f64,
    /// Parts smaller than this (m²) or narrower than `tab_min_side` get tabs.
    pub tab_area: f64,
    pub tab_min_side: f64,
    pub tab_length: f64,
    /// Material left under each tab.
    pub tab_height: f64,
}

impl Default for Cnc {
    fn default() -> Self {
        let drill = |n: u32, d: f64, rpm: f64| Tool {
            number: n,
            name: format!("Drill Ø{d}"),
            dia: d,
            feed: 0.0,
            plunge: 2000.0,
            rpm,
        };
        Self {
            router: Tool::default(),
            groove_router: Tool { number: 2, name: "Router Ø6 straight".into(), dia: 6.0, feed: 6000.0, plunge: 1500.0, rpm: 18000.0 },
            drills: vec![
                drill(3, 5.0, 6000.0),
                drill(4, 7.0, 6000.0),
                drill(5, 8.0, 6000.0),
                Tool { number: 6, name: "Hinge bit Ø35".into(), dia: 35.0, feed: 0.0, plunge: 1500.0, rpm: 4500.0 },
            ],
            pass_depth: 10.0,
            through: 0.3,
            safe_z: 15.0,
            tab_area: 0.12,
            tab_min_side: 150.0,
            tab_length: 30.0,
            tab_height: 3.0,
        }
    }
}

struct G {
    o: String,
    tool: Option<u32>,
    safe: f64,
}

impl G {
    fn new(title: &str, safe: f64) -> Self {
        let mut o = String::new();
        let _ = writeln!(o, "%");
        for line in title.lines() {
            let _ = writeln!(o, "({})", clean(line));
        }
        let _ = writeln!(o, "G21 G90 G17 G40 G49 G80");
        Self { o, tool: None, safe }
    }
    fn tool(&mut self, t: &Tool) {
        if self.tool == Some(t.number) {
            return;
        }
        if self.tool.is_some() {
            let _ = writeln!(self.o, "M5");
        }
        let _ = writeln!(self.o, "G0 Z{}", f(self.safe + 35.0));
        let _ = writeln!(self.o, "(T{} {})", t.number, clean(&t.name));
        let _ = writeln!(self.o, "T{} M6", t.number);
        let _ = writeln!(self.o, "S{} M3", t.rpm.round());
        let _ = writeln!(self.o, "G0 Z{}", f(self.safe));
        self.tool = Some(t.number);
    }
    fn comment(&mut self, s: &str) {
        let _ = writeln!(self.o, "({})", clean(s));
    }
    fn rapid_to(&mut self, x: f64, y: f64) {
        let _ = writeln!(self.o, "G0 Z{}", f(self.safe));
        let _ = writeln!(self.o, "G0 X{} Y{}", f(x), f(y));
    }
    fn plunge(&mut self, z: f64, feed: f64) {
        let _ = writeln!(self.o, "G1 Z{} F{}", f(z), feed.round());
    }
    fn line(&mut self, x: f64, y: f64, z: Option<f64>, feed: f64) {
        match z {
            Some(z) => {
                let _ = writeln!(self.o, "G1 X{} Y{} Z{} F{}", f(x), f(y), f(z), feed.round());
            }
            None => {
                let _ = writeln!(self.o, "G1 X{} Y{} F{}", f(x), f(y), feed.round());
            }
        }
    }
    fn retract(&mut self) {
        let _ = writeln!(self.o, "G0 Z{}", f(self.safe));
    }
    fn end(mut self) -> String {
        let _ = writeln!(self.o, "M5");
        let _ = writeln!(self.o, "G0 Z{}", f(self.safe + 35.0));
        let _ = writeln!(self.o, "M30");
        let _ = writeln!(self.o, "%");
        self.o
    }
}

fn f(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c == '(' || c == ')' || !c.is_ascii() { ' ' } else { c }).collect()
}

/// Offset a closed polygon. Positive `d` grows a counter-clockwise polygon.
pub fn offset(poly: &[(f64, f64)], d: f64) -> Vec<(f64, f64)> {
    let n = poly.len();
    let area2: f64 = (0..n).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        a.0 * b.1 - b.0 * a.1
    }).sum();
    let d = if area2 >= 0.0 { d } else { -d };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p0 = poly[(i + n - 1) % n];
        let p1 = poly[i];
        let p2 = poly[(i + 1) % n];
        let n1 = norm((p1.1 - p0.1, -(p1.0 - p0.0)));
        let n2 = norm((p2.1 - p1.1, -(p2.0 - p1.0)));
        // miter direction
        let (mx, my) = (n1.0 + n2.0, n1.1 + n2.1);
        let dot = n1.0 * n2.0 + n1.1 * n2.1;
        let k = d / (1.0 + dot).max(0.05);
        out.push((p1.0 + mx * k, p1.1 + my * k));
    }
    out
}

fn norm(v: (f64, f64)) -> (f64, f64) {
    let l = (v.0 * v.0 + v.1 * v.1).sqrt().max(1e-12);
    (v.0 / l, v.1 / l)
}

/// Cut a closed path from `z_top` to `z_bottom` in passes; tabs (if any)
/// on the last pass, given as distances along the path.
fn cut_closed(g: &mut G, t: &Tool, path: &[(f64, f64)], z_bottom: f64, pass: f64, tabs: &[(f64, f64)], tab_z: f64) {
    if path.len() < 2 {
        return;
    }
    let passes = ((-z_bottom) / pass).ceil().max(1.0) as usize;
    let (sx, sy) = path[0];
    g.rapid_to(sx, sy);
    for k in 1..=passes {
        let z = if k == passes { z_bottom } else { -(pass * k as f64) };
        g.plunge(z, t.plunge);
        let last = k == passes && !tabs.is_empty();
        let mut dist = 0.0;
        for i in 1..=path.len() {
            let (x0, y0) = path[i - 1];
            let (x1, y1) = path[i % path.len()];
            let seg = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            if last {
                // split this segment at tab boundaries
                let mut cuts: Vec<(f64, f64)> = vec![]; // (position along segment, z after)
                for &(a, b) in tabs {
                    for (p, zz) in [(a, tab_z.max(z)), (b, z)] {
                        if p > dist && p < dist + seg {
                            cuts.push((p - dist, zz));
                        }
                    }
                }
                cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (p, zz) in cuts {
                    let r = p / seg;
                    g.line(x0 + (x1 - x0) * r, y0 + (y1 - y0) * r, None, t.feed);
                    g.plunge(zz, t.plunge);
                }
            }
            g.line(x1, y1, None, t.feed);
            dist += seg;
        }
    }
    g.retract();
}

/// Clear a rectangular pocket (in sheet coordinates, axis-aligned) to `depth`.
fn pocket_rect(g: &mut G, t: &Tool, x0: f64, y0: f64, x1: f64, y1: f64, depth: f64, pass: f64) {
    let r = t.dia / 2.0;
    let (ax, ay, bx, by) = (x0 + r, y0 + r, x1 - r, y1 - r);
    if bx < ax - 1e-6 || by < ay - 1e-6 {
        g.comment(&format!("SKIPPED pocket {:.1}x{:.1}: narrower than tool T{}", x1 - x0, y1 - y0, t.number));
        return;
    }
    let passes = (depth / pass).ceil().max(1.0) as usize;
    let step = t.dia * 0.6;
    // zigzag along the longer side
    let along_x = (bx - ax) >= (by - ay);
    for k in 1..=passes {
        let z = -(depth * k as f64 / passes as f64);
        let mut lines = vec![];
        if along_x {
            let mut y = ay;
            loop {
                lines.push(((ax, y), (bx, y)));
                if y >= by - 1e-6 { break; }
                y = (y + step).min(by);
            }
        } else {
            let mut x = ax;
            loop {
                lines.push(((x, ay), (x, by)));
                if x >= bx - 1e-6 { break; }
                x = (x + step).min(bx);
            }
        }
        g.rapid_to(lines[0].0 .0, lines[0].0 .1);
        g.plunge(z, t.plunge);
        for (i, (a, b)) in lines.iter().enumerate() {
            let (s, e) = if i % 2 == 0 { (a, b) } else { (b, a) };
            g.line(s.0, s.1, None, t.feed);
            g.line(e.0, e.1, None, t.feed);
        }
        // finishing pass round the walls
        g.line(ax, ay, None, t.feed);
        g.line(bx, ay, None, t.feed);
        g.line(bx, by, None, t.feed);
        g.line(ax, by, None, t.feed);
        g.line(ax, ay, None, t.feed);
        g.retract();
    }
}

fn place(pl: &Placement, p: &Part, x: f64, y: f64) -> (f64, f64) {
    pl.map(p.w, x, y)
}

/// Pocket in sheet coordinates. Sides that run out through the part's
/// edge are pushed out by the tool radius so the cut reaches the edge
/// (a groove that must be open at the end, e.g. for a back panel).
fn pocket_sheet_rect(pl: &Placement, p: &Part, k: &Pocket, tool_r: f64) -> (f64, f64, f64, f64) {
    let (mut x0, mut y0, mut x1, mut y1) = (k.x, k.y, k.x + k.w, k.y + k.h);
    if x0 <= 1e-6 { x0 -= tool_r; }
    if y0 <= 1e-6 { y0 -= tool_r; }
    if x1 >= p.w - 1e-6 { x1 += tool_r; }
    if y1 >= p.h - 1e-6 { y1 += tool_r; }
    let (a, b) = place(pl, p, x0, y0);
    let (c, d) = place(pl, p, x1, y1);
    (a.min(c), b.min(d), a.max(c), b.max(d))
}

/// Tool for a vertical hole: exact drill, else helical with a smaller router.
fn drill_for<'a>(cnc: &'a Cnc, dia: f64) -> Option<&'a Tool> {
    cnc.drills.iter().find(|t| (t.dia - dia).abs() < 0.05)
}

fn helix(g: &mut G, t: &Tool, x: f64, y: f64, dia: f64, depth: f64) {
    let r = (dia - t.dia) / 2.0;
    g.rapid_to(x + r, y);
    g.plunge(0.0, t.plunge);
    let turns = (depth / (t.dia * 0.5)).ceil().max(1.0) as usize;
    for k in 1..=turns {
        let z = -(depth * k as f64 / turns as f64);
        let _ = writeln!(g.o, "G3 X{} Y{} Z{} I{} J0 F{}", f(x + r), f(y), f(z), f(-r), t.feed.round());
    }
    let _ = writeln!(g.o, "G3 X{} Y{} I{} J0 F{}", f(x + r), f(y), f(-r), t.feed.round());
    g.retract();
}

fn needs_tabs(cnc: &Cnc, p: &Part) -> bool {
    p.w * p.h / 1e6 < cnc.tab_area || p.w.min(p.h) < cnc.tab_min_side
}

/// Tab positions (start, end distance along the path).
fn tabs_for(path: &[(f64, f64)], cnc: &Cnc, small: bool) -> Vec<(f64, f64)> {
    let len: f64 = (0..path.len())
        .map(|i| {
            let (a, b) = (path[i], path[(i + 1) % path.len()]);
            ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
        })
        .sum();
    if !small || len < 4.0 * cnc.tab_length {
        return vec![];
    }
    let n = if len < 800.0 { 2 } else { 4 };
    (0..n)
        .map(|k| {
            let c = len * (k as f64 + 0.5) / n as f64;
            (c - cnc.tab_length / 2.0, c + cnc.tab_length / 2.0)
        })
        .collect()
}

/// G-code program for one nested sheet.
pub fn sheet_program(project: &Project, sheet: &Sheet, parts: &[Part], index: usize) -> String {
    let cnc = &project.cnc;
    let m = project.material(&sheet.material);
    let th = m.thickness;
    let z_through = -(th + cnc.through);
    let edge_bores: usize = sheet.placements.iter().map(|pl| parts[pl.part].holes.iter().filter(|h| h.edge.is_some()).count()).sum();
    let face2: Vec<&str> = sheet
        .placements
        .iter()
        .filter(|pl| parts[pl.part].pockets.iter().any(|k| k.face == 2))
        .map(|pl| parts[pl.part].id.as_str())
        .collect();
    let mut title = format!(
        "EZ Carpentry - {} - sheet {} - {} {}x{}x{}\nOrigin: sheet lower-left corner, Z0 = top of material",
        project.name, index + 1, m.name, sheet.length, sheet.width, th
    );
    if edge_bores > 0 {
        title.push_str(&format!("\nNOT IN THIS PROGRAM: {edge_bores} edge bores, drill on the edge borer (see part DXFs, HDRILL layers)"));
    }
    if !face2.is_empty() {
        title.push_str(&format!("\nNOT IN THIS PROGRAM: face-2 pockets on {} - run face2 programs after flipping", face2.join(", ")));
    }
    let mut g = G::new(&title, cnc.safe_z);

    // 1. Vertical drilling, grouped by tool.
    let mut by_tool: Vec<(Tool, Vec<(f64, f64, f64)>)> = vec![];
    let mut helical: Vec<(f64, f64, f64, f64)> = vec![];
    for pl in &sheet.placements {
        let p = &parts[pl.part];
        for h in p.holes.iter().filter(|h| h.edge.is_none()) {
            let (x, y) = place(pl, p, h.x, h.y);
            let depth = if h.depth <= 0.0 { th + cnc.through } else { h.depth };
            match drill_for(cnc, h.dia) {
                Some(t) => match by_tool.iter_mut().find(|(tt, _)| tt.number == t.number) {
                    Some((_, v)) => v.push((x, y, depth)),
                    None => by_tool.push((t.clone(), vec![(x, y, depth)])),
                },
                None => helical.push((x, y, h.dia, depth)),
            }
        }
    }
    by_tool.sort_by_key(|(t, _)| t.number);
    for (t, holes) in &mut by_tool {
        g.tool(t);
        g.comment(&format!("Drill Ø{} x {}", t.dia, holes.len()));
        // short path: sweep in rows
        holes.sort_by(|a, b| ((a.1 / 50.0).round(), a.0).partial_cmp(&((b.1 / 50.0).round(), b.0)).unwrap_or(std::cmp::Ordering::Equal));
        for &(x, y, depth) in holes.iter() {
            g.rapid_to(x, y);
            g.plunge(-depth, t.plunge);
            g.retract();
        }
    }

    // 2. Pockets and grooves (face 1), narrow ones with the groove router.
    let mut pockets: Vec<(f64, f64, f64, f64, f64, bool)> = vec![];
    for pl in &sheet.placements {
        let p = &parts[pl.part];
        for k in p.pockets.iter().filter(|k| k.face == 1) {
            let narrow = k.w.min(k.h) < cnc.router.dia + 0.01;
            let r = if narrow { cnc.groove_router.dia } else { cnc.router.dia } / 2.0;
            let (a, b, c, d) = pocket_sheet_rect(pl, p, k, r);
            pockets.push((a, b, c, d, k.depth, narrow));
        }
    }
    for (a, b, c, d, depth, narrow) in &pockets {
        let t = if *narrow { &cnc.groove_router } else { &cnc.router };
        g.tool(t);
        g.comment(&format!("Pocket {:.1}x{:.1}x{}", c - a, d - b, depth));
        pocket_rect(&mut g, t, *a, *b, *c, *d, *depth, cnc.pass_depth);
    }

    // 3. Holes too big for any drill: helical with the router.
    for &(x, y, dia, depth) in &helical {
        let t = &cnc.router;
        if dia <= t.dia + 0.5 {
            g.comment(&format!("SKIPPED hole Ø{dia}: no drill and not larger than T{}", t.number));
            continue;
        }
        g.tool(t);
        g.comment(&format!("Helical hole Ø{dia} x {depth}"));
        helix(&mut g, t, x, y, dia, depth);
    }

    // 4. Inner cut-outs, then 5. profiles - small parts first.
    let r = cnc.router.dia / 2.0;
    let mut order: Vec<&Placement> = sheet.placements.iter().collect();
    order.sort_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)));
    for pl in &order {
        let p = &parts[pl.part];
        for c in &p.cutouts {
            g.tool(&cnc.router);
            g.comment(&format!("{} cut-out", p.id));
            let pts: Vec<(f64, f64)> = inner(c).into_iter().map(|(x, y)| place(pl, p, x, y)).collect();
            let path = offset(&pts, -r);
            cut_closed(&mut g, &cnc.router, &path, z_through, cnc.pass_depth, &[], 0.0);
        }
    }
    for pl in &order {
        let p = &parts[pl.part];
        g.tool(&cnc.router);
        g.comment(&format!("Profile {} {} {}x{}", p.id, p.name, p.w, p.h));
        let pts: Vec<(f64, f64)> = p.outline.iter().map(|q| place(pl, p, q[0], q[1])).collect();
        let path = offset(&pts, r);
        let tabs = tabs_for(&path, cnc, needs_tabs(cnc, p));
        let tab_z = z_through + cnc.through + cnc.tab_height;
        cut_closed(&mut g, &cnc.router, &path, z_through, cnc.pass_depth, &tabs, tab_z);
    }
    g.end()
}

fn inner(c: &Cutout) -> Vec<(f64, f64)> {
    c.points(6)
}

/// Program for the opposite-face pockets of one part, part laid face 2 up
/// with its lower-left corner at the origin (the part is mirrored in X).
pub fn face2_program(project: &Project, p: &Part) -> Option<String> {
    let cnc = &project.cnc;
    let ks: Vec<&Pocket> = p.pockets.iter().filter(|k| k.face == 2).collect();
    if ks.is_empty() {
        return None;
    }
    let title = format!(
        "EZ Carpentry - {} - {} {} face 2\nPart flipped, outside face up, lower-left corner at X0 Y0, Z0 = top\nPart {}x{}x{}",
        project.name, p.id, p.name, p.w, p.h, p.thickness
    );
    let mut g = G::new(&title, cnc.safe_z);
    for k in ks {
        // flipped about the vertical axis: x -> w - x
        let (x0, x1) = (p.w - k.x - k.w, p.w - k.x);
        let t = if k.w.min(k.h) < cnc.router.dia + 0.01 { &cnc.groove_router } else { &cnc.router };
        g.tool(t);
        g.comment(&format!("Pocket {:.1}x{:.1}x{}", k.w, k.h, k.depth));
        pocket_rect(&mut g, t, x0, k.y, x1, k.y + k.h, k.depth, cnc.pass_depth);
    }
    Some(g.end())
}
