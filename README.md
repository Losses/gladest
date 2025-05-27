# Gladest

**Gladest** is a Typst-based implementation inspired by GladTeX, designed to seamlessly convert LaTeX formulas embedded in HTML files into rasterized (PNG) or vector (SVG) images. These images are then embedded directly into the output HTML as Base64-encoded data, eliminating the need for external image files. Built to integrate with Pandoc's GladTeX workflow, Gladest distinguishes itself by being a self-contained tool that requires no external Typst or LaTeX installations. It also boasts enhanced rendering performance, parallel formula processing capabilities, and support for rendering CJK (Chinese, Japanese, Korean) characters within formulas—setting it apart from GladTeX.

## Features

- **Self-contained**: No need to install Typst, LaTeX, or additional dependencies.
- **High Performance**: Utilizes parallel rendering for faster processing of formulas.
- **Flexible Output**: Renders formulas as PNG (raster) or SVG (vector) images.
- **Base64 Embedding**: Embeds images directly into HTML, ensuring portability.
- **Pandoc Integration**: Designed to complement GladTeX workflows for Markdown/LaTeX to HTML conversion.
- **CJK Support**: Renders formulas containing Chinese, Japanese, and Korean characters, unlike GladTeX.
- **Custom Font Support**: Allows users to specify custom fonts for both body text and mathematical expressions.

## Usage

### Basic Command

```bash
gladst input.htex -o output_dir --format svg --ppi 1200
```

### Font Configuration

Gladest supports flexible font configuration through command-line options:

#### Using System Fonts

```bash
# Specify system fonts by name
gladst input.htex -o output_dir --body-font-name "Times New Roman" --math-font-name "STIX Two Math"

# Use default fonts (serif for body, Fira Math for mathematics)
gladst input.htex -o output_dir
```

#### Using Font Files

```bash
# Specify font files directly
gladst input.htex -o output_dir --body-font-file fonts/serif.ttf --math-font-file fonts/math.otf

# Mix system fonts and font files
gladst input.htex -o output_dir --body-font-name "Georgia" --math-font-file fonts/custom-math.otf
```

### Batch Processing

Process multiple files with glob patterns:

```bash
# Process all HTML files in a directory
gladst "docs/*.html" --body-font-name "Source Serif Pro" --math-font-name "Latin Modern Math"

# Process files recursively
gladst "docs/**/*.htex" -o output --format svg
```

### Arguments and Options

| Argument/Option           | Description                                                            |
| ------------------------- | ---------------------------------------------------------------------- |
| `<INPUT>`                 | Path to input file or glob pattern (e.g., `*.html`, `docs/**/*.htex`). |
| `-o, --output <DIR>`      | Output directory for processed files (only used for .htex inputs).     |
| `-f, --format <FMT>`      | Output format: `png` (default) or `svg`.                               |
| `-p, --ppi <PPI>`         | Pixels per inch for rasterization (PNG only). Default: `1200`.         |
| `--body-font-file <PATH>` | Path to body font file (e.g., `serif.ttf`).                            |
| `--body-font-name <NAME>` | System body font name (e.g., `Times New Roman`).                       |
| `--math-font-file <PATH>` | Path to math font file (e.g., `math.otf`).                             |
| `--math-font-name <NAME>` | System math font name (e.g., `STIX Two Math`).                         |
| `-h, --help`              | Print help message.                                                    |
| `-V, --version`           | Display version information.                                           |

#### Examples

Process an `.htex` file with custom fonts and SVG output:

```bash
gladst document.htex -o ./output --format svg --body-font-name "Crimson Text" --math-font-name "Fira Math"
```

For high-resolution PNG output with font files:

```bash
gladst document.htex -o ./output --format png --ppi 2000 --body-font-file fonts/charter.otf --math-font-file fonts/stix.otf
```

Process multiple HTML files in-place with system fonts:

```bash
gladst "*.html" --body-font-name "Source Serif Pro" --math-font-name "Latin Modern Math"
```

## Font Configuration

### Font Sources

Gladest supports three types of font sources:

1. **System Fonts**: Use fonts installed on your system by specifying their names
2. **Font Files**: Load fonts from specific file paths

### Font Types

- **Body Font**: Used for regular text content in formulas
- **Math Font**: Used specifically for mathematical symbols and expressions

### Font Validation

- Font files are validated for existence before processing begins
- Cannot specify both file and system font name for the same font type

### Recommended Font Combinations

#### For Academic Documents

```bash
--body-font-name "Times New Roman" --math-font-name "STIX Two Math"
--body-font-name "Source Serif Pro" --math-font-name "Latin Modern Math"
```

#### For Modern Web Documents

```bash
--body-font-name "Source Serif Pro" --math-font-name "Fira Math"
--body-font-name "Crimson Text" --math-font-name "TeX Gyre Termes Math"
```

#### For CJK Documents

```bash
--body-font-name "Noto Serif SC" --math-font-name "STIX Two Math"
--body-font-file fonts/SimSun.ttf --math-font-name "Cambria Math"
```

## How It Works

1. **Font Configuration**: Validates and loads specified fonts (system fonts or font files) for body text and mathematical expressions.
2. **Input Processing**: Parses input files to extract LaTeX formulas, including those with CJK characters.
3. **Template Generation**: Dynamically generates Typst templates with the configured fonts.
4. **Rendering**: Uses Typst's `mitex` package to render each formula with custom fonts. Formulas are processed in parallel using multi-threading for optimal performance.
5. **Embedding**: Converts rendered images to Base64 strings and injects them into the final HTML using `<img>` tags with `data:image` sources.
6. **Output**: Saves the processed files with embedded images, ensuring no external references.

## Why Gladest?

Compared to GladTeX:

- ✅ **No Dependencies**: Bundles all required components; no LaTeX or Typst installations needed.
- ⚡ **Faster**: Parallel rendering drastically reduces processing time for documents with many formulas.
- 🖼️ **Modern Output**: Choose between high-resolution PNGs (up to 2400 PPI) or scalable SVGs.
- 📦 **Self-Contained HTML**: Base64-embedded images mean no external asset management.
- 🌏 **CJK Support**: Handles formulas with Chinese, Japanese, and Korean characters seamlessly.
- 🎨 **Custom Fonts**: Full control over typography with support for system fonts and font files.
- 📁 **Batch Processing**: Process multiple files efficiently with glob pattern support.

## Limitations

- Currently supports only the subset of LaTeX supported by Typst's `mitex` package (e.g., advanced LaTeX macros may not render correctly).
- SVG output may not perfectly match LaTeX's exact typography in rare edge cases due to differences in rendering engines.
- PNG output relies on `width` and `height` styles measured in `em` units, which may not be supported by all readers, particularly those with custom rendering engines. This choice balances visual consistency and compatibility.
- Font file validation occurs only at startup; corrupted or invalid font files may cause runtime errors during rendering.

## Developer Notes

- **Custom Fonts**: Developers can easily extend font support by modifying the `FontSource` enum and related configuration logic. The architecture supports adding new font source types (e.g., embedded font data, remote fonts).
- **PNG Sizing**: For PNG output, formula dimensions are constrained using `width` and `height` attributes in the `style` tag, measured in `em` units. While this approach optimizes visual fidelity and compatibility, it may not work perfectly in readers with non-standard rendering engines. After extensive testing, this was deemed the best trade-off.
- **Performance**: The rendering engine creates font-configured instances once per processing session, avoiding the overhead of repeated font loading in parallel contexts.
