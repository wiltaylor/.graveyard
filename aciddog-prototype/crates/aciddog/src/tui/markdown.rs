//! Pulldown-cmark → ratatui chunks.
//!
//! Parses a markdown string into a sequence of [`MdChunk`]s. Inline formatting
//! (bold, italic, inline code) becomes styled spans. Fenced code blocks become
//! [`MdChunk::Fence`], which the block renderer draws as bordered sub-blocks
//! with their own copy button.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub enum MdChunk {
    Lines(Vec<Line<'static>>),
    Fence { lang: String, body: String },
}

#[derive(Debug, Clone, Default)]
pub struct ParsedMd {
    pub chunks: Vec<MdChunk>,
}

/// Parse a markdown string into renderable chunks.
pub fn parse(raw: &str) -> ParsedMd {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(raw, opts);

    let mut builder = Builder::default();
    for ev in parser {
        builder.handle(ev);
    }
    builder.finish()
}

#[derive(Default)]
struct Builder {
    chunks: Vec<MdChunk>,
    /// Accumulated lines not yet flushed into a `Lines` chunk.
    lines: Vec<Line<'static>>,
    /// Current line being built.
    cur: Vec<Span<'static>>,
    /// Inline style stack.
    bold: u32,
    italic: u32,
    strike: u32,
    /// List depth stack: each entry is the current item number for ordered
    /// lists, or None for unordered.
    list_stack: Vec<Option<u64>>,
    /// When inside a fenced code block, accumulate raw text here.
    fence: Option<FenceBuilder>,
    /// Heading level currently open, if any.
    heading_level: Option<HeadingLevel>,
    /// When inside a GFM pipe table, accumulate header / body cells here. The
    /// table is pre-flattened into `lines` on `End(Table)` per the
    /// MdChunk::Lines architecture pick (no separate MdChunk::Table variant).
    current_table: Option<TableBuilder>,
}

struct FenceBuilder {
    lang: String,
    body: String,
}

#[derive(Default)]
struct TableBuilder {
    /// Per-column alignment markers from the header divider (`:---`, `---:`,
    /// `:---:`). Length matches the column count once the header row is seen.
    alignments: Vec<Alignment>,
    /// Header cells; each entry is the styled spans for one cell.
    headers: Vec<Vec<Span<'static>>>,
    /// Body rows; each row is a Vec of cells, each cell a Vec of spans.
    rows: Vec<Vec<Vec<Span<'static>>>>,
    /// Accumulator for the row currently being parsed.
    cur_row: Vec<Vec<Span<'static>>>,
    /// Accumulator for the cell currently being parsed.
    cur_cell: Vec<Span<'static>>,
    /// True while inside a `Tag::TableHead`.
    in_header: bool,
    /// True while inside a `Tag::TableCell` — gates the `target_spans` router.
    in_cell: bool,
}

impl Builder {
    fn inline_style(&self) -> Style {
        let mut s = Style::default();
        if self.bold > 0 {
            s = s.add_modifier(Modifier::BOLD);
        }
        if self.italic > 0 {
            s = s.add_modifier(Modifier::ITALIC);
        }
        if self.strike > 0 {
            s = s.add_modifier(Modifier::CROSSED_OUT);
        }
        if self.heading_level.is_some() {
            s = s.fg(Color::Yellow).add_modifier(Modifier::BOLD);
        }
        s
    }

    /// Route inline-content pushes (`push_text`, `push_code`, `SoftBreak`) to
    /// the current table cell when one is open, else to `self.cur`. Keeps the
    /// inline-style helpers cell-agnostic per plan cac1b9df T3.
    fn target_spans(&mut self) -> &mut Vec<Span<'static>> {
        if let Some(t) = self.current_table.as_mut() {
            if t.in_cell {
                return &mut t.cur_cell;
            }
        }
        &mut self.cur
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Newlines inside a table cell should never come from pulldown-cmark in
        // practice, but if they do, collapse them to a space (cells are
        // single-line in this renderer).
        let in_cell = self
            .current_table
            .as_ref()
            .is_some_and(|t| t.in_cell);
        let style = self.inline_style();
        // Text may contain newlines (e.g. inside code blocks elsewhere). For
        // regular prose, split on '\n' and start a new Line for each.
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            if !first.is_empty() {
                self.target_spans().push(Span::styled(first.to_string(), style));
            }
        }
        for rest in parts {
            if in_cell {
                self.target_spans().push(Span::raw(" "));
            } else {
                self.end_line();
            }
            if !rest.is_empty() {
                self.target_spans().push(Span::styled(rest.to_string(), style));
            }
        }
    }

    fn push_code(&mut self, text: &str) {
        let style = Style::default().fg(Color::LightMagenta).bg(Color::Black);
        self.target_spans().push(Span::styled(text.to_string(), style));
    }

    fn end_line(&mut self) {
        let spans = std::mem::take(&mut self.cur);
        self.lines.push(Line::from(spans));
    }

    fn flush_lines_chunk(&mut self) {
        if !self.cur.is_empty() {
            self.end_line();
        }
        if !self.lines.is_empty() {
            let lines = std::mem::take(&mut self.lines);
            self.chunks.push(MdChunk::Lines(lines));
        }
    }

    fn blank_line(&mut self) {
        if !self.cur.is_empty() {
            self.end_line();
        }
        self.lines.push(Line::from(""));
    }

    /// Pre-flatten the accumulated `current_table` into `Line<'static>`s on
    /// `self.lines`. Per plan cac1b9df T4 / O1: no separate MdChunk::Table
    /// variant — tables ride the existing MdChunk::Lines render path.
    ///
    /// Glyph set (planner-locked, O2): cell separator ` │ `, column joins
    /// `┼`, header divider `─`, all `Color::DarkGray`. House-style precedent
    /// is BlockQuote (`│`) and Rule (`───`).
    fn flush_table(&mut self) {
        let Some(table) = self.current_table.take() else {
            return;
        };
        if table.headers.is_empty() && table.rows.is_empty() {
            return;
        }

        // Flush any in-flight `self.cur` so the table starts on a fresh line.
        if !self.cur.is_empty() {
            self.end_line();
        }

        let col_count = table
            .headers
            .len()
            .max(table.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if col_count == 0 {
            return;
        }

        // Display-width of the concatenated text content of one cell. Uses the
        // unicode-width crate (R1 mitigation) so CJK/emoji cells size
        // correctly; raw `chars().count()` would mis-size double-width glyphs.
        let cell_width = |cell: &[Span<'static>]| -> usize {
            cell.iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        };

        let mut col_widths: Vec<usize> = vec![0; col_count];
        for (i, cell) in table.headers.iter().enumerate() {
            col_widths[i] = col_widths[i].max(cell_width(cell));
        }
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell_width(cell));
                }
            }
        }

        let sep_style = Style::default().fg(Color::DarkGray);
        let sep_span = || Span::styled(" │ ", sep_style);

        // Pad a cell's spans out to `target_width` per `align`. Padding is
        // ASCII spaces appended as a raw span; existing inline styling on the
        // cell's spans is preserved (so e.g. inline-bold survives the pad).
        let pad_cell =
            |cell: Vec<Span<'static>>, width: usize, align: Alignment, bold: bool| -> Vec<Span<'static>> {
                let content_w = cell
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum::<usize>();
                let pad = width.saturating_sub(content_w);
                let (left, right) = match align {
                    Alignment::Right => (pad, 0),
                    Alignment::Center => (pad / 2, pad - pad / 2),
                    // pulldown-cmark's `Alignment::None` is treated as left,
                    // matching GFM default.
                    Alignment::Left | Alignment::None => (0, pad),
                };
                let mut out: Vec<Span<'static>> = Vec::with_capacity(cell.len() + 2);
                if left > 0 {
                    out.push(Span::raw(" ".repeat(left)));
                }
                if bold {
                    for s in cell {
                        let style = s.style.add_modifier(Modifier::BOLD);
                        out.push(Span::styled(s.content, style));
                    }
                } else {
                    out.extend(cell);
                }
                if right > 0 {
                    out.push(Span::raw(" ".repeat(right)));
                }
                out
            };

        // Header row.
        if !table.headers.is_empty() {
            let mut headers = table.headers;
            // Pad short header rows out to col_count with empty cells.
            while headers.len() < col_count {
                headers.push(Vec::new());
            }
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (i, cell) in headers.into_iter().enumerate() {
                if i > 0 {
                    spans.push(sep_span());
                }
                let align = table.alignments.get(i).copied().unwrap_or(Alignment::None);
                spans.extend(pad_cell(cell, col_widths[i], align, true));
            }
            self.lines.push(Line::from(spans));

            // Header divider — `─` per column, `┼` at column joins.
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (i, w) in col_widths.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(
                        "─┼─".to_string(),
                        sep_style,
                    ));
                }
                spans.push(Span::styled("─".repeat(*w), sep_style));
            }
            self.lines.push(Line::from(spans));
        }

        // Body rows.
        for row in table.rows {
            let mut row = row;
            while row.len() < col_count {
                row.push(Vec::new());
            }
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (i, cell) in row.into_iter().enumerate() {
                if i > 0 {
                    spans.push(sep_span());
                }
                let align = table.alignments.get(i).copied().unwrap_or(Alignment::None);
                spans.extend(pad_cell(cell, col_widths[i], align, false));
            }
            self.lines.push(Line::from(spans));
        }

        // Trailing blank line for breathing room (mirrors Rule which leads
        // with `blank_line()` at line 241 — symmetric trailing-blank here).
        self.lines.push(Line::from(""));
    }

    fn list_prefix(&self) -> String {
        // Indentation is 2 spaces per nesting level beyond the first.
        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "  ".repeat(depth);
        match self.list_stack.last() {
            Some(Some(n)) => format!("{indent}{n}. "),
            Some(None) => format!("{indent}• "),
            None => String::new(),
        }
    }

    fn handle(&mut self, ev: Event<'_>) {
        if let Some(fb) = self.fence.as_mut() {
            match ev {
                Event::Text(t) => fb.body.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let fb = self.fence.take().unwrap();
                    // Strip one trailing newline — pulldown always appends one.
                    let body = fb.body.trim_end_matches('\n').to_string();
                    self.chunks.push(MdChunk::Fence {
                        lang: fb.lang,
                        body,
                    });
                }
                _ => {}
            }
            return;
        }

        match ev {
            Event::Start(Tag::Paragraph) => {
                if !self.lines.is_empty() || !self.cur.is_empty() {
                    self.blank_line();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !self.cur.is_empty() {
                    self.end_line();
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                if !self.lines.is_empty() || !self.cur.is_empty() {
                    self.blank_line();
                }
                self.heading_level = Some(level);
                let hashes = "#".repeat(level as usize);
                self.cur.push(Span::styled(
                    format!("{hashes} "),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                self.heading_level = None;
                if !self.cur.is_empty() {
                    self.end_line();
                }
            }
            Event::Start(Tag::List(start)) => {
                if !self.lines.is_empty() || !self.cur.is_empty() {
                    self.end_line();
                }
                self.list_stack.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                let prefix = self.list_prefix();
                self.cur
                    .push(Span::styled(prefix, Style::default().fg(Color::Cyan)));
                // Advance the current ordered-list counter for the next item.
                if let Some(Some(n)) = self.list_stack.last_mut() {
                    *n += 1;
                }
            }
            Event::End(TagEnd::Item) => {
                if !self.cur.is_empty() {
                    self.end_line();
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_lines_chunk();
                let lang = match kind {
                    CodeBlockKind::Fenced(s) => s.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.fence = Some(FenceBuilder {
                    lang,
                    body: String::new(),
                });
            }
            Event::Start(Tag::Emphasis) => self.italic += 1,
            Event::End(TagEnd::Emphasis) => self.italic = self.italic.saturating_sub(1),
            Event::Start(Tag::Strong) => self.bold += 1,
            Event::End(TagEnd::Strong) => self.bold = self.bold.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => self.strike += 1,
            Event::End(TagEnd::Strikethrough) => self.strike = self.strike.saturating_sub(1),
            Event::Start(Tag::BlockQuote(_)) => {
                if !self.cur.is_empty() {
                    self.end_line();
                }
                self.cur
                    .push(Span::styled("│ ", Style::default().fg(Color::DarkGray)));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if !self.cur.is_empty() {
                    self.end_line();
                }
            }
            Event::Start(Tag::Table(alignments)) => {
                if !self.cur.is_empty() {
                    self.end_line();
                }
                self.current_table = Some(TableBuilder {
                    alignments,
                    ..TableBuilder::default()
                });
            }
            Event::End(TagEnd::Table) => {
                self.flush_table();
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = self.current_table.as_mut() {
                    t.in_header = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = self.current_table.as_mut() {
                    let row = std::mem::take(&mut t.cur_row);
                    t.headers = row;
                    t.in_header = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = self.current_table.as_mut() {
                    t.cur_row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = self.current_table.as_mut() {
                    let row = std::mem::take(&mut t.cur_row);
                    if t.in_header {
                        // Defensive: in pulldown-cmark practice the head row
                        // is closed by End(TableHead) before End(TableRow), so
                        // this branch is rarely hit. Keep symmetric anyway.
                        t.headers = row;
                    } else {
                        t.rows.push(row);
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(t) = self.current_table.as_mut() {
                    t.cur_cell.clear();
                    t.in_cell = true;
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = self.current_table.as_mut() {
                    let cell = std::mem::take(&mut t.cur_cell);
                    t.cur_row.push(cell);
                    t.in_cell = false;
                }
            }
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => self.push_code(&t),
            Event::SoftBreak => self.target_spans().push(Span::raw(" ")),
            Event::HardBreak => self.end_line(),
            Event::Rule => {
                self.blank_line();
                self.cur
                    .push(Span::styled("───", Style::default().fg(Color::DarkGray)));
                self.end_line();
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                self.cur.push(Span::styled(
                    marker.to_string(),
                    Style::default().fg(Color::Cyan),
                ));
            }
            _ => {}
        }
    }

    fn finish(mut self) -> ParsedMd {
        self.flush_lines_chunk();
        ParsedMd {
            chunks: self.chunks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_fence(md: &ParsedMd) -> Option<(&str, &str)> {
        md.chunks.iter().find_map(|c| match c {
            MdChunk::Fence { lang, body } => Some((lang.as_str(), body.as_str())),
            _ => None,
        })
    }

    #[test]
    fn plain_text_produces_one_lines_chunk() {
        let md = parse("hello world");
        assert_eq!(md.chunks.len(), 1);
        assert!(matches!(md.chunks[0], MdChunk::Lines(_)));
    }

    #[test]
    fn fenced_code_becomes_fence_chunk() {
        let md = parse("# title\n\n```rust\nfn main() {}\n```\n");
        let (lang, body) = first_fence(&md).expect("expected a fence chunk");
        assert_eq!(lang, "rust");
        assert_eq!(body, "fn main() {}");
    }

    #[test]
    fn heading_chunk_has_hashes_prefix() {
        let md = parse("## hello");
        if let MdChunk::Lines(lines) = &md.chunks[0] {
            let rendered: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(rendered.contains("## "));
            assert!(rendered.contains("hello"));
        } else {
            panic!("expected Lines chunk");
        }
    }

    #[test]
    fn bold_text_picks_up_modifier() {
        let md = parse("this is **bold** text");
        let MdChunk::Lines(lines) = &md.chunks[0] else {
            panic!("expected Lines chunk");
        };
        let has_bold = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|s| s.content == "bold" && s.style.add_modifier.contains(Modifier::BOLD))
        });
        assert!(has_bold, "expected a span with BOLD modifier");
    }

    #[test]
    fn list_items_get_bullet_prefix() {
        let md = parse("- first\n- second\n");
        let MdChunk::Lines(lines) = &md.chunks[0] else {
            panic!("expected Lines chunk");
        };
        let rendered: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(rendered
            .iter()
            .any(|s| s.contains("• ") && s.contains("first")));
        assert!(rendered
            .iter()
            .any(|s| s.contains("• ") && s.contains("second")));
    }

    #[test]
    fn multiple_paragraphs_and_fences_preserve_order() {
        let md = parse("intro\n\n```\ncode\n```\n\noutro\n");
        assert!(matches!(md.chunks[0], MdChunk::Lines(_)));
        assert!(matches!(md.chunks[1], MdChunk::Fence { .. }));
        assert!(matches!(md.chunks[2], MdChunk::Lines(_)));
    }

    /// Helper: render a Line back to its concatenated visible text.
    fn line_text(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Helper: pull the lines out of the first MdChunk::Lines chunk.
    fn first_lines(md: &ParsedMd) -> &[Line<'static>] {
        match &md.chunks[0] {
            MdChunk::Lines(lines) => lines,
            _ => panic!("expected Lines chunk"),
        }
    }

    #[test]
    fn parses_2x2_pipe_table_into_four_visual_lines() {
        // Repro input from comment 6bfd27d0 step 3 — pre-fix this collapsed
        // into a single wrapped paragraph.
        let md = parse("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        let lines = first_lines(&md);
        // Expected: header, divider, body row 1, body row 2, plus trailing
        // blank-line breathing-room → at least 4 structural lines (the blank
        // is permitted to be Line 5).
        assert!(
            lines.len() >= 4,
            "expected at least 4 visual lines, got {}: {:?}",
            lines.len(),
            lines.iter().map(line_text).collect::<Vec<_>>()
        );
        // Header row (line 0) should mention both `a` and `b`, separated by
        // the Unicode cell separator `│`.
        let header = line_text(&lines[0]);
        assert!(header.contains('a'), "header missing 'a': {header:?}");
        assert!(header.contains('b'), "header missing 'b': {header:?}");
        assert!(
            header.contains('│'),
            "header missing '│' cell separator: {header:?}"
        );
        // Divider row (line 1) should be made of `─` and `┼`.
        let divider = line_text(&lines[1]);
        assert!(divider.contains('─'), "divider missing '─': {divider:?}");
        assert!(divider.contains('┼'), "divider missing '┼': {divider:?}");
        // Body rows mention 1/2 and 3/4 respectively.
        let body1 = line_text(&lines[2]);
        let body2 = line_text(&lines[3]);
        assert!(body1.contains('1') && body1.contains('2'), "body1: {body1:?}");
        assert!(body2.contains('3') && body2.contains('4'), "body2: {body2:?}");
    }

    #[test]
    fn table_honours_left_right_center_alignment() {
        // 3-col table: left, right, center alignment markers. Single-char
        // body cells in 3-char-wide columns (header text is wider than body
        // so col_widths == header widths == 3).
        let md = parse("| aaa | bbb | ccc |\n|:---|---:|:---:|\n| x | y | z |\n");
        let lines = first_lines(&md);
        // Body row is line 2 (header=0, divider=1).
        let body = &lines[2];
        // Walk the spans and find the chunk for each column. Easiest test:
        // assert specific padded content per cell as substrings.
        let body_text = line_text(body);
        // Columns are 3 wide. Left-aligned 'x' → "x  "; right-aligned 'y' →
        // "  y"; center-aligned 'z' → " z " (pad=2 → 1 left + 1 right).
        assert!(
            body_text.contains("x  "),
            "expected left-padded 'x  ' in body: {body_text:?}"
        );
        assert!(
            body_text.contains("  y"),
            "expected right-padded '  y' in body: {body_text:?}"
        );
        assert!(
            body_text.contains(" z "),
            "expected centred ' z ' in body: {body_text:?}"
        );
    }

    #[test]
    fn table_with_inline_bold_in_header_cell_renders_bold_span() {
        let md = parse("| **a** | b |\n|---|---|\n| 1 | 2 |\n");
        let lines = first_lines(&md);
        let header = &lines[0];
        // Header should contain a span with content "a" carrying BOLD.
        let has_bold_a = header.spans.iter().any(|s| {
            s.content == "a" && s.style.add_modifier.contains(Modifier::BOLD)
        });
        assert!(
            has_bold_a,
            "expected header to contain a bold span for 'a': {:?}",
            header.spans
        );
    }

    #[test]
    fn table_does_not_concatenate_rows_with_softbreak() {
        // Counter-test to the repro flatten bug: with ENABLE_TABLES on, the
        // 4 source rows must NOT be merged into one wrapped Line whose text
        // contains literal pipe characters.
        let md = parse("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        let lines = first_lines(&md);
        let joined: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        // Repro bug produced a single line containing `|---|---|` literally.
        assert!(
            !joined.contains("|---|---|"),
            "table rows were not parsed — saw raw header divider in output: {joined:?}"
        );
        // And there must be at least two distinct body lines (i.e. no row
        // merging).
        assert!(lines.len() >= 4, "expected ≥4 lines, got {}", lines.len());
        // Sanity: the very first line should not contain BOTH 'a' and '1'
        // (those belong to header and body 1 respectively — separate lines).
        let first = line_text(&lines[0]);
        assert!(
            !(first.contains('a') && first.contains('1')),
            "header line accidentally merged with body row 1: {first:?}"
        );
    }

    #[test]
    fn softbreak_outside_table_still_collapses_to_space() {
        // Regression check — non-table prose with a soft-break still becomes
        // a single Line whose content joins with a space (existing line-238
        // behaviour preserved post-router).
        let md = parse("alpha\nbeta\n");
        let lines = first_lines(&md);
        assert_eq!(lines.len(), 1, "expected one Line for soft-broken prose");
        let text = line_text(&lines[0]);
        assert!(
            text.contains("alpha") && text.contains("beta"),
            "missing content: {text:?}"
        );
        assert!(
            text.contains("alpha") && text.contains(' ') && text.contains("beta"),
            "expected space-joined 'alpha beta', got: {text:?}"
        );
    }
}
