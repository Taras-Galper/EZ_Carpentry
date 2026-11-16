//! Costing and customer quote.
//!
//! Cost = sheets used × sheet price + edge band + hardware + labour
//! (CNC time per sheet, assembly and installation per cabinet).
//! Each cabinet and each worktop is priced at its share of that cost
//! plus markup; VAT is added on the net total.

use crate::model::{CabinetKind, Project};
use crate::parts::Role;
use crate::Report;
use serde::Serialize;
use std::fmt::Write;

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub group: String,
    pub item: String,
    pub qty: f64,
    pub unit: String,
    pub unit_price: f64,
    pub total: f64,
}

/// A priced line on the customer quote: a cabinet or a worktop.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost: f64,
    pub price: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Quote {
    pub currency: String,
    pub lines: Vec<Line>,
    pub items: Vec<Item>,
    pub materials: f64,
    pub edge_band: f64,
    pub hardware: f64,
    pub labor: f64,
    pub cost: f64,
    pub markup: f64,
    pub net: f64,
    pub vat: f64,
    pub total: f64,
}

fn money(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn band_m(p: &crate::parts::Part) -> f64 {
    let e = p.edges;
    ((if e[0] { p.w } else { 0.0 })
        + (if e[2] { p.w } else { 0.0 })
        + (if e[1] { p.h } else { 0.0 })
        + (if e[3] { p.h } else { 0.0 }))
        / 1000.0
}

pub fn quote(project: &Project, report: &Report) -> Quote {
    let pr = &project.pricing;
    let b = &report.build;
    let mut lines: Vec<Line> = vec![];
    let n_cab = project.cabinets.len();
    let cab_idx = |id: &str| project.cabinets.iter().position(|c| c.id == id);
    // Cost carried by each part (material share, band, CNC share) and by
    // each cabinet (hardware, assembly, installation).
    let mut part_cost = vec![0.0f64; b.parts.len()];
    let mut cab_cost = vec![0.0f64; n_cab];

    // Materials: whole sheets, cost shared by part area within each material.
    let mut mats: Vec<String> = vec![];
    for s in &report.nesting.sheets {
        if !mats.contains(&s.material) {
            mats.push(s.material.clone());
        }
    }
    let total_area: f64 = b.parts.iter().map(|p| p.w * p.h).sum::<f64>().max(1.0);
    let mut materials = 0.0;
    for mid in &mats {
        let m = project.material(mid);
        let n = report.nesting.sheets.iter().filter(|s| &s.material == mid).count() as f64;
        let t = n * m.price;
        materials += t;
        lines.push(Line {
            group: "Material".into(),
            item: m.name.clone(),
            qty: n,
            unit: "sheet".into(),
            unit_price: m.price,
            total: money(t),
        });
        let area: f64 = b.parts.iter().filter(|p| &p.material == mid).map(|p| p.w * p.h).sum::<f64>().max(1.0);
        for (i, p) in b.parts.iter().enumerate().filter(|(_, p)| &p.material == mid) {
            part_cost[i] += t * p.w * p.h / area;
        }
    }

    // Edge band.
    let band: f64 = b.parts.iter().map(band_m).sum();
    let edge_band = band * pr.edge_band;
    if band > 0.0 {
        lines.push(Line {
            group: "Edge band".into(),
            item: "Edge band".into(),
            qty: (band * 10.0).round() / 10.0,
            unit: "m".into(),
            unit_price: pr.edge_band,
            total: money(edge_band),
        });
        for (i, p) in b.parts.iter().enumerate() {
            part_cost[i] += band_m(p) * pr.edge_band;
        }
    }

    // Hardware.
    let mut hw: Vec<(String, u32)> = vec![];
    for h in &b.hardware {
        match hw.iter_mut().find(|x| x.0 == h.name) {
            Some(x) => x.1 += h.qty,
            None => hw.push((h.name.clone(), h.qty)),
        }
        if let Some(i) = cab_idx(&h.cabinet) {
            cab_cost[i] += h.qty as f64 * pr.hardware_price(&h.name);
        }
    }
    let mut hardware = 0.0;
    for (name, q) in hw {
        let u = pr.hardware_price(&name);
        hardware += u * q as f64;
        lines.push(Line {
            group: "Hardware".into(),
            item: name,
            qty: q as f64,
            unit: "pc".into(),
            unit_price: u,
            total: money(u * q as f64),
        });
    }

    // Labour.
    let sheets = report.nesting.sheets.len() as f64;
    let cnc_h = sheets * pr.cnc_minutes_per_sheet / 60.0;
    // Assembly for real cabinets; installation for everything except empty appliance spaces.
    let assembles = |c: &crate::model::Cabinet| c.kind.is_carcass();
    let installs = |c: &crate::model::Cabinet| c.kind != CabinetKind::Appliance;
    let asm_h = project.cabinets.iter().filter(|c| assembles(c)).count() as f64 * pr.assembly_hours;
    let inst_h = project.cabinets.iter().filter(|c| installs(c)).count() as f64 * pr.install_hours;
    let mut labor = 0.0;
    for (item, hrs) in [("CNC / cutting", cnc_h), ("Assembly", asm_h), ("Installation", inst_h)] {
        if hrs <= 0.0 {
            continue;
        }
        labor += hrs * pr.labor_rate;
        lines.push(Line {
            group: "Labour".into(),
            item: item.into(),
            qty: (hrs * 100.0).round() / 100.0,
            unit: "h".into(),
            unit_price: pr.labor_rate,
            total: money(hrs * pr.labor_rate),
        });
    }
    for (i, p) in b.parts.iter().enumerate() {
        part_cost[i] += cnc_h * pr.labor_rate * p.w * p.h / total_area;
    }
    for (c, cab) in cab_cost.iter_mut().zip(&project.cabinets) {
        let hrs = if assembles(cab) { pr.assembly_hours } else { 0.0 } + if installs(cab) { pr.install_hours } else { 0.0 };
        *c += hrs * pr.labor_rate;
    }
    for (i, p) in b.parts.iter().enumerate() {
        if p.role != Role::Worktop {
            if let Some(ci) = cab_idx(&p.cabinet) {
                cab_cost[ci] += part_cost[i];
            }
        }
    }

    let cost = materials + edge_band + hardware + labor;
    let k = 1.0 + pr.markup_pct / 100.0;
    let mut items: Vec<Item> = project
        .cabinets
        .iter()
        .zip(cab_cost)
        .map(|(c, cost)| Item {
            id: c.id.clone(),
            name: c.name.clone(),
            description: describe(project, report, &c.id),
            cost: money(cost),
            price: money(cost * k),
        })
        .filter(|it| it.cost > 0.0)
        .collect();
    for (i, p) in b.parts.iter().enumerate().filter(|(_, p)| p.role == Role::Worktop) {
        let mut d = format!("{}, {:.0} × {:.0}", project.material(&p.material).name, p.w, p.h);
        for c in &p.cutouts {
            d.push_str(match c.kind {
                crate::model::CutoutKind::Sink => ", sink cut-out",
                crate::model::CutoutKind::Hob => ", hob cut-out",
                crate::model::CutoutKind::None => "",
            });
        }
        items.push(Item {
            id: p.id.clone(),
            name: "Worktop".into(),
            description: d,
            cost: money(part_cost[i]),
            price: money(part_cost[i] * k),
        });
    }
    // Net is the sum of the rounded item prices, so the quote adds up.
    let net = money(items.iter().map(|c| c.price).sum());
    let vat = money(net * pr.vat_pct / 100.0);
    Quote {
        currency: pr.currency.clone(),
        lines,
        items,
        materials: money(materials),
        edge_band: money(edge_band),
        hardware: money(hardware),
        labor: money(labor),
        cost: money(cost),
        markup: money(net - cost),
        net,
        vat,
        total: money(net + vat),
    }
}

/// One-line customer description of a cabinet.
pub fn describe(project: &Project, report: &Report, id: &str) -> String {
    let Some(c) = project.cabinets.iter().find(|c| c.id == id) else { return String::new() };
    match c.kind {
        CabinetKind::Filler => return format!("Filler {:.0}×{:.0}, {}", c.width, c.height, project.material(&c.front_material).name),
        CabinetKind::Dishwasher => {
            return format!("Integrated dishwasher panel {:.0} wide, {}", c.width, project.material(&c.front_material).name)
        }
        CabinetKind::Appliance => return format!("Appliance space {:.0}×{:.0}×{:.0}", c.width, c.height, c.depth),
        _ => {}
    }
    let kind = match (c.kind, c.blind_side) {
        (CabinetKind::Base, crate::model::BlindSide::None) => "Base cabinet",
        (CabinetKind::Base, _) => "Blind corner base cabinet",
        (CabinetKind::Wall, crate::model::BlindSide::None) => "Wall cabinet",
        (CabinetKind::Wall, _) => "Blind corner wall cabinet",
        _ => "Tall cabinet",
    };
    let mut bits = vec![format!("{kind} {:.0}×{:.0}×{:.0}", c.width, c.height, c.depth)];
    let fronts = report.build.parts.iter().filter(|p| p.cabinet == c.id && p.role == Role::Front);
    // Slab door ids end in "-doorN"; a Shaker door counts once via its left stile.
    let doors = fronts
        .clone()
        .filter(|p| {
            let tail = p.id.strip_prefix(&format!("{}-", c.id)).unwrap_or("");
            tail.starts_with("door") && (!tail.contains('-') || tail.ends_with("-stl"))
        })
        .count();
    let drawers = report.build.parts.iter().filter(|p| p.cabinet == c.id && p.name.ends_with("side") && p.role == Role::DrawerBox).count() / 2;
    if doors > 0 {
        bits.push(format!("{} door{}", doors, if doors > 1 { "s" } else { "" }));
    }
    if drawers > 0 {
        bits.push(format!("{} drawer{}", drawers, if drawers > 1 { "s" } else { "" }));
    }
    let shelves = report.build.parts.iter().filter(|p| p.cabinet == c.id && p.role == Role::Shelf).count();
    if shelves > 0 {
        bits.push(format!("{} shel{}", shelves, if shelves > 1 { "ves" } else { "f" }));
    }
    if fronts.count() > 0 {
        bits.push(format!(
            "{} fronts, {}",
            match c.door_style {
                crate::model::DoorStyle::Shaker => "Shaker",
                crate::model::DoorStyle::Slab => "slab",
            },
            project.material(&c.front_material).name
        ));
    }
    bits.join(", ")
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.into()
    }
}

pub fn quote_csv(q: &Quote) -> String {
    let mut o = String::from("Section,Item,Qty,Unit,Unit price,Total\n");
    for l in &q.lines {
        let _ = writeln!(o, "{},{},{},{},{:.2},{:.2}", csv(&l.group), csv(&l.item), l.qty, l.unit, l.unit_price, l.total);
    }
    let _ = writeln!(o, "Cost,Total cost,,,,{:.2}", q.cost);
    let _ = writeln!(o, "Cost,Markup,,,,{:.2}", q.markup);
    for c in &q.items {
        let _ = writeln!(o, "Item,{},1,pc,{:.2},{:.2}", csv(&format!("{} {} — {}", c.id, c.name, c.description)), c.price, c.price);
    }
    let _ = writeln!(o, "Total,Net,,,,{:.2}", q.net);
    let _ = writeln!(o, "Total,VAT,,,,{:.2}", q.vat);
    let _ = writeln!(o, "Total,Total,,,,{:.2}", q.total);
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn fmt_money(cur: &str, v: f64) -> String {
    let neg = v < 0.0;
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut g = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            g.push(',');
        }
        g.push(ch);
    }
    format!("{}{}{}.{}", if neg { "-" } else { "" }, cur, g, frac)
}

/// Printable customer quote (one self-contained HTML page).
pub fn quote_html(project: &Project, q: &Quote, vat_pct: f64) -> String {
    let m = |v: f64| fmt_money(&q.currency, v);
    let mut rows = String::new();
    for (i, c) in q.items.iter().enumerate() {
        let _ = write!(
            rows,
            "<tr><td>{}</td><td><b>{}</b><br><span>{}</span></td><td class=n>{}</td></tr>",
            i + 1,
            esc(&c.name),
            esc(&c.description),
            m(c.price)
        );
    }
    format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>Quote — {name}</title>
<style>
body{{font:14px/1.45 system-ui,-apple-system,"Segoe UI",sans-serif;color:#1f1d1a;max-width:820px;margin:40px auto;padding:0 24px}}
h1{{font-size:24px;margin:0 0 4px}}.sub{{color:#7a746a;margin-bottom:28px}}
table{{width:100%;border-collapse:collapse}}td,th{{padding:10px 8px;border-bottom:1px solid #e3dfd6;vertical-align:top;text-align:left}}
th{{color:#7a746a;font-weight:600;font-size:12px;text-transform:uppercase;letter-spacing:.05em}}
td span{{color:#7a746a;font-size:13px}}.n{{text-align:right;white-space:nowrap;font-variant-numeric:tabular-nums}}
tfoot td{{border:none;padding:6px 8px}}tfoot tr.t td{{font-size:18px;font-weight:700;border-top:2px solid #1f1d1a;padding-top:10px}}
@media print{{body{{margin:0}}}}
</style></head><body>
<h1>{name}</h1><div class="sub">Quote · {n} items</div>
<table><thead><tr><th>#</th><th>Item</th><th class=n>Price</th></tr></thead><tbody>{rows}</tbody>
<tfoot><tr><td></td><td class=n>Subtotal</td><td class=n>{net}</td></tr>
<tr><td></td><td class=n>VAT {vat_pct}%</td><td class=n>{vat}</td></tr>
<tr class=t><td></td><td class=n>Total</td><td class=n>{total}</td></tr></tfoot></table>
</body></html>"#,
        name = esc(&project.name),
        n = q.items.len(),
        rows = rows,
        net = m(q.net),
        vat = m(q.vat),
        total = m(q.total),
        vat_pct = vat_pct,
    )
}
