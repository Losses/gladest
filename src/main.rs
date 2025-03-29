use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, arg, command};
use clap_derive::{Parser, ValueEnum};
use derive_typst_intoval::{IntoDict, IntoValue};
use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
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
    /// Input file path or glob pattern (e.g., "doc.htex", "*.html", "docs/**/*.htex")
    input: String,

    /// Output directory (only used for .htex inputs)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Pixels per inch for rasterization
    #[arg(short, long, default_value_t = 1200)]
    ppi: u32,

    /// Output format (png or svg)
    #[arg(short, long, default_value = "png", value_enum)]
    format: Format,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
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

    fn render_formula(
        &self,
        formula: &str,
        inline: bool,
        format: Format,
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
            Format::Svg => typst_svg::svg(page).into_bytes(),
            Format::Png => {
                let pixel_width = (size.x.to_pt() * self.ppi as f64 / 72.0).round() as u32;
                let pixel_height = (size.y.to_pt() * self.ppi as f64 / 72.0).round() as u32;

                if pixel_width == 0 || pixel_height == 0 {
                    eprintln!(
                        "Warning: Rendered size for formula is zero. Skipping PNG encoding. Formula: {}",
                        formula
                    );
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

/// Renders formulas within HTML content and returns the modified HTML.
/// Takes an optional ProgressBar to update progress for batch processing.
fn render_formulas_in_html(
    file_name: &str,
    html_content: &str,
    ppi: f32,
    format: Format,
    pb_formulas: Option<&ProgressBar>,
) -> Result<String> {
    let document = Html::parse_document(html_content);
    let selector = Selector::parse("eq").expect("Invalid selector 'eq'"); // Panic 安全，因为选择器是硬编码的

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
        let formula_id = format!("__GLADST_FORMULA_PLACEHOLDER_{}__", formula_id_counter);

        if let Some(pos) = processed_html_string.find(&original_eq_html) {
            processed_html_string.replace_range(pos..pos + original_eq_html.len(), &formula_id);
        } else {
            eprintln!(
                "Warning: Could not find original HTML for formula '{}' during replacement.",
                formula
            );
        }

        formula_tasks.push((formula_id, formula, env));
    }

    if formula_tasks.is_empty() {
        return Ok(processed_html_string);
    }

    // Format the file name to be exactly 8 characters - safe version
    let display_name = if file_name.len() > 8 {
        // Truncate if longer than 8 characters
        &file_name[..8]
    } else {
        // Pad with spaces if shorter than 8 characters
        &format!("{:8}", file_name)
    };

    let local_pb;
    let pb_formulas_ref = match pb_formulas {
        Some(pb) => {
            pb.set_length(formula_tasks.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        &format!("    {{spinner:.green}} {} [{{bar:40.cyan/blue}}] {{pos}}/{{len}} ({{eta}})", display_name),
                    )
                    .context("Failed to set formula progress style")?
                    .progress_chars("#>-"),
            );
            pb.reset();
            pb
        }
        None => {
            local_pb = ProgressBar::new(formula_tasks.len() as u64);
            local_pb.set_style(
                ProgressStyle::default_bar()
                    .template(&format!(
                        "{{spinner:.green}} {} [{{bar:40.cyan/blue}}] {{pos}}/{{len}} ({{eta}})",
                        display_name
                    ))
                    .context("Failed to set formula progress style")?
                    .progress_chars("#>-"),
            );
            local_pb.enable_steady_tick(Duration::from_millis(100));
            &local_pb
        }
    };

    let processed_html_string_mutex = Arc::new(Mutex::new(processed_html_string));
    let render_errors = Arc::new(Mutex::new(Vec::<String>::new()));

    formula_tasks
        .into_par_iter()
        .for_each(|(formula_id, formula, env)| {
            let is_inline = match env.as_str() {
                "displaymath" => false,
                "math" | "" => true,
                _ => {
                    eprintln!(
                        "Warning: env '{}' is not recognized, defaulting to inline.",
                        env
                    );
                    true
                }
            };

            let renderer = RenderEngine::new(ppi);

            match renderer.render_formula(&formula, is_inline, format) {
                 Ok(result) => {
                    if !result.data.is_empty() {
                        let mime_type = match format {
                            Format::Svg => "image/svg+xml",
                            Format::Png => "image/png",
                        };
                        let b64 = general_purpose::STANDARD.encode(&result.data);
                        let formula_escaped = encode(&formula);

                        let replacement = format!(
                            r#"<img class="gladst {env}" style="width: {x_em:.4}em; height: {y_em:.4}em; vertical-align: middle;" src="data:{mime_type};base64,{b64}" alt="{formula_escaped}"/>"#,
                            env = env,
                            x_em = result.x_em,
                            y_em = result.y_em,
                            mime_type = mime_type,
                            b64 = b64,
                            formula_escaped = formula_escaped
                        );

                        let mut locked_string = processed_html_string_mutex.lock().unwrap();
                        *locked_string = locked_string.replacen(&formula_id, &replacement, 1);
                    } else if format == Format::Png {
                         eprintln!("Skipping replacement for formula due to zero render size: {}", formula);
                    }
                 }
                 Err(e) => {
                    let error_msg = format!("Error rendering formula '{}': {:?}", formula, e);
                    eprintln!("{}", error_msg);
                    render_errors.lock().unwrap().push(error_msg);
                     let error_replacement = format!(r#"<span style="color: red;" title="Render Error: {}">[{}]</span>"#, encode(&e.to_string()), formula);
                     let mut locked_string = processed_html_string_mutex.lock().unwrap();
                     *locked_string = locked_string.replacen(&formula_id, &error_replacement, 1);
                 }
            }


            pb_formulas_ref.inc(1);
        });

    if pb_formulas.is_none() {
        pb_formulas_ref.finish_and_clear();
    } else {
        pb_formulas_ref.finish();
    }

    let errors = render_errors.lock().unwrap();
    if !errors.is_empty() {
        eprintln!("\nEncountered {} formula rendering errors.", errors.len());
    }

    Arc::try_unwrap(processed_html_string_mutex)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap Mutex for processed HTML string"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("Mutex for processed HTML string was poisoned"))
}

fn needs_inplace_modification(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") | Some("htm") | Some("xhtml") => true,
        Some(_) => false,
        None => false,
    }
}

fn process_single_file(
    input_path: &Path,
    output_dir_option: Option<&Path>,
    ppi: f32,
    format: Format,
    pb_formulas: Option<&ProgressBar>,
) -> Result<()> {
    let input_content = fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read input file: {:?}", input_path))?;

    let processed_html = render_formulas_in_html(
        &input_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or("Formula".to_owned()),
        &input_content,
        ppi,
        format,
        pb_formulas,
    )?;

    let inplace = needs_inplace_modification(input_path);
    let output_path = if inplace {
        input_path.to_path_buf()
    } else {
        let output_base = output_dir_option
            .unwrap_or_else(|| input_path.parent().unwrap_or_else(|| Path::new(".")));
        fs::create_dir_all(output_base)
            .with_context(|| format!("Failed to create output directory: {:?}", output_base))?;

        let file_stem = input_path
            .file_stem()
            .ok_or_else(|| anyhow::anyhow!("Could not get file stem for {:?}", input_path))?;
        output_base.join(file_stem).with_extension("html")
    };

    fs::write(&output_path, processed_html)
        .with_context(|| format!("Failed to write output file: {:?}", output_path))?;

    if inplace {
        println!("Processed (in-place): {:?}", input_path);
    } else {
        println!("Processed: {:?} -> {:?}", input_path, output_path);
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let paths: Vec<PathBuf> = glob(&args.input)
        .with_context(|| format!("Failed to read glob pattern: {}", args.input))?
        .filter_map(Result::ok)
        .collect();

    if paths.is_empty() {
        println!("No files found matching pattern: {}", args.input);
        return Ok(());
    }

    let ppi_f32 = args.ppi as f32;
    let format = args.format;
    let output_dir = args.output.as_deref(); // Option<PathBuf> -> Option<&Path>

    if paths.len() == 1 {
        println!("Processing single file: {:?}", paths[0]);
        process_single_file(&paths[0], output_dir, ppi_f32, format, None)?;
    } else {
        println!("Processing {} files found by glob pattern...", paths.len());
        run_batch(&paths, output_dir, ppi_f32, format)?;
    }

    Ok(())
}

fn run_batch(
    paths: &[PathBuf],
    output_dir_option: Option<&Path>,
    ppi: f32,
    format: Format,
) -> Result<()> {
    let multi_progress = MultiProgress::new();

    let errors = Arc::new(Mutex::new(Vec::<(PathBuf, anyhow::Error)>::new()));

    paths.into_par_iter().for_each(|path| {
        let file_pb = multi_progress.add(ProgressBar::new(0));

        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        if let Err(e) = process_single_file(path, output_dir_option, ppi, format, Some(&file_pb)) {
            let error_record = (path.clone(), e);
            errors.lock().unwrap().push(error_record);
            file_pb.abandon_with_message(format!("Error processing {}", file_name));
        }
    });

    multi_progress
        .clear()
        .context("Failed to clear multi progress")?;

    let collected_errors = errors.lock().unwrap();
    if !collected_errors.is_empty() {
        eprintln!("\n* Batch Processing Errors");
        for (path, error) in collected_errors.iter() {
            eprintln!("**File: {:?}", path);
            eprintln!("***Error: {:?}", error);
        }
    }

    Ok(())
}
