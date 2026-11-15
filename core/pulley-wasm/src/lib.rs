//! WebAssembly bindings for pulley-core, used by the CEP panels and the Figma plugin UI.
//!
//! Every function takes and returns JSON text, so the JavaScript side stays trivial. Errors
//! are thrown as JSON text `{ "code", "message", "hint" }`.
//!
//! This crate is separate from pulley-core only because `#[wasm_bindgen]` expands to unsafe
//! code, which pulley-core forbids. It contains no hand-written unsafe code.

use pulley_core::Error;
use pulley_core::aeplan::{AeOptions, build_plan};
use pulley_core::illustrator::{AiOptions, capture_to_scene};
use pulley_core::validate::{Component, parse_scene, serialize_scene, validate};
use wasm_bindgen::prelude::*;

fn err(e: Error) -> JsValue {
    JsValue::from_str(&serde_json::json!({ "code": e.code.as_str(), "message": e.message, "hint": e.hint }).to_string())
}

fn opts<T: serde::de::DeserializeOwned + Default>(json: &str) -> Result<T, JsValue> {
    if json.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(json)
        .map_err(|e| err(Error::new(pulley_core::ErrorCode::InvalidOptions, format!("Invalid options ({e})."), "Reset the panel's settings.")))
}

fn component(name: &str) -> Component {
    match name {
        "illustrator" => Component::IllustratorPanel,
        "figma" => Component::FigmaPlugin,
        "bridge" => Component::Bridge,
        _ => Component::AePanel,
    }
}

/// Version of the core inside this module.
#[wasm_bindgen]
pub fn version() -> String {
    pulley_core::PULLEY_VERSION.to_string()
}

/// Illustrator capture JSON (+ SVG text, empty if none) to scene JSON.
#[wasm_bindgen(js_name = illustratorToScene)]
pub fn illustrator_to_scene(capture: &str, svg: &str, options: &str) -> Result<String, JsValue> {
    let o: AiOptions = opts(options)?;
    let scene = capture_to_scene(capture, svg, &o).map_err(err)?;
    serialize_scene(&scene).map_err(err)
}

/// Figma node data JSON (from the plugin's main thread) to scene JSON.
#[wasm_bindgen(js_name = figmaToScene)]
pub fn figma_to_scene(nodes: &str, options: &str) -> Result<String, JsValue> {
    let o: pulley_core::figma::FigmaOptions = opts(options)?;
    let scene = pulley_core::figma::nodes_to_scene(nodes, &o).map_err(err)?;
    serialize_scene(&scene).map_err(err)
}

/// Validate a scene received from another component; returns statistics JSON.
#[wasm_bindgen(js_name = checkScene)]
pub fn check_scene(scene: &str, reader: &str) -> Result<String, JsValue> {
    let s = parse_scene(scene, component(reader)).map_err(err)?;
    let st = validate(&s).map_err(err)?;
    Ok(serde_json::json!({
        "items": st.items, "shapes": st.shapes, "texts": st.texts, "groups": st.groups,
        "paths": st.paths, "vertices": st.vertices, "warnings": s.warnings.len(),
        "source": s.source.app, "documentName": s.source.document_name,
    })
    .to_string())
}

/// Scene JSON + AE options JSON to the apply plan: `{ text, gradients, ffx, summary, warnings }`.
/// `ffx` holds one base64 `.ffx` preset per gradient, for the Gradient Colors workaround.
#[wasm_bindgen(js_name = sceneToPlan)]
pub fn scene_to_plan(scene: &str, options: &str) -> Result<String, JsValue> {
    let s = parse_scene(scene, Component::AePanel).map_err(err)?;
    let o: AeOptions = opts(options)?;
    let plan = build_plan(&s, &o).map_err(err)?;
    let ffx: Vec<String> = plan.gradients.iter().map(|g| pulley_core::ffx::gradient_preset_base64(&g.stops)).collect();
    Ok(serde_json::json!({
        "text": plan.text,
        "gradients": plan.gradients,
        "ffx": ffx,
        "summary": plan.summary,
        "warnings": plan.warnings,
        "sceneWarnings": s.warnings,
    })
    .to_string())
}

/// Check one bridge message; returns `{ id, type }` or throws.
#[wasm_bindgen(js_name = checkMessage)]
pub fn check_message(text: &str) -> Result<String, JsValue> {
    let m = pulley_core::protocol::parse_message(text).map_err(err)?;
    Ok(serde_json::json!({ "id": m.id }).to_string())
}
