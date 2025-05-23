# Gladest

**Gladest** is a Typst-based implementation inspired by GladTeX, designed to seamlessly convert LaTeX formulas embedded in HTML files into rasterized (PNG) or vector (SVG) images. These images are then embedded directly into the output HTML as Base64-encoded data, eliminating the need for external image files. Built to integrate with Pandoc's GladTeX workflow, Gladest distinguishes itself by being a self-contained tool that requires no external Typst or LaTeX installations. It also boasts enhanced rendering performance, parallel formula processing capabilities, and support for rendering CJK (Chinese, Japanese, Korean) characters within formulas—setting it apart from GladTeX.

## Features

- **Self-contained**: No need to install Typst, LaTeX, or additional dependencies.
- **High Performance**: Utilizes parallel rendering for faster processing of formulas.
- **Flexible Output**: Renders formulas as PNG (raster) or SVG (vector) images.
- **Base64 Embedding**: Embeds images directly into HTML, ensuring portability.
- **Pandoc Integration**: Designed to complement GladTeX workflows for Markdown/LaTeX to HTML conversion.
- **CJK Support**: Renders formulas containing Chinese, Japanese, and Korean characters, unlike GladTeX.

## Usage

### Basic Command
```bash
gladst input.htex -o output_dir --format svg --ppi 1200
```

### Arguments and Options
| Argument/Option       | Description                                                                 |
|-----------------------|-----------------------------------------------------------------------------|
| `<INPUT>`             | Path to the input `.htex` file (required).                                  |
| `-o, --output <DIR>`  | Output directory for the processed HTML and assets (required).              |
| `-f, --format <FMT>`  | Output format: `png` (default) or `svg`.                                    |
| `-p, --ppi <PPI>`     | Pixels per inch for rasterization (PNG only). Default: `1200`.              |
| `-h, --help`          | Print help message.                                                         |
| `-V, --version`       | Display version information.                                                |

#### Example
To process an `.htex` file and output SVG images:
```bash
gladst document.htex -o ./output --format svg
```
For high-resolution PNG output:
```bash
gladst document.htex -o ./output --format png --ppi 2000
```

## How It Works
1. **Input Processing**: Parses the input `.htex` file (generated by Pandoc + GladTeX) to extract LaTeX formulas, including those with CJK characters.
2. **Rendering**: Uses Typst's `mitex` package to render each formula to PNG or SVG. Formulas are processed in parallel using multi-threading for optimal performance.
3. **Embedding**: Converts rendered images to Base64 strings and injects them into the final HTML using `<img>` tags with `data:image` sources.
4. **Output**: Saves the standalone HTML file with embedded images to the specified directory, ensuring no external references.

## Why Gladest?
Compared to GladTeX:
- ✅ **No Dependencies**: Bundles all required components; no LaTeX or Typst installations needed.
- ⚡ **Faster**: Parallel rendering drastically reduces processing time for documents with many formulas.
- 🖼️ **Modern Output**: Choose between high-resolution PNGs (up to 2400 PPI) or scalable SVGs.
- 📦 **Self-Contained HTML**: Base64-embedded images mean no external asset management.
- 🌏 **CJK Support**: Handles formulas with Chinese, Japanese, and Korean characters seamlessly.

## Limitations
- Currently supports only the subset of LaTeX supported by Typst’s `mitex` package (e.g., advanced LaTeX macros may not render correctly).
- SVG output may not perfectly match LaTeX’s exact typography in rare edge cases due to differences in rendering engines.
- PNG output relies on `width` and `height` styles measured in `em` units, which may not be supported by all readers, particularly those with custom rendering engines. This choice balances visual consistency and compatibility.

## Developer Notes
- **Font Size in `template.typ`**: The `templates` directory contains a `template.typ` file with a hardcoded font size. Developers should not modify this value, as it ensures proper alignment between Typst’s internal units and the HTML rendering output.
- **Custom Fonts**: Developers can add support for additional fonts (e.g., for specific languages) by modifying the source code and recompiling. Refer to the source for guidance on font integration.
- **PNG Sizing**: For PNG output, formula dimensions are constrained using `width` and `height` attributes in the `style` tag, measured in `em` units. While this approach optimizes visual fidelity and compatibility, it may not work perfectly in readers with non-standard rendering engines. After extensive testing, this was deemed the best trade-off.
