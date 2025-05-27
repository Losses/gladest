use std::path::Path;

use allsorts::{
    binary::read::ReadScope,
    font_data::FontData,
    tables::{FontTableProvider, NameTable},
    tag,
};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose};
use derive_typst_intoval::{IntoDict, IntoValue};
use html_escape::encode_text;
use typst::{
    foundations::{Dict, IntoValue},
    layout::PagedDocument,
};
use typst_as_lib::{
    TypstAsLibError, TypstEngine, TypstTemplateMainFile, typst_kit_options::TypstKitFontOptions,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    Png,
    Svg,
}

#[derive(Debug)]
pub struct FontNames {
    pub family_name: Option<String>,
    pub subfamily_name: Option<String>,
    pub full_name: Option<String>,
    pub postscript_name: Option<String>,
    pub typographic_family_name: Option<String>,
    pub typographic_subfamily_name: Option<String>,
}

/// Font source configuration
#[derive(Debug, Clone)]
pub enum FontSource {
    /// Use a font file from the filesystem
    File(String),
    /// Use a system font by name
    System(String),
    /// Use font data directly from memory
    Data(Vec<u8>),
}

/// Font configuration for rendering
#[derive(Debug, Clone)]
pub struct FontConfig {
    /// Font used for body text
    pub body_font: FontSource,
    /// Font used for mathematical expressions
    pub math_font: FontSource,
    /// Whether to include system fonts in the search
    pub include_system_fonts: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            body_font: FontSource::System("serif".to_string()),
            math_font: FontSource::System("Fira Math".to_string()),
            include_system_fonts: true,
        }
    }
}

pub fn read_font_names(font_data: &[u8], font_index: usize) -> Result<FontNames> {
    // Parse the font file (supports OpenType, WOFF, WOFF2, CFF)
    let scope = ReadScope::new(font_data);
    let font = scope.read::<FontData<'_>>()?;

    // Get table provider for the specified font index (useful for font collections)
    let provider = font.table_provider(font_index)?;

    // Read the NAME table
    let name_data = provider.read_table_data(tag::NAME)?;
    let name = ReadScope::new(&name_data).read::<NameTable<'_>>()?;

    // Extract various name types with fallback logic
    let family_name = name
        .string_for_id(NameTable::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name.string_for_id(NameTable::FONT_FAMILY_NAME));

    let subfamily_name = name
        .string_for_id(NameTable::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| name.string_for_id(NameTable::FONT_SUBFAMILY_NAME));

    Ok(FontNames {
        family_name,
        subfamily_name,
        full_name: name.string_for_id(NameTable::FULL_FONT_NAME),
        postscript_name: name.string_for_id(NameTable::POSTSCRIPT_NAME),
        typographic_family_name: name.string_for_id(NameTable::TYPOGRAPHIC_FAMILY_NAME),
        typographic_subfamily_name: name.string_for_id(NameTable::TYPOGRAPHIC_SUBFAMILY_NAME),
    })
}

#[derive(Debug, Clone, IntoValue, IntoDict)]
struct FormulaContent {
    formula: String,
    inline: bool,
    body_font: String,
    math_font: String,
}

impl From<FormulaContent> for Dict {
    fn from(value: FormulaContent) -> Self {
        value.into_dict()
    }
}

/// Holds the result of a formula rendering operation.
#[derive(Debug)]
pub struct RenderResult {
    /// The raw image data (PNG or SVG bytes).
    pub data: Vec<u8>,
    /// The calculated width of the rendered formula in em units.
    pub width_em: f64,
    /// The calculated height of the rendered formula in em units.
    pub height_em: f64,
}

pub struct RenderEngine {
    engine: TypstEngine<TypstTemplateMainFile>,
    font_config: FontConfig,
}

pub struct FormulaRenderResult {
    pub formula: String,
    pub is_inline: bool,
    pub format: RenderFormat,
    pub data: Vec<u8>,
    pub x_em: f64,
    pub y_em: f64,
}

/// Helper function to format Typst compilation errors with detailed information
fn format_typst_error(error: &TypstAsLibError, formula: &str) -> String {
    match error {
        TypstAsLibError::TypstSource(diagnostics) => {
            let mut error_msg = "Failed to compile the formula\n".to_string();

            if diagnostics.is_empty() {
                error_msg.push_str("Compilation failed with unspecified diagnostics.\n");
                return error_msg;
            }

            for (i, diagnostic) in diagnostics.iter().enumerate() {
                error_msg.push_str(&format!("\n{:?} #{}: ", diagnostic.severity, i + 1));

                // Add the main error message
                error_msg.push_str(&diagnostic.message);
                error_msg.push('\n');

                // Add span information
                let span = diagnostic.span;
                if let Some(file_id) = span.id() {
                    let path_repr = file_id.vpath().as_rootless_path().display().to_string();
                    error_msg.push_str(&format!("  Location: {}\n", path_repr));
                    // Optionally, include byte range if useful, though it's less user-friendly
                    // let range = span.range();
                    // error_msg.push_str(&format!("    Byte Range: {}-{}\n", range.start, range.end));
                } else {
                    error_msg.push_str("  Location: No specific source file associated (detached span or nil FileId).\n");
                }

                // Add trace information if available
                if !diagnostic.trace.is_empty() {
                    error_msg.push_str("  Trace:\n");
                    for point in &diagnostic.trace {
                        // point is Spanned<Tracepoint>
                        // point.v is the Tracepoint enum
                        // point.span is the Span of this tracepoint
                        let trace_span_info = if let Some(trace_file_id) = point.span.id() {
                            format!("at {}", trace_file_id.vpath().as_rootless_path().display())
                        } else {
                            "at detached span or nil FileId".to_string()
                        };
                        error_msg.push_str(&format!("    - {}: {}\n", point.v, trace_span_info));
                    }
                }

                // Add hints if available
                if !diagnostic.hints.is_empty() {
                    error_msg.push_str("  Hints:\n");
                    for hint in &diagnostic.hints {
                        error_msg.push_str(&format!("    - {}\n", hint));
                    }
                }
            }
            error_msg
        }
        TypstAsLibError::TypstFile(file_err) => {
            format!(
                "File Error while processing formula '{}': {}\n",
                formula, file_err
            )
        }
        TypstAsLibError::MainSourceFileDoesNotExist(file_id) => {
            // Here file_id is FileId, not Option<FileId>, so direct usage is correct.
            format!(
                "Main source file not found for formula '{}': {:?}.\n  Path (vpath): {}\n",
                formula,
                file_id,
                file_id.vpath().as_rootless_path().display()
            )
        }
        TypstAsLibError::HintedString(hinted_str) => {
            let mut msg = format!(
                "Error processing formula '{}': {}\n",
                formula,
                hinted_str.message()
            );
            if !hinted_str.hints().is_empty() {
                msg.push_str("  Hints:\n");
                for hint in hinted_str.hints() {
                    msg.push_str(&format!("    - {}\n", hint));
                }
            }
            msg
        }
        TypstAsLibError::Unspecified(err_msg) => {
            format!("Unspecified error for formula '{}': {}\n", formula, err_msg)
        }
    }
}

impl RenderEngine {
    /// Create a new render engine with default font configuration
    pub fn new() -> Self {
        Self::with_font_config(FontConfig::default())
    }

    /// Create a new render engine with custom font configuration
    pub fn with_font_config(font_config: FontConfig) -> Self {
        let source = Self::generate_template(&font_config);

        let mut engine_builder = TypstEngine::builder()
            .main_file(source)
            .with_package_file_resolver();

        // Configure font search options
        let font_options = TypstKitFontOptions::default()
            .include_system_fonts(font_config.include_system_fonts)
            .include_embedded_fonts(false);

        // Apply font search configuration
        engine_builder = engine_builder.search_fonts_with(font_options);

        // Collect additional font data for the engine
        let mut font_data = Vec::new();

        // Only add Data fonts to the engine's font collection
        // System and File fonts will be handled by the font search mechanism
        if let FontSource::Data(data) = &font_config.body_font {
            font_data.push(data.as_slice());
        }
        if let FontSource::Data(data) = &font_config.math_font {
            font_data.push(data.as_slice());
        }

        // Load font files if specified and add them to the font collection
        if let FontSource::File(path) = &font_config.body_font {
            if let Ok(data) = std::fs::read(path) {
                font_data.push(Box::leak(data.into_boxed_slice()));
            }
        }
        if let FontSource::File(path) = &font_config.math_font {
            if let Ok(data) = std::fs::read(path) {
                font_data.push(Box::leak(data.into_boxed_slice()));
            }
        }

        // Add collected font data to the engine if any
        if !font_data.is_empty() {
            engine_builder = engine_builder.fonts(font_data);
        }

        let engine = engine_builder.build();

        Self {
            engine,
            font_config,
        }
    }

    /// Generate the Typst template based on font configuration
    fn generate_template(font_config: &FontConfig) -> String {
        let body_font = Self::font_source_to_typst_name(&font_config.body_font);
        let math_font = Self::font_source_to_typst_name(&font_config.math_font);

        format!(
            r#"#import sys: inputs
#import "@preview/mitex:0.2.5": *

{}
#set page(fill: none, width: auto, height: auto, margin: (left: 0pt, right: 0pt, top: 0.455em, bottom: 0.455em))
{}

#let content = inputs.formula
#let inline = inputs.inline

#if inline [
  #mi(content)
] else [
  #mitex(content)
]"#,
            if !body_font.is_empty() {
                format!("#set text(font: \"{body_font}\", size: 10pt)")
            } else {
                "#set text(size: 10pt)".to_string()
            },
            if !math_font.is_empty() {
                format!("#show math.equation: set text(font: \"{math_font}\")")
            } else {
                "".to_string()
            },
        )
    }

    /// Convert FontSource to Typst font name
    fn font_source_to_typst_name(font_source: &FontSource) -> String {
        match font_source {
            FontSource::System(name) => name.clone(),
            FontSource::File(path) => {
                // For file fonts, try to extract the actual font name from the file
                // If that fails, fall back to using the filename
                if let Ok(font_data) = std::fs::read(path) {
                    if let Ok(font_names) = read_font_names(&font_data, 0) {
                        if let Some(family_name) = font_names.family_name {
                            return family_name;
                        }
                    }
                }

                // Fallback to filename if font name extraction fails
                Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("serif")
                    .to_string()
            }
            FontSource::Data(data) => {
                // For data fonts, try to extract the actual font name
                if let Ok(font_names) = read_font_names(data, 0) {
                    if let Some(family_name) = font_names.family_name {
                        return family_name;
                    }
                }
                "embedded".to_string()
            }
        }
    }

    /// Update the font configuration and rebuild the engine
    pub fn set_font_config(&mut self, font_config: FontConfig) -> Result<()> {
        *self = Self::with_font_config(font_config);
        Ok(())
    }

    /// Get the current font configuration
    pub fn font_config(&self) -> &FontConfig {
        &self.font_config
    }

    pub fn render_formula(
        &self,
        formula: &str,
        is_inline: bool,
        format: RenderFormat,
        ppi: Option<f32>,
    ) -> Result<FormulaRenderResult> {
        let content = FormulaContent {
            formula: formula.to_string(),
            inline: is_inline,
            body_font: Self::font_source_to_typst_name(&self.font_config.body_font),
            math_font: Self::font_source_to_typst_name(&self.font_config.math_font),
        };

        let ppi = ppi.unwrap_or(1200.0);

        let result = self.engine.compile_with_input(content);

        let doc: PagedDocument = match result.output {
            Ok(doc) => doc,
            Err(error) => {
                let error_details = format_typst_error(&error, formula);
                return Err(anyhow::anyhow!("{}", error_details));
            }
        };

        let page = &doc.pages[0];
        let size = page.frame.size();
        let x_pt = size.x.to_pt();
        let y_pt = size.y.to_pt();
        const EM_TO_PT: f64 = 10.0;
        let x_em = x_pt / EM_TO_PT;
        let y_em = y_pt / EM_TO_PT;

        let data = match format {
            RenderFormat::Svg => typst_svg::svg(page).into_bytes(),
            RenderFormat::Png => {
                let pixel_width = (size.x.to_pt() * ppi as f64 / 72.0).round() as u32;
                let pixel_height = (size.y.to_pt() * ppi as f64 / 72.0).round() as u32;

                if pixel_width == 0 || pixel_height == 0 {
                    vec![]
                } else {
                    let pixmap = typst_render::render(page, ppi / 72.0);
                    pixmap
                        .encode_png()
                        .with_context(|| format!("Failed to encode PNG for formula: {}", formula))?
                }
            }
        };

        Ok(FormulaRenderResult {
            formula: formula.to_string(),
            is_inline,
            format,
            data,
            x_em,
            y_em,
        })
    }

    /// Render formula with custom fonts for this specific render
    pub fn render_formula_with_fonts(
        &self,
        formula: &str,
        is_inline: bool,
        format: RenderFormat,
        ppi: Option<f32>,
        body_font: Option<&str>,
        math_font: Option<&str>,
    ) -> Result<FormulaRenderResult> {
        let content = FormulaContent {
            formula: formula.to_string(),
            inline: is_inline,
            body_font: body_font
                .unwrap_or(&Self::font_source_to_typst_name(
                    &self.font_config.body_font,
                ))
                .to_string(),
            math_font: math_font
                .unwrap_or(&Self::font_source_to_typst_name(
                    &self.font_config.math_font,
                ))
                .to_string(),
        };

        let ppi = ppi.unwrap_or(1200.0);

        let result = self.engine.compile_with_input(content);

        let doc: PagedDocument = match result.output {
            Ok(doc) => doc,
            Err(error) => {
                let error_details = format_typst_error(&error, formula);
                return Err(anyhow::anyhow!("{}", error_details));
            }
        };

        let page = &doc.pages[0];
        let size = page.frame.size();
        let x_pt = size.x.to_pt();
        let y_pt = size.y.to_pt();
        const EM_TO_PT: f64 = 10.0;
        let x_em = x_pt / EM_TO_PT;
        let y_em = y_pt / EM_TO_PT;

        let data = match format {
            RenderFormat::Svg => typst_svg::svg(page).into_bytes(),
            RenderFormat::Png => {
                let pixel_width = (size.x.to_pt() * ppi as f64 / 72.0).round() as u32;
                let pixel_height = (size.y.to_pt() * ppi as f64 / 72.0).round() as u32;

                if pixel_width == 0 || pixel_height == 0 {
                    vec![]
                } else {
                    let pixmap = typst_render::render(page, ppi / 72.0);
                    pixmap
                        .encode_png()
                        .with_context(|| format!("Failed to encode PNG for formula: {}", formula))?
                }
            }
        };

        Ok(FormulaRenderResult {
            formula: formula.to_string(),
            is_inline,
            format,
            data,
            x_em,
            y_em,
        })
    }
}

/// Implements the Default trait for RenderEngine.
impl Default for RenderEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FormulaRenderResult {
    pub fn to_html(&self) -> String {
        let mime_type = match self.format {
            RenderFormat::Svg => "image/svg+xml",
            RenderFormat::Png => "image/png",
        };
        let b64 = general_purpose::STANDARD.encode(&self.data);
        let formula_escaped = encode_text(&self.formula);

        format!(
            r#"<img class="gladst {env}" style="width: {x_em:.4}em; height: {y_em:.4}em; vertical-align: middle;" src="data:{mime_type};base64,{b64}" alt="{formula_escaped}"/>"#,
            env = if self.is_inline {
                "math"
            } else {
                "displaymath"
            },
            x_em = self.x_em,
            y_em = self.y_em,
            mime_type = mime_type,
            b64 = b64,
            formula_escaped = formula_escaped
        )
    }
}
