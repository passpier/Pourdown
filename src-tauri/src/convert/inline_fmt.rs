//! Shared inline-formatting helpers used by both the docx and pptx importers.
//!
//! Both formats build up a paragraph as a sequence of formatted "runs" and
//! need to wrap them in Markdown emphasis markers without producing invalid
//! or ambiguous CommonMark (`****` at a run boundary, or a `**`/`_` marker
//! immediately touching whitespace). This module holds that shared, subtle
//! logic in one place so the two importers can't drift out of sync.

/// Escape literal Markdown inline-emphasis/code characters that appear in raw
/// source text, so author-typed `*`, `_`, backticks aren't reinterpreted as
/// Markdown (e.g. a leading `*` becoming a bullet). Backslash is escaped first.
/// Block-level leading markers (- + # >) are intentionally NOT escaped (a
/// deliberate, narrower scope) to avoid mangling common text like dates
/// ("2024-07-26") and "Item #5".
pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Apply bold/italic/strikethrough markers, skipping whitespace-only text.
///
/// CommonMark requires an emphasis opener/closer to hug its text — `**`
/// immediately followed by whitespace is not a valid opener, so a run like
/// `"  Title"` wrapped naively as `"**  Title**"` renders as literal
/// asterisks. Leading/trailing whitespace is moved outside the markers so
/// emphasis stays valid while inter-run spacing (e.g. between adjacent runs
/// in the same paragraph) is preserved.
pub fn apply_inline_fmt(text: &str, bold: bool, italic: bool, strike: bool) -> String {
    if text.trim().is_empty() || (!bold && !italic && !strike) {
        return text.to_string();
    }
    let trimmed_start = text.trim_start();
    let leading = &text[..text.len() - trimmed_start.len()];
    let core = trimmed_start.trim_end();
    let trailing = &trimmed_start[core.len()..];

    let s = if strike {
        format!("~~{}~~", core)
    } else {
        core.to_string()
    };
    let wrapped = match (bold, italic) {
        (true, true) => format!("***{}***", s),
        (true, false) => format!("**{}**", s),
        (false, true) => format!("*{}*", s),
        (false, false) => s,
    };
    format!("{}{}{}", leading, wrapped, trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_markdown() {
        assert_eq!(escape_markdown("a*b_c`d\\e"), "a\\*b\\_c\\`d\\\\e");
    }

    #[test]
    fn test_apply_inline_fmt_moves_leading_whitespace_outside_markers() {
        assert_eq!(apply_inline_fmt("  Foo (FSD)", true, false, false), "  **Foo (FSD)**");
        assert_eq!(apply_inline_fmt(" Version : 1.0", true, false, false), " **Version : 1.0**");
    }

    #[test]
    fn test_apply_inline_fmt_moves_trailing_whitespace_outside_markers() {
        assert_eq!(apply_inline_fmt("bold ", true, false, false), "**bold** ");
    }

    #[test]
    fn test_apply_inline_fmt_whitespace_only_not_wrapped() {
        assert_eq!(apply_inline_fmt("   ", true, false, false), "   ");
    }

    #[test]
    fn test_apply_inline_fmt_italic_and_strike() {
        assert_eq!(apply_inline_fmt("hi", false, true, false), "*hi*");
        assert_eq!(apply_inline_fmt("hi", false, false, true), "~~hi~~");
        assert_eq!(apply_inline_fmt("hi", true, true, false), "***hi***");
    }
}
