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

#[wasm_bindgen]
pub fn export_zip(json: &str) -> Result<Vec<u8>, JsError> {
    let p = project(json)?;
    let r = cw::compute(&p);
    Ok(cw::export::bundle(&p, &r))
}

/// kind: "cutlist" | "hardware" | "obj" | "part_dxf" | "sheet_dxf" | "sheet_svg" | "drawing" (index = cabinet)
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
        "quote" => serde_json::to_string(&cw::export::quote::quote(&p, &r)).unwrap_or_default(),
        "quote_csv" => cw::export::quote::quote_csv(&cw::export::quote::quote(&p, &r)),
        "quote_html" => cw::export::quote::quote_html(&p, &cw::export::quote::quote(&p, &r), p.pricing.vat_pct),
        "obj" => cw::export::obj(&p, &r.build),
        "part_dxf" => cw::export::dxf::part_dxf(parts.get(index).ok_or_else(|| JsError::new("no such part"))?),
        "sheet_dxf" => cw::export::dxf::sheet_dxf(sheet()?, parts),
        "sheet_svg" => cw::export::sheet_svg(&p, sheet()?, parts),
        "drawing" => cw::export::drawing::cabinet_svg(
            &p,
            &r.build,
            p.cabinets.get(index).ok_or_else(|| JsError::new("no such cabinet"))?,
        ),
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
