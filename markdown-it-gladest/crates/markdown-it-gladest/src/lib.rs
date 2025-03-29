use std::sync::Mutex;

use gladest_engine::{RenderEngine, RenderFormat};
use html_escape::encode_text;
use once_cell::sync::Lazy;

use neon::prelude::*;

static RENDER_ENGINE: Lazy<Mutex<RenderEngine>> = Lazy::new(|| Mutex::new(RenderEngine::new()));

fn get_options(
    cx: &mut FunctionContext,
    options_arg: Handle<JsValue>,
) -> (RenderFormat, Option<f32>) {
    let mut format = RenderFormat::Svg;
    let mut ppi = None;

    if let Ok(options_obj) = options_arg.downcast::<JsObject, _>(cx) {
        // Get format
        if let Ok(format_val) = options_obj.get::<JsString, _, _>(cx, "format") {
            let format_str = format_val.value(cx);
            if format_str.eq_ignore_ascii_case("png") {
                format = RenderFormat::Png;
            } else if format_str.eq_ignore_ascii_case("svg") {
                format = RenderFormat::Svg;
            }
            // Ignore invalid values, keep default
        }

        // Get PPI
        if let Ok(ppi_val) = options_obj.get::<JsNumber, _, _>(cx, "ppi") {
            let ppi_f64 = ppi_val.value(cx);
            if ppi_f64 > 0.0 {
                ppi = Some(ppi_f64 as f32);
            }
        }
    }

    (format, ppi)
}

// Neon function to render a single formula
// Args: formula (String), delimiter (String: "$$" or "$$$"), options (Object: { format?: "svg"|"png", ppi?: number })
// Returns: String (HTML <img> tag or error message)
fn render_latex(mut cx: FunctionContext) -> JsResult<JsString> {
    // 1. Get arguments
    let formula = cx.argument::<JsString>(0)?.value(&mut cx);
    let delimiter = cx.argument::<JsString>(1)?.value(&mut cx);
    let options_arg = cx
        .argument_opt(2)
        .unwrap_or_else(|| cx.undefined().upcast()); // Handle missing options

    // 2. Parse options
    let (format, ppi) = get_options(&mut cx, options_arg);

    // 3. Determine environment class based on delimiter
    let is_inline = delimiter != "$$";

    // 4. Lock the engine and render
    let result = {
        // Scope the lock
        let engine = RENDER_ENGINE.lock().unwrap();
        engine.render_formula(&formula, is_inline, format, ppi)
    };

    // 5. Handle result and format output
    match result {
        Ok(render_result) => {
            let html = render_result.to_html();
            Ok(cx.string(html))
        }
        Err(e) => {
            // Log the error on the Rust side for debugging
            eprintln!("Error rendering formula: {:?}", e);
            // Return an error message string to JS, maybe styled
            let error_message = format!(
                "Gladst Error: Failed to render formula. Check console. Formula: {}",
                encode_text(&formula)
            );
            Ok(cx.string(format!(
                r#"<span class="gladst-error" title="{}">{}</span>"#,
                encode_text(&e.to_string()),
                error_message
            )))
        }
    }
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("renderLatex", render_latex)?;
    Ok(())
}