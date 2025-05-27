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

// Embed resources directly in the library
static TEMPLATE_FILE: &str = include_str!("./templates/template.typ"); // Adjust path relative to lib.rs
static FONT0: &[u8] = include_bytes!("./fonts/IBMPlexMath-Regular.ttf"); // Adjust path relative to lib.rs
static FONT1: &[u8] = include_bytes!("./fonts/NotoSerifSC-Regular.otf"); // Adjust path relative to lib.rs

#[derive(Debug, Clone, IntoValue, IntoDict)]
struct FormulaContent {
    formula: String,
    inline: bool,
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
}

pub struct FormulaRenderResult {
    pub formula: String,
    pub is_inline: bool,
    pub format: RenderFormat,
    pub data: Vec<u8>,
    pub x_em: f64,
    pub y_em: f64,
}

/// Implements the Default trait for RenderEngine.
impl Default for RenderEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderEngine {
    pub fn new() -> Self {
        let engine = TypstEngine::builder()
            .main_file(TEMPLATE_FILE)
            .fonts([FONT0, FONT1])
            .with_package_file_resolver()
            .build();

        Self { engine }
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
