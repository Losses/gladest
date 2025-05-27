# markdown-it-gladest

[![npm version](https://badge.fury.io/js/@fuuck%2Fmarkdown-it-gladest.svg)](https://www.npmjs.com/package/@fuuck/markdown-it-gladest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A [Markdown-it](https://github.com/markdown-it/markdown-it) plugin to render LaTeX mathematical formulas using the [Typst](https://typst.app/) typesetting system. It leverages a high-performance native Rust addon to generate SVG or PNG images from your math blocks.

## Features

- Renders inline (`$ ... $`) and block (`$$ ... $$`) LaTeX math.
- Powered by Typst for accurate and beautiful mathematical typesetting.
- Uses a native Rust addon for fast rendering.
- Outputs math as embedded SVG (default) or PNG images.
- Configurable PPI (Pixels Per Inch) for PNG output.
- Customizable fonts for body text and mathematical formulas.
- Easy integration with Markdown-it.
- Includes basic error handling display for rendering issues.

## Installation

```bash
npm install markdown-it @fuuck/markdown-it-gladest
# or
yarn add markdown-it @fuuck/markdown-it-gladest
# or
bun add markdown-it @fuuck/markdown-it-gladest
```

**Note:** This package relies on a native Node.js addon built with Rust. Pre-compiled binaries for common platforms (Windows, macOS, Linux) are typically provided. If a pre-compiled binary is not available for your specific platform/architecture, you might need a Rust toolchain installed to build the addon during installation.

## Usage

### Basic Usage

```javascript
import { writeFileSync } from "node:fs";
import MarkdownIt from "markdown-it";
import markdownItGladest from "@fuuck/markdown-it-gladest";

// Initialize Markdown-it
const md = new MarkdownIt().use(markdownItGladest, {
  // Plugin options (optional)
  format: "svg", // 'svg' (default) or 'png'
  ppi: 600, // Pixels Per Inch for PNG rendering (default: uses Typst default)
});

// Your Markdown content with LaTeX
const markdownContent = `
# Normal Distribution

The probability density function (PDF) is:

$$
f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}} e^{-\\frac{1}{2}\\left(\\frac{x-\\mu}{\\sigma}\\right)^2}
$$

Where $\\mu$ is the mean and $\\sigma$ is the standard deviation.
The standard normal distribution has $\\mu = 0$ and $\\sigma = 1$, denoted as $Z \\sim N(0,1)$.
An important transformation is standardization: $Z = \\frac{X-\\mu}{\\sigma}$.
`;

// Render the Markdown
const htmlResult = md.render(markdownContent);

// Save or use the result
writeFileSync("output.html", htmlResult);
console.log("Rendered HTML saved to output.html");

/*
Expected output in output.html (structure may vary slightly):

<h1>Normal Distribution</h1>
<p>The probability density function (PDF) is:</p>
<div class="gladst-block"><img src="data:image/svg+xml;base64,..." alt="f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}} e^{-\\frac{1}{2}\\left(\\frac{x-\\mu}{\\sigma}\\right)^2}" /></div>
<p>Where <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="\\mu" /></span> is the mean and <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="\\sigma" /></span> is the standard deviation.
The standard normal distribution has <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="\\mu = 0" /></span> and <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="\\sigma = 1" /></span>, denoted as <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="Z \\sim N(0,1)" /></span>.
An important transformation is standardization: <span class="gladst-inline"><img src="data:image/svg+xml;base64,..." alt="Z = \\frac{X-\\mu}{\\sigma}" /></span>.</p>
*/
```

### Font Configuration

You can customize fonts used for rendering mathematical formulas by specifying system fonts or font files:

#### Using System Fonts

```javascript
import MarkdownIt from "markdown-it";
import markdownItGladest from "@fuuck/markdown-it-gladest";

const md = new MarkdownIt().use(markdownItGladest, {
  format: "svg",
  fonts: {
    // Configure body font (used for text elements)
    bodyFont: {
      system: "Times New Roman",
    },
    // Configure math font (used for mathematical symbols)
    mathFont: {
      system: "Latin Modern Math",
    },
  },
});
```

#### Using Font Files

```javascript
import MarkdownIt from "markdown-it";
import markdownItGladest from "@fuuck/markdown-it-gladest";

const md = new MarkdownIt().use(markdownItGladest, {
  format: "svg",
  fonts: {
    // Use custom font files
    bodyFont: {
      file: "/path/to/your/custom-font.ttf",
    },
    mathFont: {
      file: "/path/to/your/math-font.otf",
    },
  },
});
```

#### Mixed Font Configuration

```javascript
import MarkdownIt from "markdown-it";
import markdownItGladest from "@fuuck/markdown-it-gladest";

const md = new MarkdownIt().use(markdownItGladest, {
  format: "png",
  ppi: 300,
  fonts: {
    // Use system font for body text
    bodyFont: {
      system: "Georgia",
    },
    // Use custom font file for math
    mathFont: {
      file: "/usr/share/fonts/opentype/lmodern/lmmath-regular.otf",
    },
  },
});

const content = `
Using custom fonts for better typography:

$$
\\sum_{i=1}^{n} \\frac{1}{i^2} = \\frac{\\pi^2}{6}
$$

The Euler's identity: $e^{i\\pi} + 1 = 0$.
`;

console.log(md.render(content));
```

## Options

You can pass an options object when enabling the plugin with `.use()`:

- **`format`**:

  - Type: `'svg' | 'png'`
  - Default: `'svg'`
  - Description: Specifies the output image format for the rendered math formulas. SVG is generally recommended for scalability and quality, while PNG might be needed for specific compatibility reasons.

- **`ppi`**:

  - Type: `number | null`
  - Default: `null` (uses the default PPI configured in the underlying Typst rendering engine)
  - Description: Sets the Pixels Per Inch for PNG rendering. Higher values result in larger, more detailed images. This option is ignored if `format` is `'svg'`. Invalid values (e.g., non-positive numbers) will also cause it to fall back to the default.

- **`fonts`**:

  - Type: `FontConfig | undefined`
  - Default: `undefined` (uses default Typst fonts)
  - Description: Font configuration object that allows customizing the fonts used for rendering.

  **FontConfig Properties:**

  - **`bodyFont`**: Font configuration for body text elements
    - **`system`**: Use a system font by name (string)
    - **`file`**: Use a font file by path (string)
  - **`mathFont`**: Font configuration for mathematical symbols
    - **`system`**: Use a system font by name (string)
    - **`file`**: Use a font file by path (string)

  **Important:** You cannot specify both `system` and `file` for the same font type. Choose one approach per font.

## Syntax

- **Inline Math**: Use single dollar signs (`$`). Example: `This is inline math: $E = mc^2$.`
- **Block Math**: Use double dollar signs (`$$`). These will be rendered as block elements (typically centered on their own line). Example:
  ```latex
  $$
  \int_a^b f(x) dx = F(b) - F(a)
  $$
  ```

The content between the delimiters is treated as LaTeX code and passed to Typst for rendering. Make sure to escape literal dollar signs in your Markdown text using a backslash: `\$`.

## How It Works

1.  **Parsing**: Markdown-it parses the input text. `markdown-it-gladest` registers inline and block rules to detect `$ ... $` and `$$ ... $$` sequences.
2.  **Tokenization**: When math delimiters are found, custom tokens (`gladst_inline_math`, `gladst_block_math`) are generated, containing the LaTeX code within them.
3.  **Rendering**: The plugin provides renderer functions for these custom tokens.
4.  **Native Call**: The renderer function calls the `renderLatex` function exported by the native Rust addon, passing the LaTeX code, the delimiter type (`$` or `$$`), and the configured options (`format`, `ppi`).
5.  **Typst Execution**: The Rust addon invokes the Typst library to parse the LaTeX input and render it into the desired format (SVG or PNG).
6.  **HTML Generation**: The Rust addon returns an HTML string, typically an `<img>` tag with the image data embedded as a Base64 data URI in the `src` attribute. If an error occurs during Typst rendering, an error message wrapped in a `<span>` or `<div>` is returned instead.
7.  **Output**: The plugin wraps the returned HTML in a `<span>` (for inline) or `<div>` (for block) with appropriate CSS classes (`gladst-inline` or `gladst-block`) and inserts it into the final HTML output generated by Markdown-it.

## Error Handling

If the Typst engine encounters an error while rendering a formula (e.g., invalid LaTeX syntax), the plugin will output an error message directly in the HTML instead of an image. This message will be wrapped in a `<span class="gladst-error-inline">` or `<div class="gladst-error-block">` and will often include the original formula and a summary of the error (potentially in the `title` attribute for hover details). Check your browser's developer console for more detailed error messages logged by the plugin.

## License

This package MIT licensed.
