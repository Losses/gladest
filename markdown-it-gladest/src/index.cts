import type MarkdownIt from "markdown-it";
import type {
  Options as MarkdownItOptions,
  StateBlock,
  StateInline,
  Token,
  Renderer,
} from "markdown-it";

// Import the native addon using the loader
import * as addon from "./load.cjs";

/**
 * Font source configuration
 */
interface FontSource {
  /** Use a system font by name */
  system?: string;
  /** Use a font file by path */
  file?: string;
}

/**
 * Font configuration for rendering
 */
interface FontConfig {
  /** Body font configuration */
  bodyFont?: FontSource;
  /** Math font configuration */
  mathFont?: FontSource;
}

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

  /**
   * Font configuration for rendering
   */
  fonts?: FontConfig;
}

/**
 * Internal representation of parsed options passed to Rust.
 */
interface InternalRustOptions {
  format: "svg" | "png";
  ppi: number | null;
  fonts?: {
    bodyFont?: {
      type: "system" | "file";
      value: string;
    };
    mathFont?: {
      type: "system" | "file";
      value: string;
    };
    // Note: includeSystemFonts is automatically inferred by Rust, not passed from JS
  };
}

// Use this declaration to assign types to the addon's exports.
declare module "./load.cjs" {
  /**
   * Renders a LaTeX formula string into an HTML img tag (exported from Rust).
   * @param formula The LaTeX code (without delimiters).
   * @param delimiter The delimiter used ("$$" or "$").
   * @param options Rendering options (format, ppi, fonts).
   * @returns HTML string (<img> tag or error message/span).
   */
  function renderLatex(
    formula: string,
    delimiter: string,
    options: InternalRustOptions
  ): string;

  /**
   * Updates the global font configuration for all subsequent renders.
   * @param fontConfig Font configuration object
   * @returns boolean indicating success
   */
  function setFontConfig(fontConfig: InternalRustOptions["fonts"]): boolean;
}

// Block Rule for $$
function gladstBlockRule(
  state: StateBlock,
  startLine: number,
  endLine: number,
  silent: boolean
): boolean {
  const startMarker = "$$";
  const endMarker = "$$";
  const pos = state.bMarks[startLine] + state.tShift[startLine];
  const max = state.eMarks[startLine];

  if (!state.src.startsWith(startMarker, pos)) {
    return false;
  }

  // Check for quick end on the same line: $$ formula $$
  const firstLineEndMarkerPos = state.src.indexOf(
    endMarker,
    pos + startMarker.length
  );
  if (
    firstLineEndMarkerPos !== -1 &&
    firstLineEndMarkerPos <= max - endMarker.length
  ) {
    if (
      firstLineEndMarkerPos === pos + startMarker.length &&
      state.src.slice(
        pos + startMarker.length,
        pos + startMarker.length + endMarker.length
      ) === endMarker
    ) {
      // Handle $$$$ case or similar - treat as empty? Maybe okay.
    }

    if (silent) {
      return true;
    }

    const content = state.src
      .slice(pos + startMarker.length, firstLineEndMarkerPos)
      .trim();
    const token = state.push("gladst_block_math", "div", 0);
    token.block = true;
    token.content = content;
    token.markup = startMarker;
    token.map = [startLine, startLine + 1];
    state.line = startLine + 1;
    return true;
  }

  // Multi-line block: search for ending '$$' on subsequent lines
  let nextLine = startLine + 1;
  const contentLines: string[] = [];
  contentLines.push(state.src.slice(pos + startMarker.length, max).trim()); // Rest of the first line

  while (nextLine < endLine) {
    const lineStart = state.bMarks[nextLine] + state.tShift[nextLine];
    const lineMax = state.eMarks[nextLine];
    const lineText = state.src.slice(lineStart, lineMax).trim(); // Trim for easy comparison

    // Check if the entire line is the end marker
    if (lineText === endMarker) {
      if (silent) {
        return true;
      }
      // Found the end marker on its own line
      const token = state.push("gladst_block_math", "div", 0);
      token.block = true;
      token.content = contentLines.join("\n").trim();
      token.markup = startMarker;
      token.map = [startLine, nextLine + 1];
      state.line = nextLine + 1;
      return true; // <-- Exit: Found end marker
    }

    // Check if the end marker is at the *end* of the current line
    if (lineText.endsWith(endMarker)) {
      if (silent) {
        return true;
      }
      // Found end marker at the end of the current line
      contentLines.push(
        lineText.slice(0, lineText.length - endMarker.length).trim()
      ); // Add content before marker
      const token = state.push("gladst_block_math", "div", 0);
      token.block = true;
      token.content = contentLines.join("\n").trim();
      token.markup = startMarker;
      token.map = [startLine, nextLine + 1];
      state.line = nextLine + 1;
      return true; // <-- Exit: Found end marker
    }

    // If neither 'if' above returned, it's a content line
    contentLines.push(
      state.src.slice(state.bMarks[nextLine], state.eMarks[nextLine])
    ); // Keep original spacing/indent
    nextLine++;
  }

  // End marker not found after checking all lines
  return false;
}

// Inline Rule for $
function gladstInlineRule(state: StateInline, silent: boolean): boolean {
  const startMarker = "$";
  const endMarker = "$";
  const pos = state.pos;

  if (state.src.charCodeAt(pos) !== 0x24 /* $ */) {
    return false;
  }

  const prevChar = pos > 0 ? state.src.charCodeAt(pos - 1) : -1;
  if (prevChar === 0x5c /* \ */) {
    return false;
  } // Escaped dollar
  if (state.src.charCodeAt(pos + 1) === 0x24 /* $ */) {
    return false;
  } // Let block rule handle $$

  let foundClosing = false;
  let endPos = -1;
  let currentPos = pos + startMarker.length;

  while (currentPos < state.posMax) {
    const charCode = state.src.charCodeAt(currentPos);

    // Check for closing marker first
    if (charCode === 0x24 /* $ */) {
      const nextChar =
        currentPos + 1 < state.posMax
          ? state.src.charCodeAt(currentPos + 1)
          : -1;
      if (nextChar === 0x24) {
        // Part of $$
        currentPos++; // Skip the second '$'
        // Continue loop normally to handle potential content after $$
      } else {
        // Found valid closing $
        foundClosing = true;
        endPos = currentPos;
        break; // Exit the while loop
      }
    }
    // Check for escaped characters
    else if (charCode === 0x5c /* \ */) {
      currentPos++; // Skip the escaped character as well
      // Continue loop normally
    }
    // Check for newline (disallow inline math spanning lines)
    else if (charCode === 0x0a /* \n */) {
      break; // Exit the while loop, closing marker not found on this line
    }

    // If none of the special conditions caused a break or modification,
    // just advance the position.
    currentPos++;
  } // End of while loop

  if (!foundClosing || endPos < 0) {
    return false; // Closing marker not found
  }

  // We found a valid sequence
  if (silent) {
    return true;
  }

  const content = state.src.slice(pos + startMarker.length, endPos).trim();
  if (!content) {
    return false; // Reject empty formulas like '$ $'
  }

  const token = state.push("gladst_inline_math", "span", 0);
  token.markup = startMarker;
  token.content = content;

  state.pos = endPos + endMarker.length; // Move position past the closing '$'
  return true;
}

/**
 * Validates and normalizes font configuration
 */
function normalizeFontConfig(
  fonts?: FontConfig
): InternalRustOptions["fonts"] | undefined {
  if (!fonts) return undefined;

  const result: NonNullable<InternalRustOptions["fonts"]> = {};

  // Process body font
  if (fonts.bodyFont) {
    if (fonts.bodyFont.system && fonts.bodyFont.file) {
      throw new Error("Cannot specify both system and file for body font");
    }
    if (fonts.bodyFont.system) {
      result.bodyFont = { type: "system", value: fonts.bodyFont.system };
    } else if (fonts.bodyFont.file) {
      result.bodyFont = { type: "file", value: fonts.bodyFont.file };
    }
  }

  // Process math font
  if (fonts.mathFont) {
    if (fonts.mathFont.system && fonts.mathFont.file) {
      throw new Error("Cannot specify both system and file for math font");
    }
    if (fonts.mathFont.system) {
      result.mathFont = { type: "system", value: fonts.mathFont.system };
    } else if (fonts.mathFont.file) {
      result.mathFont = { type: "file", value: fonts.mathFont.file };
    }
  }

  return Object.keys(result).length > 0 ? result : undefined;
}

function gladstPlugin(md: MarkdownIt, options?: GladstPluginOptions): void {
  // Prepare options to pass to the Rust function
  const internalOptions: InternalRustOptions = {
    format: options?.format === "png" ? "png" : "svg", // Default to svg
    ppi:
      typeof options?.ppi === "number" && options.ppi > 0 ? options.ppi : 300,
    fonts: normalizeFontConfig(options?.fonts),
  };

  // Set global font configuration if provided
  if (internalOptions.fonts) {
    try {
      const success = addon.setFontConfig(internalOptions.fonts);
      if (!success) {
        console.warn("[markdown-it-gladst] Failed to set font configuration");
      }
    } catch (error) {
      console.error(
        "[markdown-it-gladst] Error setting font configuration:",
        error
      );
    }
  }

  // Common rendering logic (extracted)
  function renderFormula(
    formula: string,
    delimiter: string,
    isBlock: boolean
  ): string {
    if (!formula || !delimiter) {
      const type = isBlock ? "Block" : "Inline";
      console.warn(
        `[markdown-it-gladst] ${type} token missing content or info.`
      );
      const tag = isBlock ? "div" : "span";
      return `<${tag} class="gladst-error-${
        isBlock ? "block" : "inline"
      }">Internal Plugin Error: Token invalid</${tag}>`;
    }

    try {
      const htmlOutput = addon.renderLatex(formula, delimiter, internalOptions);
      // Wrap the output from Rust (assuming it's just the core like <img> or error span)
      const wrapperTag = isBlock ? "div" : "span";
      const wrapperClass = `gladst-${isBlock ? "block" : "inline"}`;
      // Simple check if Rust already returned an error structure
      if (
        htmlOutput.startsWith('<span class="gladst-error') ||
        htmlOutput.startsWith('<div class="gladst-error')
      ) {
        return htmlOutput; // Return Rust error directly
      }
      // Wrap successful render
      return `<${wrapperTag} class="${wrapperClass}">${htmlOutput}</${wrapperTag}>`;
    } catch (error: unknown) {
      const safeFormula = formula.replace(/</g, "&lt;").replace(/>/g, "&gt;");
      const safeError =
        error instanceof Error
          ? error.message.replace(/</g, "&lt;").replace(/>/g, "&gt;")
          : "Unknown render execution error";
      console.error(
        `[markdown-it-gladst] Critical error calling native renderLatex for formula:\n${formula}\n`,
        error
      );
      const tag = isBlock ? "div" : "span";
      const errClass = `gladst-error-${isBlock ? "block" : "inline"}`;
      const title = `title="${safeError.replace(/"/g, "&quot;")}"`;
      return `<${tag} class="${errClass}" ${title}>Critical Error rendering: ${delimiter}${safeFormula}${delimiter}</${tag}>`;
    }
  }

  // Register the block rule for $$
  md.block.ruler.before("fence", "gladst_block", gladstBlockRule, {
    alt: ["paragraph", "reference", "blockquote", "list", "hr", "html_block"],
  });

  // Register the inline rule for $
  // Run after 'escape' rule but before emphasis, links etc.
  md.inline.ruler.after("escape", "gladst_inline", gladstInlineRule);

  // Renderer for block math ($$)
  md.renderer.rules.gladst_block_math = (
    tokens: Token[],
    idx: number,
    _options: MarkdownItOptions,
    _env: unknown,
    _self: Renderer
  ): string => {
    const token = tokens[idx];
    return renderFormula(token.content, token.markup, true); // true for isBlock
  };

  // Renderer for inline math ($)
  md.renderer.rules.gladst_inline_math = (
    tokens: Token[],
    idx: number,
    _options: MarkdownItOptions,
    _env: unknown,
    _self: Renderer
  ): string => {
    const token = tokens[idx];
    return renderFormula(token.content, token.markup, false); // false for isBlock
  };
}

export default gladstPlugin;
