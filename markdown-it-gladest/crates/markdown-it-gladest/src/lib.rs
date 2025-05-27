use std::sync::Mutex;

use gladest_engine::{FontConfig, FontSource, RenderEngine, RenderFormat};
use html_escape::encode_text;
use once_cell::sync::Lazy;

use neon::prelude::*;

struct EngineWithConfig {
    engine: RenderEngine,
    config: Option<FontConfig>,
}

static RENDER_ENGINE: Lazy<Mutex<Option<EngineWithConfig>>> = Lazy::new(|| Mutex::new(None));

/// Expands tilde in file paths (similar to main.rs)
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            path.replacen("~", &home, 1)
        } else {
            path.to_string()
        }
    } else if path == "~" {
        std::env::var("HOME").unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

/// Parse font configuration from JavaScript object
fn parse_font_config(
    cx: &mut FunctionContext,
    fonts_obj: Handle<JsObject>,
) -> NeonResult<FontConfig> {
    let mut body_font = FontSource::System("serif".to_string()); // Default
    let mut math_font = FontSource::System("Fira Math".to_string()); // Default
    let mut has_system_font = false;

    // Parse body font
    if let Ok(body_font_obj) = fonts_obj.get::<JsObject, _, _>(cx, "bodyFont") {
        let font_type = body_font_obj
            .get::<JsString, _, _>(cx, "type")
            .map(|s| s.value(cx))
            .unwrap_or_default();
        let font_value = body_font_obj
            .get::<JsString, _, _>(cx, "value")
            .map(|s| s.value(cx))
            .unwrap_or_default();

        match font_type.as_str() {
            "system" => {
                body_font = FontSource::System(font_value);
                has_system_font = true;
            }
            "file" => {
                let expanded_path = expand_tilde(&font_value);
                if !std::path::Path::new(&expanded_path).exists() {
                    return cx
                        .throw_error(format!("Body font file does not exist: {}", expanded_path));
                }
                body_font = FontSource::File(expanded_path);
            }
            _ => {
                return cx.throw_error(format!("Invalid body font type: {}", font_type));
            }
        }
    }

    // Parse math font
    if let Ok(math_font_obj) = fonts_obj.get::<JsObject, _, _>(cx, "mathFont") {
        let font_type = math_font_obj
            .get::<JsString, _, _>(cx, "type")
            .map(|s| s.value(cx))
            .unwrap_or_default();
        let font_value = math_font_obj
            .get::<JsString, _, _>(cx, "value")
            .map(|s| s.value(cx))
            .unwrap_or_default();

        match font_type.as_str() {
            "system" => {
                math_font = FontSource::System(font_value);
                has_system_font = true;
            }
            "file" => {
                let expanded_path = expand_tilde(&font_value);
                if !std::path::Path::new(&expanded_path).exists() {
                    return cx
                        .throw_error(format!("Math font file does not exist: {}", expanded_path));
                }
                math_font = FontSource::File(expanded_path);
            }
            _ => {
                return cx.throw_error(format!("Invalid math font type: {}", font_type));
            }
        }
    }

    // Automatically determine include_system_fonts based on whether any system fonts are used
    let include_system_fonts = has_system_font;

    Ok(FontConfig {
        body_font,
        math_font,
        include_system_fonts,
    })
}

fn get_options(
    cx: &mut FunctionContext,
    options_arg: Handle<JsValue>,
) -> NeonResult<(RenderFormat, Option<f32>, Option<FontConfig>)> {
    let mut format = RenderFormat::Svg;
    let mut ppi = None;
    let mut font_config = None;

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

        // Get font config
        if let Ok(fonts_obj) = options_obj.get::<JsObject, _, _>(cx, "fonts") {
            font_config = Some(parse_font_config(cx, fonts_obj)?);
        }
    }

    Ok((format, ppi, font_config))
}

/// Get or create render engine with the appropriate font configuration
fn get_or_create_engine(
    font_config: Option<FontConfig>,
) -> anyhow::Result<&'static Mutex<Option<EngineWithConfig>>> {
    let mut engine_guard = RENDER_ENGINE.lock().unwrap();

    if let Some(current) = engine_guard.as_ref() {
        let configs_match = match (&current.config, &font_config) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        };

        if configs_match {
            drop(engine_guard);
            return Ok(&RENDER_ENGINE);
        }
    }

    let engine = if let Some(config) = font_config.clone() {
        RenderEngine::with_font_config(config)
    } else {
        RenderEngine::new()
    };

    *engine_guard = Some(EngineWithConfig {
        engine,
        config: font_config,
    });

    drop(engine_guard);
    Ok(&RENDER_ENGINE)
}

// Neon function to render a single formula
// Args: formula (String), delimiter (String: "$$" or "$"), options (Object: { format?: "svg"|"png", ppi?: number, fonts?: FontConfig })
// Returns: String (HTML <img> tag or error message)
fn render_latex(mut cx: FunctionContext) -> JsResult<JsString> {
    // 1. Get arguments
    let formula = cx.argument::<JsString>(0)?.value(&mut cx);
    let delimiter = cx.argument::<JsString>(1)?.value(&mut cx);
    let options_arg = cx
        .argument_opt(2)
        .unwrap_or_else(|| cx.undefined().upcast()); // Handle missing options

    // 2. Parse options
    let (format, ppi, font_config) = get_options(&mut cx, options_arg)?;

    // 3. Determine environment class based on delimiter
    let is_inline = delimiter != "$$";

    // 4. Get or create engine and render
    let result = match get_or_create_engine(font_config) {
        Ok(engine_ref) => {
            let engine_guard = engine_ref.lock().unwrap();
            if let Some(ref engine_with_config) = *engine_guard {
                engine_with_config
                    .engine
                    .render_formula(&formula, is_inline, format, ppi)
            } else {
                return Ok(cx.string(format!(
                    r#"<span class="gladst-error" title="Engine not initialized">Gladst Error: Engine not initialized. Formula: {}</span>"#,
                    encode_text(&formula)
                )));
            }
        }
        Err(e) => {
            eprintln!("Error creating render engine: {:?}", e);
            let error_message = format!(
                "Gladst Error: Failed to create render engine. Formula: {}",
                encode_text(&formula)
            );
            return Ok(cx.string(format!(
                r#"<span class="gladst-error" title="{}">{}</span>"#,
                encode_text(&e.to_string()),
                error_message
            )));
        }
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

// Neon function to set global font configuration
// Args: fontConfig (Object: { bodyFont?: {type: "system"|"file", value: string}, mathFont?: {type: "system"|"file", value: string} })
// Returns: Boolean (success)
fn set_font_config(mut cx: FunctionContext) -> JsResult<JsBoolean> {
    let fonts_arg = cx.argument::<JsValue>(0)?;

    if let Ok(fonts_obj) = fonts_arg.downcast::<JsObject, _>(&mut cx) {
        match parse_font_config(&mut cx, fonts_obj) {
            Ok(font_config) => {
                let mut engine_guard = RENDER_ENGINE.lock().unwrap();

                let needs_update = match engine_guard.as_ref() {
                    Some(current) => match &current.config {
                        Some(current_config) => current_config != &font_config,
                        None => true,
                    },
                    None => true,
                };

                if needs_update {
                    let engine = RenderEngine::with_font_config(font_config.clone());
                    *engine_guard = Some(EngineWithConfig {
                        engine,
                        config: Some(font_config),
                    });
                }

                Ok(cx.boolean(true))
            }
            Err(e) => Err(e),
        }
    } else {
        cx.throw_error("Font configuration must be an object")
    }
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("renderLatex", render_latex)?;
    cx.export_function("setFontConfig", set_font_config)?;
    Ok(())
}
