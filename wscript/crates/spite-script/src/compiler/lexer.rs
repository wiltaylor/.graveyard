//! Lexer for SpiteScript source code.

use smol_str::SmolStr;

use super::token::{Token, TokenKind, Span};

// ---------------------------------------------------------------------------
// Lexer mode stack (for template string interpolation)
// ---------------------------------------------------------------------------

/// Tracks what context the lexer is currently scanning inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerMode {
    /// Normal top-level (or nested-expression) scanning.
    Normal,
    /// Inside a template literal – scanning string parts and looking for `${`.
    Template,
    /// Inside a `${...}` expression within a template literal.  The `depth`
    /// counts how many unmatched `{` we have seen so we know when the closing
    /// `}` ends the interpolation versus a nested block.
    TemplateExpr { brace_depth: u32 },
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// The SpiteScript lexer.
pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: u32,
    col: u32,
    mode_stack: Vec<LexerMode>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
            col: 1,
            mode_stack: vec![LexerMode::Normal],
        }
    }

    /// Tokenize the entire source into a list of tokens.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    // ------------------------------------------------------------------
    // Mode helpers
    // ------------------------------------------------------------------

    fn current_mode(&self) -> LexerMode {
        self.mode_stack.last().copied().unwrap_or(LexerMode::Normal)
    }

    fn push_mode(&mut self, mode: LexerMode) {
        self.mode_stack.push(mode);
    }

    fn pop_mode(&mut self) {
        self.mode_stack.pop();
    }

    // ------------------------------------------------------------------
    // Character-level helpers
    // ------------------------------------------------------------------

    #[inline]
    fn at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    /// Return the current character without consuming it.  Returns `'\0'` at
    /// end-of-input.
    #[inline]
    fn current(&self) -> char {
        if self.at_end() {
            '\0'
        } else {
            // SAFETY: pos is always kept on a char boundary by advance().
            // We still use the safe path via chars().
            self.source[self.pos..].chars().next().unwrap_or('\0')
        }
    }

    /// Peek one character ahead of `current()`.
    #[inline]
    fn peek(&self) -> char {
        if self.at_end() {
            return '\0';
        }
        let c = self.current();
        let next = self.pos + c.len_utf8();
        if next >= self.source.len() {
            '\0'
        } else {
            self.source[next..].chars().next().unwrap_or('\0')
        }
    }

    /// Advance past the current character, updating line/col tracking.
    fn advance(&mut self) {
        if !self.at_end() {
            let ch = self.current();
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += ch.len_utf8();
        }
    }

    /// Advance and return the character that was consumed.
    fn advance_char(&mut self) -> char {
        let ch = self.current();
        self.advance();
        ch
    }

    fn skip_whitespace(&mut self) {
        while !self.at_end() && self.current().is_whitespace() {
            self.advance();
        }
    }

    fn make_span(&self, start: usize, start_line: u32, start_col: u32) -> Span {
        Span {
            start: start as u32,
            end: self.pos as u32,
            line: start_line,
            col: start_col,
        }
    }

    fn make_token(&self, kind: TokenKind, start: usize, start_line: u32, start_col: u32) -> Token {
        Token {
            kind,
            span: self.make_span(start, start_line, start_col),
        }
    }

    // ------------------------------------------------------------------
    // Main dispatch
    // ------------------------------------------------------------------

    fn next_token(&mut self) -> Token {
        match self.current_mode() {
            LexerMode::Template => return self.lex_template_body(),
            LexerMode::TemplateExpr { .. } | LexerMode::Normal => {}
        }

        self.skip_whitespace();

        if self.at_end() {
            return Token {
                kind: TokenKind::Eof,
                span: Span::new(self.pos as u32, self.pos as u32, self.line, self.col),
            };
        }

        let start = self.pos;
        let start_line = self.line;
        let start_col = self.col;
        let ch = self.current();

        // ----- Numbers -----
        if ch.is_ascii_digit() {
            return self.lex_number(start, start_line, start_col);
        }

        // ----- Identifiers / keywords -----
        if is_ident_start(ch) {
            return self.lex_ident(start, start_line, start_col);
        }

        // ----- String literals -----
        if ch == '"' {
            return self.lex_string(start, start_line, start_col);
        }

        // ----- Character literals -----
        if ch == '\'' {
            return self.lex_char(start, start_line, start_col);
        }

        // ----- Template strings -----
        if ch == '`' {
            return self.lex_template_start(start, start_line, start_col);
        }

        // ----- Comments & slash -----
        if ch == '/' {
            if self.peek() == '/' {
                return self.lex_line_comment(start, start_line, start_col);
            }
            if self.peek() == '*' {
                return self.lex_block_comment(start, start_line, start_col);
            }
        }

        // ----- Operators / punctuation -----
        self.lex_operator_or_punct(start, start_line, start_col)
    }

    // ------------------------------------------------------------------
    // Operators & punctuation
    // ------------------------------------------------------------------

    fn lex_operator_or_punct(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        let ch = self.advance_char();
        let kind = match ch {
            '+' => {
                if self.current() == '=' { self.advance(); TokenKind::PlusEq }
                else { TokenKind::Plus }
            }
            '-' => {
                if self.current() == '>' { self.advance(); TokenKind::Arrow }
                else if self.current() == '=' { self.advance(); TokenKind::MinusEq }
                else { TokenKind::Minus }
            }
            '*' => {
                if self.current() == '=' { self.advance(); TokenKind::StarEq }
                else { TokenKind::Star }
            }
            '/' => {
                // Comments are handled before we get here.
                if self.current() == '=' { self.advance(); TokenKind::SlashEq }
                else { TokenKind::Slash }
            }
            '%' => {
                if self.current() == '=' { self.advance(); TokenKind::PercentEq }
                else { TokenKind::Percent }
            }
            '=' => {
                if self.current() == '=' { self.advance(); TokenKind::EqEq }
                else if self.current() == '>' { self.advance(); TokenKind::FatArrow }
                else { TokenKind::Eq }
            }
            '!' => {
                if self.current() == '=' { self.advance(); TokenKind::BangEq }
                else { TokenKind::Bang }
            }
            '<' => {
                // <<=  <=>  <<  <=  <
                if self.current() == '<' {
                    self.advance();
                    if self.current() == '=' { self.advance(); TokenKind::LtLtEq }
                    else { TokenKind::LtLt }
                } else if self.current() == '=' {
                    self.advance();
                    if self.current() == '>' { self.advance(); TokenKind::Spaceship }
                    else { TokenKind::LtEq }
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                // >>=  >>  >=  >
                if self.current() == '>' {
                    self.advance();
                    if self.current() == '=' { self.advance(); TokenKind::GtGtEq }
                    else { TokenKind::GtGt }
                } else if self.current() == '=' {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                if self.current() == '&' { self.advance(); TokenKind::AmpAmp }
                else if self.current() == '=' { self.advance(); TokenKind::AmpEq }
                else { TokenKind::Amp }
            }
            '|' => {
                if self.current() == '|' { self.advance(); TokenKind::PipePipe }
                else if self.current() == '>' { self.advance(); TokenKind::PipeGt }
                else if self.current() == '=' { self.advance(); TokenKind::PipeEq }
                else { TokenKind::Pipe }
            }
            '^' => {
                if self.current() == '=' { self.advance(); TokenKind::CaretEq }
                else { TokenKind::Caret }
            }
            '~' => TokenKind::Tilde,
            '.' => {
                if self.current() == '.' {
                    self.advance();
                    if self.current() == '=' { self.advance(); TokenKind::DotDotEq }
                    else { TokenKind::DotDot }
                } else {
                    TokenKind::Dot
                }
            }
            ':' => {
                if self.current() == ':' { self.advance(); TokenKind::ColonColon }
                else { TokenKind::Colon }
            }
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => {
                // Track brace depth when inside a template expression.
                if let LexerMode::TemplateExpr { brace_depth } = self.current_mode() {
                    self.pop_mode();
                    self.push_mode(LexerMode::TemplateExpr { brace_depth: brace_depth + 1 });
                }
                TokenKind::LBrace
            }
            '}' => {
                if let LexerMode::TemplateExpr { brace_depth } = self.current_mode() {
                    if brace_depth == 0 {
                        // This `}` closes the `${...}` interpolation.
                        self.pop_mode(); // pop TemplateExpr
                        // We should now be back in Template mode (pushed when we
                        // started the interpolation).  Emit TemplateExprEnd.
                        return self.make_token(TokenKind::TemplateExprEnd, start, start_line, start_col);
                    } else {
                        self.pop_mode();
                        self.push_mode(LexerMode::TemplateExpr { brace_depth: brace_depth - 1 });
                    }
                }
                TokenKind::RBrace
            }
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '#' => TokenKind::Hash,
            '@' => TokenKind::At,
            '?' => TokenKind::Question,
            _ => TokenKind::Error(format!("unexpected character: {ch}")),
        };

        self.make_token(kind, start, start_line, start_col)
    }

    // ------------------------------------------------------------------
    // Line comments  (// ... and /// ...)
    // ------------------------------------------------------------------

    fn lex_line_comment(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        // We are positioned at '/'.  Peek is '/'.
        self.advance(); // consume first '/'
        self.advance(); // consume second '/'

        let is_doc = self.current() == '/' && self.peek() != '/';

        if is_doc {
            self.advance(); // consume third '/'
            // Skip optional single leading space.
            if self.current() == ' ' {
                self.advance();
            }
            let content_start = self.pos;
            while !self.at_end() && self.current() != '\n' {
                self.advance();
            }
            let content = self.source[content_start..self.pos].to_string();
            return self.make_token(TokenKind::DocComment(content), start, start_line, start_col);
        }

        // Regular line comment – skip until EOL and recurse.
        while !self.at_end() && self.current() != '\n' {
            self.advance();
        }
        self.next_token()
    }

    // ------------------------------------------------------------------
    // Block comments  (/* ... */ and /** ... */)
    // ------------------------------------------------------------------

    fn lex_block_comment(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // consume '/'
        self.advance(); // consume '*'

        let is_doc = self.current() == '*' && self.peek() != '/';
        if is_doc {
            self.advance(); // consume the extra '*'
        }

        let content_start = self.pos;
        let mut depth: u32 = 1;

        while !self.at_end() && depth > 0 {
            if self.current() == '/' && self.peek() == '*' {
                self.advance();
                self.advance();
                depth += 1;
            } else if self.current() == '*' && self.peek() == '/' {
                self.advance();
                self.advance();
                depth -= 1;
            } else {
                self.advance();
            }
        }

        if depth > 0 {
            return self.make_token(
                TokenKind::Error("unterminated block comment".to_string()),
                start,
                start_line,
                start_col,
            );
        }

        if is_doc {
            // content_start .. (self.pos - 2) (before closing */)
            let end_content = if self.pos >= 2 { self.pos - 2 } else { self.pos };
            let raw = &self.source[content_start..end_content];
            let content = raw.trim().to_string();
            return self.make_token(TokenKind::DocComment(content), start, start_line, start_col);
        }

        // Regular block comment – skip and get next real token.
        self.next_token()
    }

    // ------------------------------------------------------------------
    // String literals
    // ------------------------------------------------------------------

    fn lex_string(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // consume opening '"'
        let mut content = String::new();

        while !self.at_end() && self.current() != '"' {
            if self.current() == '\\' {
                self.advance(); // consume '\'
                if self.at_end() {
                    return self.make_token(
                        TokenKind::Error("unterminated string escape".to_string()),
                        start,
                        start_line,
                        start_col,
                    );
                }
                match self.current() {
                    'n' => { content.push('\n'); self.advance(); }
                    't' => { content.push('\t'); self.advance(); }
                    'r' => { content.push('\r'); self.advance(); }
                    '\\' => { content.push('\\'); self.advance(); }
                    '"' => { content.push('"'); self.advance(); }
                    '0' => { content.push('\0'); self.advance(); }
                    'x' => {
                        self.advance(); // skip 'x'
                        match self.lex_hex_escape(2) {
                            Ok(c) => content.push(c),
                            Err(msg) => {
                                return self.make_token(
                                    TokenKind::Error(msg),
                                    start,
                                    start_line,
                                    start_col,
                                );
                            }
                        }
                    }
                    'u' => {
                        self.advance(); // skip 'u'
                        match self.lex_unicode_escape() {
                            Ok(c) => content.push(c),
                            Err(msg) => {
                                return self.make_token(
                                    TokenKind::Error(msg),
                                    start,
                                    start_line,
                                    start_col,
                                );
                            }
                        }
                    }
                    c => {
                        content.push('\\');
                        content.push(c);
                        self.advance();
                    }
                }
            } else {
                content.push(self.advance_char());
            }
        }

        if self.at_end() {
            return self.make_token(
                TokenKind::Error("unterminated string literal".to_string()),
                start,
                start_line,
                start_col,
            );
        }

        self.advance(); // consume closing '"'
        self.make_token(TokenKind::StringLit(content), start, start_line, start_col)
    }

    // ------------------------------------------------------------------
    // Character literals
    // ------------------------------------------------------------------

    fn lex_char(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // consume opening '\''

        if self.at_end() || self.current() == '\'' {
            return self.make_token(
                TokenKind::Error("empty character literal".to_string()),
                start,
                start_line,
                start_col,
            );
        }

        let ch = if self.current() == '\\' {
            self.advance(); // consume '\'
            if self.at_end() {
                return self.make_token(
                    TokenKind::Error("unterminated character escape".to_string()),
                    start,
                    start_line,
                    start_col,
                );
            }
            match self.current() {
                'n' => { self.advance(); '\n' }
                't' => { self.advance(); '\t' }
                'r' => { self.advance(); '\r' }
                '\\' => { self.advance(); '\\' }
                '\'' => { self.advance(); '\'' }
                '0' => { self.advance(); '\0' }
                'x' => {
                    self.advance(); // skip 'x'
                    match self.lex_hex_escape(2) {
                        Ok(c) => c,
                        Err(msg) => {
                            return self.make_token(
                                TokenKind::Error(msg),
                                start,
                                start_line,
                                start_col,
                            );
                        }
                    }
                }
                'u' => {
                    self.advance(); // skip 'u'
                    match self.lex_unicode_escape() {
                        Ok(c) => c,
                        Err(msg) => {
                            return self.make_token(
                                TokenKind::Error(msg),
                                start,
                                start_line,
                                start_col,
                            );
                        }
                    }
                }
                c => { self.advance(); c }
            }
        } else {
            let c = self.current();
            self.advance();
            c
        };

        if self.at_end() || self.current() != '\'' {
            return self.make_token(
                TokenKind::Error("unterminated character literal".to_string()),
                start,
                start_line,
                start_col,
            );
        }

        self.advance(); // consume closing '\''
        self.make_token(TokenKind::CharLit(ch), start, start_line, start_col)
    }

    // ------------------------------------------------------------------
    // Escape helpers
    // ------------------------------------------------------------------

    /// Consume exactly `count` hex digits and return the corresponding char.
    fn lex_hex_escape(&mut self, count: usize) -> Result<char, String> {
        let mut value: u32 = 0;
        for _ in 0..count {
            if self.at_end() {
                return Err("incomplete hex escape".to_string());
            }
            let c = self.current();
            let digit = match c {
                '0'..='9' => c as u32 - '0' as u32,
                'a'..='f' => c as u32 - 'a' as u32 + 10,
                'A'..='F' => c as u32 - 'A' as u32 + 10,
                _ => return Err(format!("invalid hex digit in escape: {c}")),
            };
            value = value * 16 + digit;
            self.advance();
        }
        char::from_u32(value).ok_or_else(|| format!("invalid character value: {value:#x}"))
    }

    /// Consume `\u{XXXX}` (the `\u` has already been consumed; we expect
    /// `{` followed by 1-6 hex digits and `}`).
    fn lex_unicode_escape(&mut self) -> Result<char, String> {
        if self.at_end() || self.current() != '{' {
            return Err("expected '{' in unicode escape".to_string());
        }
        self.advance(); // consume '{'

        let mut value: u32 = 0;
        let mut digits = 0;
        while !self.at_end() && self.current() != '}' {
            let c = self.current();
            let digit = match c {
                '0'..='9' => c as u32 - '0' as u32,
                'a'..='f' => c as u32 - 'a' as u32 + 10,
                'A'..='F' => c as u32 - 'A' as u32 + 10,
                _ => return Err(format!("invalid hex digit in unicode escape: {c}")),
            };
            value = value * 16 + digit;
            digits += 1;
            if digits > 6 {
                return Err("unicode escape has too many digits".to_string());
            }
            self.advance();
        }

        if self.at_end() {
            return Err("unterminated unicode escape".to_string());
        }
        self.advance(); // consume '}'

        if digits == 0 {
            return Err("empty unicode escape".to_string());
        }

        char::from_u32(value).ok_or_else(|| format!("invalid unicode scalar value: {value:#x}"))
    }

    // ------------------------------------------------------------------
    // Number literals
    // ------------------------------------------------------------------

    fn lex_number(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        // Check for prefix: 0x, 0b, 0o
        if self.current() == '0' {
            match self.peek() {
                'x' | 'X' => return self.lex_hex_int(start, start_line, start_col),
                'b' | 'B' => return self.lex_bin_int(start, start_line, start_col),
                'o' | 'O' => return self.lex_oct_int(start, start_line, start_col),
                _ => {}
            }
        }

        // Decimal integer or float.
        self.eat_decimal_digits();

        let mut is_float = false;

        // Decimal point – but not `..` or `..=`
        if self.current() == '.' && self.peek() != '.' && self.peek().is_ascii_digit() {
            is_float = true;
            self.advance(); // consume '.'
            self.eat_decimal_digits();
        }

        // Exponent
        if self.current() == 'e' || self.current() == 'E' {
            is_float = true;
            self.advance();
            if self.current() == '+' || self.current() == '-' {
                self.advance();
            }
            if !self.current().is_ascii_digit() {
                return self.make_token(
                    TokenKind::Error("expected digits in exponent".to_string()),
                    start,
                    start_line,
                    start_col,
                );
            }
            self.eat_decimal_digits();
        }

        let raw = &self.source[start..self.pos];
        let clean: String = raw.chars().filter(|&c| c != '_').collect();

        if is_float {
            match clean.parse::<f64>() {
                Ok(v) => self.make_token(TokenKind::FloatLit(v), start, start_line, start_col),
                Err(e) => self.make_token(
                    TokenKind::Error(format!("invalid float literal: {e}")),
                    start,
                    start_line,
                    start_col,
                ),
            }
        } else {
            match clean.parse::<i128>() {
                Ok(v) => self.make_token(TokenKind::IntLit(v), start, start_line, start_col),
                Err(e) => self.make_token(
                    TokenKind::Error(format!("invalid integer literal: {e}")),
                    start,
                    start_line,
                    start_col,
                ),
            }
        }
    }

    fn eat_decimal_digits(&mut self) {
        while !self.at_end() && (self.current().is_ascii_digit() || self.current() == '_') {
            self.advance();
        }
    }

    fn lex_hex_int(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // '0'
        self.advance(); // 'x'
        if self.at_end() || !self.current().is_ascii_hexdigit() {
            return self.make_token(
                TokenKind::Error("expected hex digits after 0x".to_string()),
                start,
                start_line,
                start_col,
            );
        }
        while !self.at_end() && (self.current().is_ascii_hexdigit() || self.current() == '_') {
            self.advance();
        }
        let raw = &self.source[start + 2..self.pos]; // skip "0x"
        let clean: String = raw.chars().filter(|&c| c != '_').collect();
        match i128::from_str_radix(&clean, 16) {
            Ok(v) => self.make_token(TokenKind::IntLit(v), start, start_line, start_col),
            Err(e) => self.make_token(
                TokenKind::Error(format!("invalid hex literal: {e}")),
                start,
                start_line,
                start_col,
            ),
        }
    }

    fn lex_bin_int(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // '0'
        self.advance(); // 'b'
        if self.at_end() || !matches!(self.current(), '0' | '1') {
            return self.make_token(
                TokenKind::Error("expected binary digits after 0b".to_string()),
                start,
                start_line,
                start_col,
            );
        }
        while !self.at_end() && (matches!(self.current(), '0' | '1') || self.current() == '_') {
            self.advance();
        }
        let raw = &self.source[start + 2..self.pos];
        let clean: String = raw.chars().filter(|&c| c != '_').collect();
        match i128::from_str_radix(&clean, 2) {
            Ok(v) => self.make_token(TokenKind::IntLit(v), start, start_line, start_col),
            Err(e) => self.make_token(
                TokenKind::Error(format!("invalid binary literal: {e}")),
                start,
                start_line,
                start_col,
            ),
        }
    }

    fn lex_oct_int(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // '0'
        self.advance(); // 'o'
        if self.at_end() || !matches!(self.current(), '0'..='7') {
            return self.make_token(
                TokenKind::Error("expected octal digits after 0o".to_string()),
                start,
                start_line,
                start_col,
            );
        }
        while !self.at_end() && (matches!(self.current(), '0'..='7') || self.current() == '_') {
            self.advance();
        }
        let raw = &self.source[start + 2..self.pos];
        let clean: String = raw.chars().filter(|&c| c != '_').collect();
        match i128::from_str_radix(&clean, 8) {
            Ok(v) => self.make_token(TokenKind::IntLit(v), start, start_line, start_col),
            Err(e) => self.make_token(
                TokenKind::Error(format!("invalid octal literal: {e}")),
                start,
                start_line,
                start_col,
            ),
        }
    }

    // ------------------------------------------------------------------
    // Identifiers & keywords
    // ------------------------------------------------------------------

    fn lex_ident(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        while !self.at_end() && is_ident_continue(self.current()) {
            self.advance();
        }
        let text = &self.source[start..self.pos];

        // Check for `_` as a standalone token.
        if text == "_" {
            return self.make_token(TokenKind::Underscore, start, start_line, start_col);
        }

        // Check for `true` / `false` (BoolLit).
        if text == "true" {
            return self.make_token(TokenKind::True, start, start_line, start_col);
        }
        if text == "false" {
            return self.make_token(TokenKind::False, start, start_line, start_col);
        }

        let kind = TokenKind::keyword_from_str(text)
            .unwrap_or_else(|| TokenKind::Ident(SmolStr::new(text)));

        self.make_token(kind, start, start_line, start_col)
    }

    // ------------------------------------------------------------------
    // Template strings
    // ------------------------------------------------------------------

    /// Called when we see a backtick in Normal or TemplateExpr mode.
    fn lex_template_start(
        &mut self,
        start: usize,
        start_line: u32,
        start_col: u32,
    ) -> Token {
        self.advance(); // consume '`'
        self.push_mode(LexerMode::Template);
        self.make_token(TokenKind::TemplateLitStart, start, start_line, start_col)
    }

    /// Called when current mode is `Template`.  Scans the template body,
    /// emitting `TemplateStringPart`, `TemplateExprStart`, or `TemplateLitEnd`
    /// tokens.
    fn lex_template_body(&mut self) -> Token {
        let start = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        if self.at_end() {
            self.pop_mode();
            return self.make_token(
                TokenKind::Error("unterminated template literal".to_string()),
                start,
                start_line,
                start_col,
            );
        }

        // Closing backtick?
        if self.current() == '`' {
            self.advance();
            self.pop_mode();
            return self.make_token(TokenKind::TemplateLitEnd, start, start_line, start_col);
        }

        // Start of interpolation `${`?
        if self.current() == '$' && self.peek() == '{' {
            self.advance(); // '$'
            self.advance(); // '{'
            // Push TemplateExpr on top of Template so that when the expr ends
            // we pop back to Template mode.
            self.push_mode(LexerMode::TemplateExpr { brace_depth: 0 });
            return self.make_token(TokenKind::TemplateExprStart, start, start_line, start_col);
        }

        // Otherwise, accumulate a string part until we hit '`', '${', or EOF.
        let mut content = String::new();
        while !self.at_end() {
            if self.current() == '`' {
                break;
            }
            if self.current() == '$' && self.peek() == '{' {
                break;
            }
            if self.current() == '\\' {
                self.advance(); // consume '\'
                if self.at_end() {
                    break;
                }
                match self.current() {
                    'n' => { content.push('\n'); self.advance(); }
                    't' => { content.push('\t'); self.advance(); }
                    'r' => { content.push('\r'); self.advance(); }
                    '\\' => { content.push('\\'); self.advance(); }
                    '`' => { content.push('`'); self.advance(); }
                    '$' => { content.push('$'); self.advance(); }
                    '0' => { content.push('\0'); self.advance(); }
                    c => {
                        content.push('\\');
                        content.push(c);
                        self.advance();
                    }
                }
            } else {
                content.push(self.advance_char());
            }
        }

        self.make_token(TokenKind::TemplateStringPart(content), start, start_line, start_col)
    }
}

// ---------------------------------------------------------------------------
// Identifier character classification (Unicode-aware)
// ---------------------------------------------------------------------------

/// Characters that may start an identifier: Unicode letters or `_`.
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

/// Characters that may continue an identifier: Unicode letters, digits, or `_`.
fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(src);
        lexer.tokenize().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_basic_operators() {
        let toks = kinds("+ - * / %");
        assert_eq!(toks, vec![
            TokenKind::Plus, TokenKind::Minus, TokenKind::Star,
            TokenKind::Slash, TokenKind::Percent, TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_compound_assignments() {
        let toks = kinds("+= -= *= /= %= &= |= ^= <<= >>=");
        assert_eq!(toks, vec![
            TokenKind::PlusEq, TokenKind::MinusEq, TokenKind::StarEq,
            TokenKind::SlashEq, TokenKind::PercentEq, TokenKind::AmpEq,
            TokenKind::PipeEq, TokenKind::CaretEq, TokenKind::LtLtEq,
            TokenKind::GtGtEq, TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_comparison_and_shift() {
        let toks = kinds("== != < > <= >= << >> <=> |>");
        assert_eq!(toks, vec![
            TokenKind::EqEq, TokenKind::BangEq, TokenKind::Lt,
            TokenKind::Gt, TokenKind::LtEq, TokenKind::GtEq,
            TokenKind::LtLt, TokenKind::GtGt, TokenKind::Spaceship,
            TokenKind::PipeGt, TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_dots_and_arrows() {
        let toks = kinds(". .. ..= -> => :: @");
        assert_eq!(toks, vec![
            TokenKind::Dot, TokenKind::DotDot, TokenKind::DotDotEq,
            TokenKind::Arrow, TokenKind::FatArrow, TokenKind::ColonColon,
            TokenKind::At, TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_keywords() {
        let toks = kinds("let mut const fn return if else match for in while loop break continue struct impl trait enum as and or not pub self Self None Some Ok Err");
        // Just check a few
        assert_eq!(toks[0], TokenKind::Let);
        assert_eq!(toks[1], TokenKind::Mut);
        assert_eq!(toks[2], TokenKind::Const);
        assert_eq!(toks[3], TokenKind::Fn);
        assert!(matches!(toks.last(), Some(TokenKind::Eof)));
    }

    #[test]
    fn test_integer_literals() {
        assert_eq!(kinds("42"), vec![TokenKind::IntLit(42), TokenKind::Eof]);
        assert_eq!(kinds("0xff"), vec![TokenKind::IntLit(255), TokenKind::Eof]);
        assert_eq!(kinds("0b1010"), vec![TokenKind::IntLit(10), TokenKind::Eof]);
        assert_eq!(kinds("0o77"), vec![TokenKind::IntLit(63), TokenKind::Eof]);
        assert_eq!(kinds("1_000_000"), vec![TokenKind::IntLit(1_000_000), TokenKind::Eof]);
    }

    #[test]
    fn test_float_literals() {
        assert_eq!(kinds("3.14"), vec![TokenKind::FloatLit(3.14), TokenKind::Eof]);
        assert_eq!(kinds("1e10"), vec![TokenKind::FloatLit(1e10), TokenKind::Eof]);
        assert_eq!(kinds("2.5e-3"), vec![TokenKind::FloatLit(2.5e-3), TokenKind::Eof]);
    }

    #[test]
    fn test_string_literal() {
        let toks = kinds(r#""hello\nworld""#);
        assert_eq!(toks, vec![
            TokenKind::StringLit("hello\nworld".to_string()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_char_literal() {
        let toks = kinds("'a' '\\n'");
        assert_eq!(toks, vec![
            TokenKind::CharLit('a'),
            TokenKind::CharLit('\n'),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_line_comment() {
        let toks = kinds("x // comment\ny");
        assert_eq!(toks, vec![
            TokenKind::Ident(SmolStr::new("x")),
            TokenKind::Ident(SmolStr::new("y")),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_doc_comment() {
        let toks = kinds("/// this is a doc comment");
        assert_eq!(toks[0], TokenKind::DocComment("this is a doc comment".to_string()));
    }

    #[test]
    fn test_block_comment() {
        let toks = kinds("x /* block */ y");
        assert_eq!(toks, vec![
            TokenKind::Ident(SmolStr::new("x")),
            TokenKind::Ident(SmolStr::new("y")),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_block_doc_comment() {
        let toks = kinds("/** doc block */");
        assert!(matches!(&toks[0], TokenKind::DocComment(_)));
    }

    #[test]
    fn test_template_string_simple() {
        let toks = kinds("`hello`");
        assert_eq!(toks, vec![
            TokenKind::TemplateLitStart,
            TokenKind::TemplateStringPart("hello".to_string()),
            TokenKind::TemplateLitEnd,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_template_string_interpolation() {
        let toks = kinds("`hi ${name}!`");
        assert_eq!(toks, vec![
            TokenKind::TemplateLitStart,
            TokenKind::TemplateStringPart("hi ".to_string()),
            TokenKind::TemplateExprStart,
            TokenKind::Ident(SmolStr::new("name")),
            TokenKind::TemplateExprEnd,
            TokenKind::TemplateStringPart("!".to_string()),
            TokenKind::TemplateLitEnd,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_underscore_standalone() {
        let toks = kinds("_ _foo");
        assert_eq!(toks[0], TokenKind::Underscore);
        assert_eq!(toks[1], TokenKind::Ident(SmolStr::new("_foo")));
    }

    #[test]
    fn test_logical_and_bitwise() {
        let toks = kinds("&& || & | ^ ~");
        assert_eq!(toks, vec![
            TokenKind::AmpAmp, TokenKind::PipePipe,
            TokenKind::Amp, TokenKind::Pipe,
            TokenKind::Caret, TokenKind::Tilde,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_delimiters() {
        let toks = kinds("( ) { } [ ] # , ; : ?");
        assert_eq!(toks, vec![
            TokenKind::LParen, TokenKind::RParen,
            TokenKind::LBrace, TokenKind::RBrace,
            TokenKind::LBracket, TokenKind::RBracket,
            TokenKind::Hash, TokenKind::Comma,
            TokenKind::Semicolon, TokenKind::Colon,
            TokenKind::Question, TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_span_tracking() {
        let mut lexer = Lexer::new("let x = 42");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0].span.line, 1);
        assert_eq!(tokens[0].span.col, 1);
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 3);
    }

    #[test]
    fn test_unicode_identifier() {
        let toks = kinds("cafe\u{0301}");
        assert!(matches!(&toks[0], TokenKind::Ident(_)));
    }

    #[test]
    fn test_error_on_unexpected() {
        let toks = kinds("\u{ffff}");
        assert!(matches!(&toks[0], TokenKind::Error(_)));
    }
}
