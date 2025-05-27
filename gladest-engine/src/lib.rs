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
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};

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
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            body_font: FontSource::System("serif".to_string()),
            math_font: FontSource::System("Fira Math".to_string()),
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

impl RenderEngine {
    /// Create a new render engine with default font configuration
    pub fn new() -> Self {
        Self::with_font_config(FontConfig::default())
    }

    /// Create a new render engine with custom font configuration
    pub fn with_font_config(font_config: FontConfig) -> Self {
        let mut engine_builder = TypstEngine::builder()
            .main_file(Self::generate_template(&font_config))
            .with_package_file_resolver();

        // Collect font data for the engine
        let mut font_data = Vec::new();

        if let FontSource::Data(data) = &font_config.body_font {
            font_data.push(data.as_slice());
        }
        if let FontSource::Data(data) = &font_config.math_font {
            font_data.push(data.as_slice());
        }

        // Load font files if specified
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
            r#"
#import sys: inputs
#import "@preview/mitex:0.2.5": *

#set text(font: "{}", size: 10pt)
#set page(fill: none, width: auto, height: auto, margin: (left: 0pt, right: 0pt, top: 0.455em, bottom: 0.455em))
#show math.equation: set text(font: "{}")

#let content = inputs.formula
#let inline = inputs.inline
#let body_font = inputs.body_font
#let math_font = inputs.math_font

// Override fonts if provided in inputs
#if body_font != "" [
  #set text(font: body_font)
]
#if math_font != "" [
  #show math.equation: set text(font: math_font)
]

#if inline [
  #mi(content)
] else [
  #mitex(content)
]"#,
            body_font, math_font
        )
    }

    /// Convert FontSource to Typst font name
    fn font_source_to_typst_name(font_source: &FontSource) -> String {
        match font_source {
            FontSource::System(name) => name.clone(),
            FontSource::File(path) => {
                // Extract font name from file if possible, otherwise use filename
                Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("serif")
                    .to_string()
            }
            FontSource::Data(_) => "embedded".to_string(),
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

        let doc: PagedDocument = self
            .engine
            .compile_with_input(content)
            .output
            .with_context(|| format!("Failed to compile formula: {}", formula))?;

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

        let doc: PagedDocument = self
            .engine
            .compile_with_input(content)
            .output
            .with_context(|| format!("Failed to compile formula: {}", formula))?;

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
