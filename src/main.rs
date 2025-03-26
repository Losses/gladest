use std::sync::{Arc, Mutex};
use std::{fs, path::PathBuf};

use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, arg, command};
use clap_derive::{Parser, ValueEnum};
use derive_typst_intoval::{IntoDict, IntoValue};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use scraper::{Html, Selector};
use typst::{
    foundations::{Dict, IntoValue},
    layout::PagedDocument,
};
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};
use urlencoding::encode;

static TEMPLATE_FILE: &str = include_str!("./templates/template.typ");
static FONT0: &[u8] = include_bytes!("./fonts/IBMPlexMath-Regular.ttf");
static FONT1: &[u8] = include_bytes!("./fonts/NotoSerifSC-Regular.otf");

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input HTEX file
    input: PathBuf,

    /// Output directory
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Pixels per inch for rasterization
    #[arg(short, long, default_value_t = 1200)]
    ppi: u32,

    /// Output format (png or svg)
    #[arg(short, long, default_value = "png", value_enum)]
    format: Format,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Format {
    Png,
    Svg,
}

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

struct RenderEngine {
    engine: TypstEngine<TypstTemplateMainFile>,
    ppi: f32,
}

struct FormulaRenderResult {
    data: Vec<u8>,
    x_em: f64,
    y_em: f64,
}

impl RenderEngine {
    fn new(ppi: f32) -> Self {
        let engine = TypstEngine::builder()
            .main_file(TEMPLATE_FILE)
            .fonts([FONT0, FONT1])
            .with_package_file_resolver()
            .build();

        Self { engine, ppi }
    }

    fn render_formula(&self, formula: &str, inline: bool, format: Format) -> FormulaRenderResult {
        let content = FormulaContent {
            formula: formula.to_string(),
            inline,
        };

        let doc: PagedDocument = self
            .engine
            .compile_with_input(content)
            .output
            .expect("Failed to compile formula");

        let page = &doc.pages[0];
        let size = page.frame.size();
        let x_pt = size.x.to_pt();
        let y_pt = size.y.to_pt();
        const EM_TO_PT: f64 = 10.0;
        let x_em = x_pt / EM_TO_PT;
        let y_em = y_pt / EM_TO_PT;

        FormulaRenderResult {
            data: match format {
                Format::Svg => typst_svg::svg(page).into_bytes(),
                Format::Png => typst_render::render(page, self.ppi / 72.0)
                    .encode_png()
                    .expect("Failed to encode PNG"),
            },
            x_em,
            y_em,
        }
    }
}

fn process_html(input: &str, ppi: f32, format: Format) -> String {
    let document = Html::parse_document(input);
    let selector = Selector::parse("eq").unwrap();

    let mut processed_html_string = document.html();
    let mut formula_tasks = Vec::new();

    for (formula_id_counter, element) in document.select(&selector).enumerate() {
        let formula = element.text().collect::<String>();
        let env = element
            .value()
            .attr("env")
            .map(|s| s.to_string())
            .unwrap_or_default();
        let original_eq_html = element.html();
        let formula_id = format!("__FORMULA_ID_{}__", formula_id_counter);

        processed_html_string = processed_html_string.replacen(&original_eq_html, &formula_id, 1);

        formula_tasks.push((formula_id, formula, env));
    }

    let pb = ProgressBar::new(formula_tasks.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    let pb = Arc::new(pb);
    let processed_html_string_mutex = Arc::new(Mutex::new(processed_html_string));

    formula_tasks
        .into_par_iter()
        .for_each(|(formula_id, formula, env)| {
            let is_inline = match env.as_str() {
                "displaymath" => false,
                "math" => true,
                _ => {
                    if !env.is_empty() {
                        eprintln!(
                            "Warning: env '{}' is not recognized, defaulting to inline.",
                            env
                        );
                    } else {
                        eprintln!("Warning: no env attribute specified, defaulting to inline.");
                    }
                    true
                }
            };

            let renderer = RenderEngine::new(ppi);
            let result = renderer.render_formula(&formula, is_inline, format);

            let x_em = result.x_em;
            let y_em = result.y_em;

            let mime_type = match format {
                Format::Svg => "image/svg+xml",
                Format::Png => "image/png",
            };
            let b64 = general_purpose::STANDARD.encode(&result.data);
            let formula_escaped = encode(&formula);

            let replacement = format!(
                r#"<img class="gladst {}" style="width: {x_em}em; height: {y_em}em" src="data:{mime_type};base64,{b64}" alt="{formula_escaped}"/>"#,
                env
            );

            let mut locked_string = processed_html_string_mutex.lock().unwrap();
            *locked_string = locked_string.replacen(&formula_id, &replacement, 1);
            drop(locked_string);

            Arc::clone(&pb).inc(1);
        });

    Arc::try_unwrap(pb).unwrap().finish_and_clear();

    Arc::try_unwrap(processed_html_string_mutex)
        .unwrap()
        .into_inner()
        .unwrap()
}

fn main() {
    let args = Args::parse();

    let output_dir = args.output.unwrap_or_else(|| {
        args.input
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    });

    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let input_content = fs::read_to_string(&args.input).expect("Failed to read input file");

    let processed_html = process_html(&input_content, args.ppi as f32, args.format);

    let output_path = output_dir
        .join(args.input.file_stem().unwrap())
        .with_extension("html");
    fs::write(output_path, processed_html).expect("Failed to write output file");
}
