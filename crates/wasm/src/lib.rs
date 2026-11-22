//! WebAssembly bindings. All inputs/outputs are JSON strings or bytes.

use casework_core as cw;
use wasm_bindgen::prelude::*;

fn project(json: &str) -> Result<cw::Project, JsError> {
    cw::parse(json).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen]
pub fn sample_project() -> String {
    serde_json::to_string(&cw::sample_kitchen()).unwrap_or_default()
}

/// Parts, hardware, nesting and summary as JSON.
#[wasm_bindgen]
pub fn compute(json: &str) -> Result<String, JsError> {
    let p = project(json)?;
    serde_json::to_string(&cw::compute(&p)).map_err(|e| JsError::new(&e.to_string()))
}

/// `date` (any text, e.g. "2026-11-22") is printed on the shop packet.
#[wasm_bindgen]
pub fn export_zip(json: &str, date: Option<String>) -> Result<Vec<u8>, JsError> {
    let p = project(json)?;
    let r = cw::compute(&p);
    Ok(cw::export::bundle_dated(&p, &r, date.as_deref()))
}

/// Customer quote with pictures (data: URLs, e.g. renders of the 3D view).
#[wasm_bindgen]
pub fn quote_html(json: &str, date: Option<String>, images: Vec<String>) -> Result<String, JsError> {
    let p = project(json)?;
    let r = cw::compute(&p);
    Ok(cw::export::quote::quote_html(&p, &cw::export::quote::quote(&p, &r), date.as_deref(), &images))
}

#[wasm_bindgen]
pub fn packet_html(json: &str, date: Option<String>) -> Result<String, JsError> {
    let p = project(json)?;
    let r = cw::compute(&p);
    Ok(cw::export::packet::packet_html(&p, &r, date.as_deref()))
}

/// kind: "cutlist" | "hardware" | "obj" | "part_dxf" | "sheet_dxf" | "sheet_svg" | "drawing" (index = cabinet) | "wall" (index = wall) | "plan"
#[wasm_bindgen]
pub fn export_one(json: &str, kind: &str, index: usize) -> Result<String, JsError> {
    let p = project(json)?;
    let r = cw::compute(&p);
    let parts = &r.build.parts;
    let sheet = || r.nesting.sheets.get(index).ok_or_else(|| JsError::new("no such sheet"));
    Ok(match kind {
        "cutlist" => cw::export::cutlist_csv(&p, &r.build),
        "hardware" => cw::export::hardware_csv(&p, &r.build),
        "labels" => cw::export::labels_csv(&p, &r),
        "labels_html" => cw::export::packet::labels_html(&p, &r),
        "quote" => serde_json::to_string(&cw::export::quote::quote(&p, &r)).unwrap_or_default(),
        "quote_csv" => cw::export::quote::quote_csv(&cw::export::quote::quote(&p, &r)),
        "quote_html" => cw::export::quote::quote_html(&p, &cw::export::quote::quote(&p, &r), None, &[]),
        "obj" => cw::export::obj(&p, &r.build),
        "part_dxf" => cw::export::dxf::part_dxf(parts.get(index).ok_or_else(|| JsError::new("no such part"))?),
        "sheet_dxf" => cw::export::dxf::sheet_dxf(sheet()?, parts),
        "sheet_svg" => cw::export::sheet_svg(&p, sheet()?, parts),
        "sheet_nc" => cw::export::gcode::sheet_program(&p, sheet()?, parts, index),
        "plan" => cw::export::drawing::plan_svg(&p).ok_or_else(|| JsError::new("no room"))?,
        "wall" => cw::export::drawing::wall_svg(&p, &r.build, index).ok_or_else(|| JsError::new("no such wall"))?,
        "drawing" => cw::export::drawing::cabinet_svg(&p, &r.build, p.cabinets.get(index).ok_or_else(|| JsError::new("no such cabinet"))?),
        _ => return Err(JsError::new("unknown export kind")),
    })
}

/// Wrap one file in a ZIP (for hosts that only allow certain extensions).
#[wasm_bindgen]
pub fn zip_one(name: &str, data: &[u8]) -> Vec<u8> {
    cw::export::zip::store(&[(name.to_string(), data.to_vec())])
}

/// Parse with defaults for every missing field and re-serialize, so the
/// editor always works on a complete project (older saves, hand-written JSON).
#[wasm_bindgen]
pub fn normalize(json: &str) -> Result<String, JsError> {
    let p = project(json)?;
    serde_json::to_string(&p).map_err(|e| JsError::new(&e.to_string()))
}

/// Fill wall `wall` with cabinets. tier: "base" | "wall" | "both".
/// Returns the updated project JSON.
#[wasm_bindgen]
pub fn fill_wall(json: &str, wall: usize, tier: &str) -> Result<String, JsError> {
    use cw::room::{fill_wall, Tier};
    let mut p = project(json)?;
    let tiers: &[Tier] = match tier {
        "base" => &[Tier::Base],
        "wall" => &[Tier::Wall],
        "both" => &[Tier::Base, Tier::Wall],
        _ => return Err(JsError::new("tier must be base, wall or both")),
    };
    for &t in tiers {
        let new = fill_wall(&p, wall, t).map_err(|e| JsError::new(&e))?;
        p.cabinets.extend(new);
    }
    serde_json::to_string(&p).map_err(|e| JsError::new(&e.to_string()))
}

/// Replace the room with the outline found in a DXF floor plan.
/// Returns `{"project": <json>, "info": "<walls, units, loops>"}`.
#[wasm_bindgen]
pub fn import_room_dxf(json: &str, dxf: &str) -> Result<String, JsError> {
    let mut p = project(json)?;
    let (room, info) = cw::import_dxf::room_from_dxf(dxf).map_err(|e| JsError::new(&e))?;
    let height = p.room.height;
    let thickness = p.room.thickness;
    p.room = cw::room::Room { height, thickness, ..room };
    serde_json::to_string(&serde_json::json!({ "project": p, "info": info })).map_err(|e| JsError::new(&e.to_string()))
}
