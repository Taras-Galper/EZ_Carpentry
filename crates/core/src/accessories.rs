//! Bought-in cabinet fittings that do not change the cut parts: pull-outs,
//! bins, inserts, corner mechanisms, racks. Each is listed with the
//! hardware (sized to the cabinet) and checked against where it can go.

use crate::model::*;
use crate::parts::{Build, Hardware};

/// Catalogue entry: id stored on the cabinet, label for the editor.
pub struct Accessory {
    pub id: &'static str,
    pub label: &'static str,
}

pub const CATALOGUE: [Accessory; 9] = [
    Accessory { id: "cutlery", label: "Cutlery insert" },
    Accessory { id: "bin", label: "Waste bin pull-out" },
    Accessory { id: "bottle", label: "Bottle pull-out" },
    Accessory { id: "larder", label: "Larder pull-out" },
    Accessory { id: "carousel", label: "Corner carousel" },
    Accessory { id: "blind_pullout", label: "Blind corner pull-out" },
    Accessory { id: "plate_rack", label: "Plate rack" },
    Accessory { id: "drying_rack", label: "Drying rack" },
    Accessory { id: "trouser_rack", label: "Trouser pull-out" },
];

/// Accessory ids that fit this cabinet (for the editor), in catalogue order.
pub fn fitting(cab: &Cabinet) -> Vec<&'static str> {
    CATALOGUE.iter().filter(|a| fits(cab, a.id).is_ok()).map(|a| a.id).collect()
}

/// Ok, or why it does not fit.
fn fits(cab: &Cabinet, id: &str) -> Result<(), &'static str> {
    use CabinetKind::*;
    let w = cab.width;
    let door = cab.doors > 0;
    match id {
        "cutlery" if cab.drawers.is_empty() => Err("needs a drawer"),
        "cutlery" if !cab.kind.is_carcass() => Err("needs a drawer"),
        "cutlery" => Ok(()),
        "bin" if cab.kind != Base || cab.is_l() => Err("is for base cabinets"),
        "bin" if !(300.0..=900.0).contains(&w) => Err("needs a 300–900 mm cabinet"),
        "bin" if !door => Err("needs a door"),
        "bin" => Ok(()),
        "bottle" if cab.kind != Base || cab.is_l() => Err("is for base cabinets"),
        "bottle" if !(150.0..=400.0).contains(&w) => Err("needs a 150–400 mm cabinet"),
        "bottle" => Ok(()),
        "larder" if cab.kind != Tall => Err("is for tall cabinets"),
        "larder" if !(300.0..=600.0).contains(&w) => Err("needs a 300–600 mm cabinet"),
        "larder" => Ok(()),
        "carousel" if !(cab.is_l() && cab.kind == Base) => Err("is for L corner base cabinets"),
        "carousel" => Ok(()),
        "blind_pullout" if cab.blind_side == BlindSide::None || cab.kind != Base => Err("is for blind corner base cabinets"),
        "blind_pullout" => Ok(()),
        "plate_rack" | "drying_rack" if cab.kind != Wall => Err("is for wall cabinets"),
        "plate_rack" | "drying_rack" if !(400.0..=1200.0).contains(&w) => Err("needs a 400–1200 mm cabinet"),
        "plate_rack" | "drying_rack" => Ok(()),
        "trouser_rack" if cab.kind != Tall => Err("is for tall cabinets"),
        "trouser_rack" if w < 450.0 => Err("needs a cabinet 450 mm or wider"),
        "trouser_rack" => Ok(()),
        _ => Err("is not in the catalogue"),
    }
}

/// Hardware name for an accessory in this cabinet: the fitting size
/// (interior width rounded down to 50 mm; corner sizes from the arms).
fn name(project: &Project, cab: &Cabinet, id: &str) -> String {
    let t = project.material(&cab.carcass_material).thickness;
    let iw = ((cab.width - 2.0 * t) / 50.0).floor() * 50.0;
    let cw = ((cab.width.min(cab.width2.max(cab.depth))) / 50.0).floor() * 50.0;
    let label = CATALOGUE.iter().find(|a| a.id == id).map(|a| a.label).unwrap_or(id);
    match id {
        "carousel" => format!("{label} for {:.0} corner", cw),
        "blind_pullout" => format!("{label}, {:.0} cabinet", cab.width),
        "larder" => format!("{label} {:.0} × {:.0}", iw, cab.height),
        _ => format!("{label} {:.0}", iw),
    }
}

pub fn cabinet(project: &Project, cab: &Cabinet, out: &mut Build) {
    for id in &cab.accessories {
        if let Err(why) = fits(cab, id) {
            let label = CATALOGUE.iter().find(|a| a.id == id).map(|a| a.label).unwrap_or(id);
            out.warnings.push(format!("{} ({}): {} {}", cab.name, cab.id, label.to_lowercase(), why));
            continue;
        }
        let n = name(project, cab, id);
        match out.hardware.iter_mut().find(|h| h.cabinet == cab.id && h.name == n) {
            Some(h) => h.qty += 1,
            None => out.hardware.push(Hardware { cabinet: cab.id.clone(), name: n, qty: 1 }),
        }
        if matches!(id.as_str(), "bin" | "larder" | "bottle") && cab.shelves > 0 {
            out.warnings.push(format!("{} ({}): shelves are left in with the {}", cab.name, cab.id, id.replace('_', " ")));
        }
    }
}
