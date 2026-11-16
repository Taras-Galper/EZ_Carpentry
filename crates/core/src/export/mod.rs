pub mod drawing;
pub mod dxf;
pub mod zip;

use crate::model::Project;
use crate::nest::{Nesting, Sheet};
use crate::parts::{Build, Part};
use crate::Report;
use std::fmt::Write;

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One row per unique part (same cabinet, name, material, size, banding).
pub fn cutlist_csv(project: &Project, build: &Build) -> String {
    let band = project.settings.edge_band;
    let mut rows: Vec<(Vec<String>, u32)> = vec![];
    for p in &build.parts {
        let m = project.material(&p.material);
        let (cw, ch) = p.cut_size(band);
        // Length runs with the grain (or the longer side when there is none).
        let along_y = if m.grain { p.grain_y } else { ch >= cw };
        let (len, wid) = if along_y { (ch, cw) } else { (cw, ch) };
        // edges: [bottom, right, top, left]; length edges are the ones parallel to length.
        let (l1, l2, w1, w2) = if along_y {
            (p.edges[3], p.edges[1], p.edges[0], p.edges[2])
        } else {
            (p.edges[0], p.edges[2], p.edges[3], p.edges[1])
        };
        let x = |b: bool| if b { "X".to_string() } else { String::new() };
        let key = vec![
            p.cabinet.clone(),
            p.name.clone(),
            m.name.clone(),
            format!("{}", m.thickness),
            format!("{:.1}", len),
            format!("{:.1}", wid),
            x(l1),
            x(l2),
            x(w1),
            x(w2),
            if m.grain { "Y".into() } else { String::new() },
            p.holes.len().to_string(),
        ];
        if let Some(r) = rows.iter_mut().find(|r| r.0 == key) {
            r.1 += 1;
        } else {
            rows.push((key, 1));
        }
    }
    let mut o = String::from("Cabinet,Part,Material,Thickness,Length,Width,Qty,L1,L2,W1,W2,Grain,Holes\n");
    for (k, q) in rows {
        let cells: Vec<String> = k.iter().map(|c| csv_cell(c)).collect();
        let _ = writeln!(
            o,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            cells[0], cells[1], cells[2], cells[3], cells[4], cells[5], q, cells[6], cells[7], cells[8], cells[9], cells[10], cells[11]
        );
    }
    o
}

pub fn hardware_csv(project: &Project, build: &Build) -> String {
    let mut o = String::from("Cabinet,Item,Qty\n");
    for h in &build.hardware {
        let name = project
            .cabinets
            .iter()
            .find(|c| c.id == h.cabinet)
            .map(|c| format!("{} {}", c.id, c.name))
            .unwrap_or_else(|| h.cabinet.clone());
        let _ = writeln!(o, "{},{},{}", csv_cell(&name), csv_cell(&h.name), h.qty);
    }
    let mut totals: Vec<(String, u32)> = vec![];
    for h in &build.hardware {
        match totals.iter_mut().find(|t| t.0 == h.name) {
            Some(t) => t.1 += h.qty,
            None => totals.push((h.name.clone(), h.qty)),
        }
    }
    for (n, q) in totals {
        let _ = writeln!(o, "TOTAL,{},{}", csv_cell(&n), q);
    }
    o
}

pub fn sheet_svg(project: &Project, sheet: &Sheet, parts: &[Part]) -> String {
    let m = project.material(&sheet.material);
    let mut o = String::new();
    let _ = write!(
        o,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-20 -60 {} {}" width="{}mm" height="{}mm" font-family="sans-serif">"##,
        sheet.length + 40.0,
        sheet.width + 80.0,
        sheet.length + 40.0,
        sheet.width + 80.0
    );
    let _ = write!(
        o,
        r##"<text x="0" y="-20" font-size="36">{} — {}×{}×{} — {}%</text>"##,
        esc(&m.name), sheet.length, sheet.width, m.thickness, sheet.yield_pct
    );
    // Flip y so the SVG matches the DXF orientation.
    let _ = write!(o, r##"<g transform="translate(0,{}) scale(1,-1)">"##, sheet.width);
    let _ = write!(
        o,
        r##"<rect x="0" y="0" width="{}" height="{}" fill="#f4f1ea" stroke="#888" stroke-width="2"/>"##,
        sheet.length, sheet.width
    );
    for pl in &sheet.placements {
        let p = &parts[pl.part];
        let pts: Vec<String> = p
            .outline
            .iter()
            .map(|q| {
                let (x, y) = pl.map(p.w, q[0], q[1]);
                format!("{:.1},{:.1}", x, y)
            })
            .collect();
        let _ = write!(
            o,
            r##"<polygon points="{}" fill="{}" fill-opacity="0.55" stroke="#222" stroke-width="2"/>"##,
            pts.join(" "),
            esc(&m.color)
        );
        for c in &p.cutouts {
            let pts: Vec<String> = c
                .points(6)
                .into_iter()
                .map(|(x, y)| {
                    let (a, b) = pl.map(p.w, x, y);
                    format!("{:.1},{:.1}", a, b)
                })
                .collect();
            let _ = write!(
                o,
                r##"<polygon points="{}" fill="#f4f1ea" stroke="#222" stroke-width="2"/>"##,
                pts.join(" ")
            );
        }
        for h in &p.holes {
            let (x, y) = pl.map(p.w, h.x, h.y);
            let _ = write!(
                o,
                r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{}" stroke-width="1.5"/>"##,
                x, y, h.dia / 2.0, if h.depth <= 0.0 { "#c00" } else { "#06c" }
            );
        }
    }
    o.push_str("</g>");
    for pl in &sheet.placements {
        let p = &parts[pl.part];
        let fs = (pl.w.min(pl.h) / 6.0).clamp(14.0, 40.0);
        let cx = pl.x + pl.w / 2.0;
        let cy = sheet.width - (pl.y + pl.h / 2.0);
        let _ = write!(
            o,
            r##"<text x="{:.1}" y="{:.1}" font-size="{:.0}" text-anchor="middle">{} {}</text><text x="{:.1}" y="{:.1}" font-size="{:.0}" text-anchor="middle" fill="#555">{:.0}×{:.0}</text>"##,
            cx, cy - fs * 0.2, fs, esc(&p.cabinet), esc(&p.name), cx, cy + fs, fs * 0.8, p.w, p.h
        );
    }
    o.push_str("</svg>");
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// One row per physical part with its sheet and position, for label
/// printers and checking parts off at the saw or router.
pub fn labels_csv(project: &Project, report: &Report) -> String {
    let b = &report.build;
    let mut at: Vec<Option<(usize, f64, f64, bool)>> = vec![None; b.parts.len()];
    for (si, s) in report.nesting.sheets.iter().enumerate() {
        for pl in &s.placements {
            at[pl.part] = Some((si + 1, pl.x, pl.y, pl.rotated));
        }
    }
    let mut o = String::from("Label,Cabinet,Cabinet name,Part,Material,Thickness,W,H,Edges,Bores,Cut-outs,Sheet,Sheet X,Sheet Y,Rotated\n");
    for p in &b.parts {
        let m = project.material(&p.material);
        let cname = project.cabinets.iter().find(|c| c.id == p.cabinet).map(|c| c.name.clone()).unwrap_or_default();
        let edges: String = ["B", "R", "T", "L"].iter().zip(p.edges).filter(|(_, e)| *e).map(|(s, _)| *s).collect();
        let (sheet, sx, sy, rot) = match at[p.id_index(b)] {
            Some((n, x, y, r)) => (n.to_string(), format!("{x:.1}"), format!("{y:.1}"), if r { "Y" } else { "" }.to_string()),
            None => ("OVERSIZE".into(), String::new(), String::new(), String::new()),
        };
        let _ = writeln!(
            o,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_cell(&p.id), csv_cell(&p.cabinet), csv_cell(&cname), csv_cell(&p.name), csv_cell(&m.name),
            m.thickness, p.w, p.h, edges, p.holes.len(), p.cutouts.len(), sheet, sx, sy, rot
        );
    }
    o
}

/// World-space boxes as Wavefront OBJ, millimetres, Y up.
pub fn obj(project: &Project, build: &Build) -> String {
    let mut o = String::from("# Casework export, millimetres, Y up\n");
    let mut vi = 1;
    for p in &build.parts {
        let Some(c) = project.cabinets.iter().find(|c| c.id == p.cabinet) else { continue };
        let (sin, cos) = c.rotation.to_radians().sin_cos();
        let _ = writeln!(o, "o {}_{}", p.cabinet, p.name.replace(' ', "_"));
        let [x0, y0, z0] = p.box3d.pos;
        let [sx, sy, sz] = p.box3d.size;
        for i in 0..8 {
            let x = x0 + if i & 1 != 0 { sx } else { 0.0 };
            let y = y0 + if i & 2 != 0 { sy } else { 0.0 };
            let z = z0 + if i & 4 != 0 { sz } else { 0.0 };
            let wx = c.x + x * cos + z * sin;
            let wz = c.z - x * sin + z * cos;
            let _ = writeln!(o, "v {:.2} {:.2} {:.2}", wx, y + c.elevation, wz);
        }
        for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
            let _ = writeln!(o, "f {} {} {} {}", vi + f[0], vi + f[1], vi + f[2], vi + f[3]);
        }
        vi += 8;
    }
    o
}

fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Everything in one ZIP: project, reports, per-part and per-sheet DXF, SVG layouts, OBJ.
pub fn bundle(project: &Project, report: &Report) -> Vec<u8> {
    zip::store(&files(project, report))
}

/// All export files as (path, bytes).
pub fn files(project: &Project, report: &Report) -> Vec<(String, Vec<u8>)> {
    let b = &report.build;
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("project.json".into(), serde_json::to_vec_pretty(project).unwrap_or_default()),
        ("cutlist.csv".into(), cutlist_csv(project, b).into_bytes()),
        ("hardware.csv".into(), hardware_csv(project, b).into_bytes()),
        ("labels.csv".into(), labels_csv(project, report).into_bytes()),
        ("model.obj".into(), obj(project, b).into_bytes()),
    ];
    for p in &b.parts {
        files.push((format!("parts/{}.dxf", safe(&p.id)), dxf::part_dxf(p).into_bytes()));
    }
    for c in &project.cabinets {
        files.push((format!("drawings/{}.svg", safe(&c.id)), drawing::cabinet_svg(project, b, c).into_bytes()));
    }
    write_sheets(project, &report.nesting, &b.parts, &mut files);
    files
}

fn write_sheets(project: &Project, n: &Nesting, parts: &[Part], files: &mut Vec<(String, Vec<u8>)>) {
    for (i, s) in n.sheets.iter().enumerate() {
        let base = format!("sheets/{:02}_{}", i + 1, safe(&s.material));
        files.push((format!("{base}.dxf"), dxf::sheet_dxf(s, parts).into_bytes()));
        files.push((format!("{base}.svg"), sheet_svg(project, s, parts).into_bytes()));
    }
}
