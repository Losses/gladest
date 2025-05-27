use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, arg, command};
use clap_derive::{Parser, ValueEnum};
use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use scraper::{Html, Selector};

use gladest_engine::{FontConfig, FontSource, RenderEngine, RenderFormat};

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

    /// Body font file path
    #[arg(long, help = "Path to body font file (e.g., serif.ttf)")]
    body_font_file: Option<String>,

    /// Body font name (system font)
    #[arg(long, help = "System body font name (e.g., 'Times New Roman')")]
    body_font_name: Option<String>,

    /// Math font file path
    #[arg(long, help = "Path to math font file (e.g., math.otf)")]
    math_font_file: Option<String>,

    /// Math font name (system font)
    #[arg(long, help = "System math font name (e.g., 'STIX Two Math')")]
    math_font_name: Option<String>,

    /// Show verbose error output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Png,
    Svg,
}

#[derive(Debug)]
struct FormulaError {
    formula: String,
    error: anyhow::Error,
    formula_index: usize,
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = env::var("HOME") {
            path.replacen("~", &home, 1)
        } else {
            path.to_string()
        }
    } else if path == "~" {
        env::var("HOME").unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

/// Create font configuration from command line arguments
fn create_font_config(args: &Args) -> Result<FontConfig> {
    let body_font = match (&args.body_font_file, &args.body_font_name) {
        (Some(file), None) => {
            let expanded_path = expand_tilde(file);
            let expanded_file = PathBuf::from(&expanded_path);

            if !expanded_file.exists() {
                return Err(anyhow::anyhow!(
                    "Body font file does not exist: {:?}",
                    expanded_path
                ));
            }
            FontSource::File(expanded_path)
        }
        (None, Some(name)) => FontSource::System(name.clone()),
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!(
                "Cannot specify both body font file and body font name. Choose one."
            ));
        }
        (None, None) => FontSource::System("serif".to_string()), // Default
    };

    let math_font = match (&args.math_font_file, &args.math_font_name) {
        (Some(file), None) => {
            let expanded_path = expand_tilde(file);
            let expanded_file = PathBuf::from(&expanded_path);

            if !expanded_file.exists() {
                return Err(anyhow::anyhow!(
                    "Math font file does not exist: {:?}",
                    expanded_path
                ));
            }
            FontSource::File(expanded_path)
        }
        (None, Some(name)) => FontSource::System(name.clone()),
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!(
                "Cannot specify both math font file and math font name. Choose one."
            ));
        }
        (None, None) => FontSource::System("Fira Math".to_string()), // Default
    };

    Ok(FontConfig {
        body_font,
        math_font,
        include_system_fonts: args.body_font_name.is_some() || args.math_font_name.is_some(),
        include_embedded_fonts: args.body_font_file.is_some() || args.math_font_file.is_some(),
    })
}

/// Extract detailed error information from anyhow::Error chain
fn extract_detailed_error(error: &anyhow::Error) -> String {
    let mut error_parts = Vec::new();
    
    // Get the main error message
    error_parts.push(format!("Main error: {}", error));
    
    // Walk through the error chain
    let mut current = error.source();
    let mut depth = 1;
    while let Some(err) = current {
        error_parts.push(format!("  Cause {}: {}", depth, err));
        current = err.source();
        depth += 1;
    }
    
    error_parts.join("\n")
}

/// Format formula error for display
fn format_formula_error(formula_error: &FormulaError, verbose: bool) -> String {
    let formula_preview = if formula_error.formula.len() > 100 {
        format!("{}...", &formula_error.formula[..97])
    } else {
        formula_error.formula.clone()
    };

    let mut output = String::new();
    output.push_str(&format!(
        "❌ Formula #{} failed to render:\n",
        formula_error.formula_index + 1
    ));
    output.push_str(&format!("   Formula: {}\n", formula_preview));
    
    if verbose {
        output.push_str("   Error details:\n");
        for line in extract_detailed_error(&formula_error.error).lines() {
            output.push_str(&format!("     {}\n", line));
        }
    } else {
        output.push_str(&format!("   Error: {}\n", formula_error.error));
    }
    
    output
}

/// Renders formulas within HTML content and returns the modified HTML.
/// Takes an optional ProgressBar ONLY for the single-file case to update formula progress.
fn render_formulas_in_html(
    html_content: &str,
    ppi: f32,
    format: Format,
    font_config: &FontConfig,
    pb_formulas: Option<&ProgressBar>,
) -> Result<(String, Vec<FormulaError>)> {
    let document = Html::parse_document(html_content);
    let selector = Selector::parse("eq").expect("Invalid selector 'eq'");

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
        }

        formula_tasks.push((formula_id, formula, env, formula_id_counter));
    }

    if formula_tasks.is_empty() {
        return Ok((processed_html_string, Vec::new()));
    }

    if let Some(pb) = pb_formulas {
        pb.set_length(formula_tasks.len() as u64);
        pb.reset();
    }

    let processed_html_string_mutex = Arc::new(Mutex::new(processed_html_string));
    let formula_errors = Arc::new(Mutex::new(Vec::<FormulaError>::new()));

    // Create render engine once with the configured fonts
    let renderer = RenderEngine::with_font_config(font_config.clone());

    formula_tasks
        .into_par_iter()
        .for_each(|(formula_id, formula, env, formula_index)| {
            let is_inline = match env.as_str() {
                "displaymath" => false,
                "math" | "" => true,
                _ => true,
            };

            match renderer.render_formula(
                &formula,
                is_inline,
                match format {
                    Format::Png => RenderFormat::Png,
                    Format::Svg => RenderFormat::Svg,
                },
                Some(ppi),
            ) {
                Ok(result) => {
                    if !result.data.is_empty() {
                        let replacement = result.to_html();

                        let mut locked_string = processed_html_string_mutex.lock().unwrap();
                        *locked_string = locked_string.replacen(&formula_id, &replacement, 1);
                    }
                }
                Err(e) => {
                    // Store the error for later reporting
                    formula_errors.lock().unwrap().push(FormulaError {
                        formula: formula.clone(),
                        error: e,
                        formula_index,
                    });
                    
                    // Create error replacement in HTML
                    let error_replacement = format!(
                        r#"<span style="color: red; background-color: #ffe6e6; padding: 2px 4px; border-radius: 3px;" title="Formula render error - see logs for details">[Formula Error #{}: {}]</span>"#,
                        formula_index + 1,
                        if formula.len() > 20 { format!("{}...", &formula[..17]) } else { formula }
                    );
                    let mut locked_string = processed_html_string_mutex.lock().unwrap();
                    *locked_string = locked_string.replacen(&formula_id, &error_replacement, 1);
                }
            }

            if let Some(pb) = pb_formulas {
                pb.inc(1);
            }
        });

    let final_html = Arc::try_unwrap(processed_html_string_mutex)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap Mutex for processed HTML string"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("Mutex for processed HTML string was poisoned"))?;

    let mut errors = Arc::try_unwrap(formula_errors)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap Mutex for formula errors"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("Mutex for formula errors was poisoned"))?;

    // Sort errors by formula index for consistent output
    errors.sort_by_key(|e| e.formula_index);

    Ok((final_html, errors))
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
    font_config: &FontConfig,
    pb_formulas: Option<&ProgressBar>,
    verbose: bool,
) -> Result<()> {
    let input_content = fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read input file: {:?}", input_path))?;

    let (processed_html, formula_errors) =
        render_formulas_in_html(&input_content, ppi, format, font_config, pb_formulas)?;

    // Report formula errors if any
    if !formula_errors.is_empty() {
        println!("\n⚠️  Formula Rendering Errors in {:?}:", input_path);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        for formula_error in &formula_errors {
            print!("{}", format_formula_error(formula_error, verbose));
        }
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Summary: {} out of {} formulas failed to render", 
                formula_errors.len(), 
                processed_html.matches("__GLADST_FORMULA_PLACEHOLDER_").count() + formula_errors.len());
        if !verbose {
            println!("💡 Use --verbose flag to see detailed error information");
        }
        println!();
    }

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

    Ok(())
}

fn print_font_config(font_config: &FontConfig) {
    println!("🔤 Font Configuration:");
    match &font_config.body_font {
        FontSource::System(name) => println!("  📝 Body Font: {} (system)", name),
        FontSource::File(path) => println!("  📝 Body Font: {} (file)", path),
        FontSource::Data(_) => println!("  📝 Body Font: embedded data"),
    }
    match &font_config.math_font {
        FontSource::System(name) => println!("  🔢 Math Font: {} (system)", name),
        FontSource::File(path) => println!("  🔢 Math Font: {} (file)", path),
        FontSource::Data(_) => println!("  🔢 Math Font: embedded data"),
    }
    println!();
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Create font configuration
    let font_config = create_font_config(&args).context("Failed to create font configuration")?;

    let paths: Vec<PathBuf> = glob(&expand_tilde(&args.input))
        .with_context(|| format!("Failed to read glob pattern: {}", args.input))?
        .filter_map(Result::ok)
        .collect();

    if paths.is_empty() {
        println!("❌ No files found matching pattern: {}", args.input);
        return Ok(());
    }

    let ppi_f32 = args.ppi as f32;
    let format = args.format;
    let output_dir = args.output.as_deref();
    let verbose = args.verbose;

    // Print font configuration
    print_font_config(&font_config);

    if paths.len() == 1 {
        println!("📄 Processing single file: {:?}", paths[0]);
        let formula_pb = ProgressBar::new(0);
        formula_pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "    {spinner:.green} Formulas: [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}",
                )
                .context("Failed to set formula progress style for single file")?
                .progress_chars("#>-"),
        );
        formula_pb.enable_steady_tick(Duration::from_millis(100));

        process_single_file(
            &paths[0],
            output_dir,
            ppi_f32,
            format,
            &font_config,
            Some(&formula_pb),
            verbose,
        )?;

        formula_pb.finish_and_clear();
    } else {
        println!("📁 Processing {} files found by glob pattern...", paths.len());
        run_batch(&paths, output_dir, ppi_f32, format, &font_config, verbose)?;
        println!("✅ Batch processing complete.");
    }

    Ok(())
}

fn run_batch(
    paths: &[PathBuf],
    output_dir_option: Option<&Path>,
    ppi: f32,
    format: Format,
    font_config: &FontConfig,
    verbose: bool,
) -> Result<()> {
    let multi_progress = MultiProgress::new();
    let files_pb = multi_progress.add(ProgressBar::new(paths.len() as u64));
    files_pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} Files: [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .context("Failed to set files progress style")?
            .progress_chars("#>-"),
    );
    files_pb.set_message("Starting...");

    let errors = Arc::new(Mutex::new(Vec::<(PathBuf, anyhow::Error)>::new()));

    paths.into_par_iter().for_each(|path| {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        files_pb.set_message(format!("Processing: {}", file_name));

        if let Err(e) = process_single_file(path, output_dir_option, ppi, format, font_config, None, verbose)
        {
            let error_record = (
                path.clone(),
                e.context(format!("Processing failed for file: {:?}", path)),
            );
            errors.lock().unwrap().push(error_record);
        }
        files_pb.inc(1);
    });

    files_pb.finish_with_message("All files processed.");

    let collected_errors = Arc::try_unwrap(errors)
        .expect("Mutex should not be locked after parallel processing")
        .into_inner()
        .expect("Mutex should not be poisoned");

    if !collected_errors.is_empty() {
        println!("\n❌ Batch Processing Errors ({}):", collected_errors.len());
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        for (i, (path, error)) in collected_errors.iter().enumerate() {
            println!("🗂️  File #{}: {:?}", i + 1, path);
            if verbose {
                for line in extract_detailed_error(error).lines() {
                    println!("   {}", line);
                }
            } else {
                println!("   Error: {}", error);
            }
            println!();
        }
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Finished with {} file processing errors.", collected_errors.len());
        if !verbose {
            println!("💡 Use --verbose flag to see detailed error information");
        }
    }

    Ok(())
}