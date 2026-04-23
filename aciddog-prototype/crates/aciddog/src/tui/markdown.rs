//! Pulldown-cmark → ratatui chunks.
//!
//! Parses a markdown string into a sequence of [`MdChunk`]s. Inline formatting
//! (bold, italic, inline code) becomes styled spans. Fenced code blocks become
//! [`MdChunk::Fence`], which the block renderer draws as bordered sub-blocks
//! with their own copy button.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

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
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
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
}

struct FenceBuilder {
    lang: String,
    body: String,
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

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Text may contain newlines (e.g. inside code blocks elsewhere). For
        // regular prose, split on '\n' and start a new Line for each.
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            if !first.is_empty() {
                self.cur
                    .push(Span::styled(first.to_string(), self.inline_style()));
            }
        }
        for rest in parts {
            self.end_line();
            if !rest.is_empty() {
                self.cur
                    .push(Span::styled(rest.to_string(), self.inline_style()));
            }
        }
    }

    fn push_code(&mut self, text: &str) {
        let style = Style::default().fg(Color::LightMagenta).bg(Color::Black);
        self.cur.push(Span::styled(text.to_string(), style));
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
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => self.push_code(&t),
            Event::SoftBreak => self.cur.push(Span::raw(" ")),
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
}
