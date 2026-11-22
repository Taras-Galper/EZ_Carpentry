//! Printable job documents: the shop packet and the part label sheets.
//! Both are self-contained HTML pages laid out for A4 (print at 100 %).

use super::{code128, drawing, quote, sheet_svg};
use crate::model::Project;
use crate::parts::{Part, Role};
use crate::Report;
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn n(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{:.0}", v) } else { format!("{:.1}", v) }
}

/// Sheet number (1-based) for every part, None when oversize.
fn sheet_of(report: &Report) -> Vec<Option<usize>> {
    let mut at = vec![None; report.build.parts.len()];
    for (i, s) in report.nesting.sheets.iter().enumerate() {
        for pl in &s.placements {
            at[pl.part] = Some(i + 1);
        }
    }
    at
}

/// Length runs with the grain (or the longer side without grain).
fn len_wid(project: &Project, p: &Part) -> (f64, f64) {
    let m = project.material(&p.material);
    let along_y = if m.grain { p.grain_y } else { p.h >= p.w };
    if along_y { (p.h, p.w) } else { (p.w, p.h) }
}

fn edges_text(p: &Part) -> String {
    ["B", "R", "T", "L"].iter().zip(p.edges).filter(|(_, e)| *e).map(|(s, _)| *s).collect()
}

/// Small outline of a part: banded edges thick, bores as dots.
fn mini(p: &Part, w: f64, h: f64) -> String {
    let s = (w / p.w).min(h / p.h);
    let (ox, oy) = ((w - p.w * s) / 2.0, (h - p.h * s) / 2.0);
    let x = |v: f64| ox + v * s;
    let y = |v: f64| h - oy - v * s;
    let mut o = String::new();
    let pts: Vec<String> = p.outline.iter().map(|q| format!("{:.2},{:.2}", x(q[0]), y(q[1]))).collect();
    let _ = write!(o, r##"<polygon points="{}" fill="#f4f1ea" stroke="#000" stroke-width="0.25"/>"##, pts.join(" "));
    let segs = [(0.0, 0.0, p.w, 0.0), (p.w, 0.0, p.w, p.h), (0.0, p.h, p.w, p.h), (0.0, 0.0, 0.0, p.h)];
    for (i, (a, b, c, d)) in segs.iter().enumerate() {
        if p.edges[i] {
            let _ = write!(o, r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="#000" stroke-width="1.1"/>"##, x(*a), y(*b), x(*c), y(*d));
        }
    }
    for k in &p.pockets {
        let _ = write!(
            o,
            r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="#000" stroke-width="0.2" stroke-dasharray="{}"/>"##,
            x(k.x), y(k.y + k.h), (k.w * s).max(0.3), (k.h * s).max(0.3), if k.face == 2 { "0.8 0.5" } else { "none" }
        );
    }
    for hle in p.holes.iter().filter(|h| h.edge.is_none()) {
        let _ = write!(o, r##"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="#000"/>"##, x(hle.x), y(hle.y), (hle.dia * s / 2.0).max(0.35));
    }
    format!(r#"<svg viewBox="0 0 {w} {h}" width="{w}mm" height="{h}mm">{o}</svg>"#)
}

const PACKET_CSS: &str = r#"
@page{size:A4;margin:12mm}
@page land{size:A4 landscape;margin:10mm}
*{box-sizing:border-box}
body{font:10pt/1.35 system-ui,-apple-system,"Segoe UI",Arial,sans-serif;color:#1f1d1a;margin:0}
h1{font-size:22pt;margin:0 0 2mm}h2{font-size:14pt;margin:0 0 4mm;border-bottom:.4mm solid #1f1d1a;padding-bottom:1.5mm}
h3{font-size:10pt;margin:5mm 0 2mm;text-transform:uppercase;letter-spacing:.05em;color:#555}
.sub{color:#666;margin-bottom:8mm}
section{page-break-before:always}section:first-child{page-break-before:auto}
section.land{page:land}
table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}
th,td{border-bottom:.2mm solid #ccc;padding:1.2mm 1.5mm;text-align:left;vertical-align:top}
th{font-size:8pt;text-transform:uppercase;color:#666;font-weight:600}
td.n,th.n{text-align:right}td.c{text-align:center}
.box{display:inline-block;width:3.5mm;height:3.5mm;border:.3mm solid #000}
.grid{display:grid;grid-template-columns:repeat(5,1fr);gap:3mm;margin-bottom:6mm}
.grid div{border:.3mm solid #ccc;padding:2mm 3mm}.grid b{display:block;font-size:16pt}
.warn{color:#b5481b}
svg{max-width:100%;height:auto}
.sheet svg{width:100%;height:auto;max-height:175mm}
.drawing svg{width:100%;height:auto;max-height:250mm}
@media screen{body{background:#eee}section{background:#fff;max-width:210mm;margin:8mm auto;padding:12mm;box-shadow:0 1px 4px rgba(0,0,0,.15)}section.land{max-width:297mm}}
"#;

/// The shop packet: cover, cabinets, cut list by material, sheet layouts,
/// shop drawings, hardware. `date` is printed on the cover when given.
pub fn packet_html(project: &Project, report: &Report, date: Option<&str>) -> String {
    let b = &report.build;
    let sheet = sheet_of(report);
    let s = &report.summary;
    let mut o = String::new();
    let _ = write!(o, r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>Shop packet — {}</title><style>{}</style></head><body>"#, esc(&project.name), PACKET_CSS);

    // ---------- cover ----------
    let _ = write!(o, "<section><h1>{}</h1><div class=sub>Shop packet{}</div>", esc(&project.name), date.map(|d| format!(" · {}", esc(d))).unwrap_or_default());
    let sheets_n: usize = s.sheets.iter().map(|x| x.1).sum();
    let _ = write!(
        o,
        "<div class=grid><div><b>{}</b>cabinets</div><div><b>{}</b>parts</div><div><b>{}</b>sheets</div><div><b>{}</b>m edge band</div><div><b>{}</b>bores</div></div>",
        s.cabinets, s.parts, sheets_n, s.edge_band_m, s.holes
    );
    if !b.warnings.is_empty() {
        let _ = write!(o, "<h3>Check before cutting</h3><ul>");
        for w in &b.warnings {
            let _ = write!(o, "<li class=warn>{}</li>", esc(w));
        }
        let _ = write!(o, "</ul>");
    }
    let _ = write!(o, "<h3>Materials</h3><table><tr><th>Material</th><th class=n>Thickness</th><th class=n>Sheet</th><th class=n>Sheets</th><th class=n>Yield</th><th class=n>Parts</th></tr>");
    let mut mats: Vec<String> = vec![];
    for sh in &report.nesting.sheets {
        if !mats.contains(&sh.material) {
            mats.push(sh.material.clone());
        }
    }
    for mid in &mats {
        let m = project.material(mid);
        let shs: Vec<_> = report.nesting.sheets.iter().filter(|x| &x.material == mid).collect();
        let y = shs.iter().map(|x| x.yield_pct).sum::<f64>() / shs.len().max(1) as f64;
        let parts = b.parts.iter().filter(|p| &p.material == mid).count();
        let _ = write!(o, "<tr><td>{}</td><td class=n>{}</td><td class=n>{}×{}</td><td class=n>{}</td><td class=n>{:.0}%</td><td class=n>{}</td></tr>", esc(&m.name), n(m.thickness), n(m.sheet_length), n(m.sheet_width), shs.len(), y, parts);
    }
    let _ = write!(o, "</table>");
    let st = &project.settings;
    let _ = write!(
        o,
        "<h3>Construction</h3><table><tr><td>Joinery</td><td>{:?}</td><td>Back</td><td>{:?}{}</td></tr><tr><td>System line</td><td>{} mm</td><td>Front gap</td><td>{} mm</td></tr><tr><td>Groove depth</td><td>{} mm</td><td>Edge band</td><td>{} mm</td></tr></table>",
        st.joinery,
        st.back_mode,
        if st.back_mode == crate::model::BackMode::Grooved { format!(", inset {} mm", n(st.back_inset)) } else { String::new() },
        n(st.system_line),
        n(st.front_gap),
        n(st.groove_depth),
        n(st.edge_band)
    );
    let mut tot: Vec<(String, u32)> = vec![];
    for h in &b.hardware {
        match tot.iter_mut().find(|t| t.0 == h.name) {
            Some(t) => t.1 += h.qty,
            None => tot.push((h.name.clone(), h.qty)),
        }
    }
    let _ = write!(o, "<h3>Hardware</h3><table><tr><th>Item</th><th class=n>Qty</th><th class=c>✓</th></tr>");
    for (name, q) in &tot {
        let _ = write!(o, "<tr><td>{}</td><td class=n>{}</td><td class=c><span class=box></span></td></tr>", esc(name), q);
    }
    let _ = write!(o, "</table></section>");

    // ---------- cabinets ----------
    let _ = write!(o, "<section><h2>Cabinets</h2><table><tr><th>Cabinet</th><th>Name</th><th>Description</th><th class=n>Parts</th></tr>");
    for c in &project.cabinets {
        let np = b.parts.iter().filter(|p| p.cabinet == c.id && p.role != Role::Worktop).count();
        let _ = write!(o, "<tr><td><b>{}</b></td><td>{}</td><td>{}</td><td class=n>{}</td></tr>", esc(&c.id), esc(&c.name), esc(&quote::describe(project, report, &c.id)), np);
    }
    for p in b.parts.iter().filter(|p| p.role == Role::Worktop) {
        let _ = write!(o, "<tr><td><b>{}</b></td><td>Worktop</td><td>{} × {} × {}, {} cut-out(s)</td><td class=n>1</td></tr>", esc(&p.id), n(p.w), n(p.h), n(p.thickness), p.cutouts.len());
    }
    let _ = write!(o, "</table></section>");

    // ---------- cut list ----------
    for mid in &mats {
        let m = project.material(mid);
        let _ = write!(o, "<section><h2>Cut list — {} ({} mm)</h2><table><tr><th class=c>✓</th><th>Part</th><th>Cabinet</th><th>Name</th><th class=n>Length</th><th class=n>Width</th><th>Edges</th><th class=n>Bores</th><th class=n>Sheet</th></tr>", esc(&m.name), n(m.thickness));
        let mut rows: Vec<(usize, &Part)> = b.parts.iter().enumerate().filter(|(_, p)| &p.material == mid).collect();
        rows.sort_by_key(|(i, _)| (sheet[*i].unwrap_or(usize::MAX), *i));
        for (i, p) in rows {
            let (l, w) = len_wid(project, p);
            let _ = write!(
                o,
                "<tr><td class=c><span class=box></span></td><td><b>{}</b></td><td>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td></tr>",
                esc(&p.id), esc(&p.cabinet), esc(&p.name), n(l), n(w), edges_text(p), p.holes.len(),
                sheet[i].map(|s| s.to_string()).unwrap_or_else(|| "oversize".into())
            );
        }
        let _ = write!(o, "</table></section>");
    }

    // ---------- edge boring (not in the sheet G-code) ----------
    let bored: Vec<&Part> = b.parts.iter().filter(|p| p.holes.iter().any(|h| h.edge.is_some())).collect();
    if !bored.is_empty() {
        let _ = write!(
            o,
            "<section><h2>Edge boring</h2><p>Horizontal bores into the panel ends, mid-thickness. Positions are measured along the edge from the part's front (the banded edge, or the edge at drawing y = 0).</p><table><tr><th class=c>✓</th><th>Part</th><th>Name</th><th>Edge</th><th>Bore</th><th>Positions</th></tr>"
        );
        for p in bored {
            for (e, label) in [(3u8, "left"), (1u8, "right"), (0u8, "bottom"), (2u8, "top")] {
                let hs: Vec<_> = p.holes.iter().filter(|h| h.edge == Some(e)).collect();
                if hs.is_empty() {
                    continue;
                }
                let mut pos: Vec<f64> = hs.iter().map(|h| if e == 1 || e == 3 { h.y } else { h.x }).collect();
                pos.sort_by(|a, b| a.total_cmp(b));
                let _ = write!(
                    o,
                    "<tr><td class=c><span class=box></span></td><td><b>{}</b></td><td>{}</td><td>{}</td><td>Ø{} × {}</td><td>{}</td></tr>",
                    esc(&p.id), esc(&p.name), label, n(hs[0].dia), n(hs[0].depth),
                    pos.iter().map(|v| n(*v)).collect::<Vec<_>>().join(" · ")
                );
            }
        }
        let _ = write!(o, "</table></section>");
    }

    // ---------- sheets ----------
    for (i, sh) in report.nesting.sheets.iter().enumerate() {
        let m = project.material(&sh.material);
        let _ = write!(o, "<section class=land><h2>Sheet {} of {} — {} · {} parts · {:.0}%</h2><div class=sheet>{}</div></section>", i + 1, report.nesting.sheets.len(), esc(&m.name), sh.placements.len(), sh.yield_pct, sheet_svg(project, sh, &b.parts));
    }

    // ---------- drawings ----------
    for c in project.cabinets.iter().filter(|c| c.kind.is_carcass()) {
        let _ = write!(o, "<section><h2>{} · {}</h2><div class=drawing>{}</div>", esc(&c.id), esc(&c.name), drawing::cabinet_svg(project, b, c));
        let hw: Vec<_> = b.hardware.iter().filter(|h| h.cabinet == c.id).collect();
        if !hw.is_empty() {
            let _ = write!(o, "<h3>Hardware</h3><table>");
            for h in hw {
                let _ = write!(o, "<tr><td>{}</td><td class=n>{}</td><td class=c><span class=box></span></td></tr>", esc(&h.name), h.qty);
            }
            let _ = write!(o, "</table>");
        }
        let _ = write!(o, "</section>");
    }
    o.push_str("</body></html>");
    o
}

const LABEL_CSS: &str = r#"
@page{size:A4;margin:0}
*{box-sizing:border-box}
body{margin:0;font:7.5pt/1.2 system-ui,-apple-system,"Segoe UI",Arial,sans-serif;color:#000}
.page{width:210mm;height:297mm;padding:4.5mm 0 0 0;display:grid;grid-template-columns:repeat(3,70mm);grid-auto-rows:36mm;page-break-after:always;overflow:hidden}
.label{padding:2mm 2.5mm;display:grid;grid-template-columns:19mm 1fr;grid-template-rows:auto 1fr auto;column-gap:2mm;overflow:hidden}
.label .id{grid-column:1/3;display:flex;justify-content:space-between;font-weight:700;font-size:10pt}
.label .id span{font-weight:600}
.label .pic{grid-row:2;display:flex;align-items:center}
.label .txt{grid-row:2;overflow:hidden}
.label .txt b{font-size:9pt}
.label .bc{grid-column:1/3}
.label .bc svg{display:block;width:100%;height:7mm}
@media screen{body{background:#eee}.page{background:#fff;margin:8mm auto;box-shadow:0 1px 4px rgba(0,0,0,.15)}.label{outline:.1mm dashed #ccc}}
"#;

/// A4 label sheets, 3 × 8 labels of 70 × 36 mm, one label per part.
pub fn labels_html(project: &Project, report: &Report) -> String {
    let b = &report.build;
    let sheet = sheet_of(report);
    let mut order: Vec<usize> = (0..b.parts.len()).collect();
    order.sort_by_key(|&i| (sheet[i].unwrap_or(usize::MAX), i));
    let mut o = String::new();
    let _ = write!(o, r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>Labels — {}</title><style>{}</style></head><body>"#, esc(&project.name), LABEL_CSS);
    for chunk in order.chunks(24) {
        o.push_str("<div class=page>");
        for &i in chunk {
            let p = &b.parts[i];
            let m = project.material(&p.material);
            let cname = project.cabinets.iter().find(|c| c.id == p.cabinet).map(|c| c.name.clone()).unwrap_or_default();
            let (l, w) = len_wid(project, p);
            let _ = write!(
                o,
                r#"<div class=label><div class=id>{}<span>S{}</span></div><div class=pic>{}</div><div class=txt><b>{} × {}</b> × {}<br>{}<br>{} · {}<br>{}</div><div class=bc><svg viewBox="0 0 100 10" preserveAspectRatio="none">{}</svg></div></div>"#,
                esc(&p.id),
                sheet[i].map(|s| s.to_string()).unwrap_or_else(|| "–".into()),
                mini(p, 18.0, 16.0),
                n(l), n(w), n(p.thickness),
                esc(&m.name),
                esc(&cname), esc(&p.name),
                [
                    (!edges_text(p).is_empty()).then(|| format!("Edges {}", edges_text(p))),
                    (!p.holes.is_empty()).then(|| format!("{} bores", p.holes.len())),
                    (!p.pockets.is_empty()).then(|| format!("{} grooves", p.pockets.len())),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
                code128::svg(&p.id, 0.0, 0.0, 100.0, 10.0)
            );
        }
        o.push_str("</div>");
    }
    o.push_str("</body></html>");
    o
}
