use anyhow::{Context, Result};
use derive_typst_intoval::{IntoDict, IntoValue};
use typst::{
    foundations::{Dict, IntoValue},
    layout::PagedDocument,
};
use typst_as_lib::TypstEngine;
use typst_as_lib::TypstTemplateMainFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    Png,
    Svg,
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
    ppi: f32,
}

pub struct FormulaRenderResult {
    pub data: Vec<u8>,
    pub x_em: f64,
    pub y_em: f64,
}

impl RenderEngine {
    pub fn new(ppi: f32) -> Self {
        let engine = TypstEngine::builder()
            .main_file(TEMPLATE_FILE)
            .fonts([FONT0, FONT1])
            .with_package_file_resolver()
            .build();

        Self { engine, ppi }
    }

    pub fn render_formula(
        &self,
        formula: &str,
        inline: bool,
        format: RenderFormat,
    ) -> Result<FormulaRenderResult> {
        let content = FormulaContent {
            formula: formula.to_string(),
            inline,
        };

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
                let pixel_width = (size.x.to_pt() * self.ppi as f64 / 72.0).round() as u32;
                let pixel_height = (size.y.to_pt() * self.ppi as f64 / 72.0).round() as u32;

                if pixel_width == 0 || pixel_height == 0 {
                    vec![]
                } else {
                    let pixmap = typst_render::render(page, self.ppi / 72.0);
                    pixmap
                        .encode_png()
                        .with_context(|| format!("Failed to encode PNG for formula: {}", formula))?
                }
            }
        };

        Ok(FormulaRenderResult { data, x_em, y_em })
    }
}
