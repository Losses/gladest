// src/index.cts
// This module is the CJS entry point for the library.

import type MarkdownIt from "markdown-it";
import type {
  Options as MarkdownItOptions,
  StateBlock,
  Token,
  Renderer,
} from "markdown-it";

// Import the native addon using the loader
import * as addon from "./load.cjs";

/**
 * Options for the gladst markdown-it plugin.
 */
interface GladstPluginOptions {
  /**
   * Output format for the rendered formula.
   * @default 'svg'
   */
  format?: "svg" | "png";

  /**
   * Pixels per inch (PPI) for PNG rendering.
   * If not specified or null, the default PPI of the Rust engine is used.
   */
  ppi?: number | null;
}

/**
 * Internal representation of parsed options passed to Rust.
 */
interface InternalRustOptions {
  format: "svg" | "png";
  ppi: number | null; // Pass null to Rust if not specified or invalid
}

// Use this declaration to assign types to the addon's exports.
// It tells TypeScript what functions are available on the object
// imported from './load.cjs'.
declare module "./load.cjs" {
  /**
   * Renders a LaTeX formula string into an HTML img tag (exported from Rust).
   * @param formula The LaTeX code (without delimiters).
   * @param delimiter The delimiter used ("$$" or "$$$").
   * @param options Rendering options (format, ppi).
   * @returns HTML string (<img> tag or error message/span).
   * @throws Will throw if serious internal error occurs in Rust/Neon layer,
   *         but typically returns error HTML string for rendering errors.
   */
  function renderLatex(
    formula: string,
    delimiter: string,
    options: InternalRustOptions // Use the specific internal options type
  ): string;

  // Add other functions exported by your Rust code here, if any.
  // e.g., function anotherRustFunction(arg: number): boolean;
}

function gladstPlugin(md: MarkdownIt, options?: GladstPluginOptions): void {
  // Prepare options to pass to the Rust function
  const internalOptions: InternalRustOptions = {
    format: options?.format === "png" ? "png" : "svg", // Default to svg
    ppi:
      typeof options?.ppi === "number" && options.ppi > 0 ? options.ppi : null,
  };

  function gladstBlockRule(
    state: StateBlock,
    startLine: number,
    endLine: number,
    silent: boolean
  ): boolean {
    let nextLine: number;
    const start = state.bMarks[startLine] + state.tShift[startLine];

    if (state.src.charCodeAt(start) !== 0x24 /* $ */) {
      return false;
    }

    let marker: string | null = null;
    if (state.src.startsWith("$$", start)) {
      marker = "$$";
    } else if (state.src.startsWith("$", start)) {
      marker = "$";
    } else {
      return false;
    }

    const markup = marker;

    nextLine = startLine;
    let found = false;
    const contentLines: string[] = [];

    while (nextLine < endLine) {
      nextLine++;
      if (nextLine >= endLine) break;

      const lineStart = state.bMarks[nextLine] + state.tShift[nextLine];
      const lineMax = state.eMarks[nextLine];
      const lineText = state.src.slice(lineStart, lineMax).trim();

      if (lineText === marker) {
        found = true;
        break;
      }
      contentLines.push(
        state.src.slice(state.bMarks[nextLine], state.eMarks[nextLine])
      );
    }

    if (!found) {
      return false;
    }

    if (silent) {
      return true;
    }

    const contentEndLine = nextLine;
    const content = contentLines.join("\n").trim();

    const token = state.push("gladst_render", "div", 0);
    token.markup = markup;
    token.content = content;
    token.info = marker;
    token.map = [startLine, contentEndLine + 1];
    token.block = true;

    state.line = contentEndLine + 1;

    return true;
  }

  // Register the block rule
  md.block.ruler.before("fence", "gladst_block", gladstBlockRule, {
    alt: ["paragraph", "reference", "blockquote", "list", "hr", "html_block"],
  });

  md.renderer.rules.gladst_render = (
    tokens: Token[],
    idx: number,
    _options: MarkdownItOptions,
    _env: unknown,
    _self: Renderer
  ): string => {
    const token = tokens[idx];
    const formula = token.content;
    const delimiter = token.info;

    if (!formula || !delimiter) {
      console.warn(
        "[markdown-it-gladst] Token missing content or info:",
        token
      );
      return `<div class="gladst-error-block">Internal Plugin Error: Token invalid</div>`;
    }

    try {
      // *** Use the imported addon directly ***
      const htmlOutput = addon.renderLatex(formula, delimiter, internalOptions);
      return htmlOutput;
    } catch (error: unknown) {
      // This catches errors thrown *by the Neon layer or addon loading*,
      // not usually formula rendering errors (which return error HTML).
      const safeFormula = formula.replace(/</g, "<").replace(/>/g, ">");
      const safeError =
        error instanceof Error
          ? error.message.replace(/</g, "<").replace(/>/g, ">")
          : "Unknown render execution error";
      console.error(
        `[markdown-it-gladst] Critical error calling native renderLatex for formula:\n${formula}\n`,
        error
      );
      return `<div class="gladst-error-block" title="${safeError.replace(
        /"/g,
        '"'
      )}">Critical Error rendering block: ${delimiter}${safeFormula}${delimiter}</div>`;
    }
  };
}

export default gladstPlugin;
