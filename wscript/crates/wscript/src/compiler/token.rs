//! Token and span types for the Wscript compiler.

use smol_str::SmolStr;
use std::fmt;

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// A source location span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Byte offset of the start of the span.
    pub start: u32,
    /// Byte offset of the end of the span.
    pub end: u32,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub col: u32,
}

impl Span {
    pub fn new(start: u32, end: u32, line: u32, col: u32) -> Self {
        Self { start, end, line, col }
    }

    pub fn dummy() -> Self {
        Self::default()
    }

    /// Merge two spans into one that covers both.
    pub fn merge(self, other: Span) -> Span {
        let start = self.start.min(other.start);
        let end = self.end.max(other.end);
        let (line, col) = if self.start <= other.start {
            (self.line, self.col)
        } else {
            (other.line, other.col)
        };
        Span { start, end, line, col }
    }
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// A single token from the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// TokenKind
// ---------------------------------------------------------------------------

/// The kind of a token.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ──────────────────────────────────────────────────────
    IntLit(i128),
    FloatLit(f64),
    BoolLit(bool),
    CharLit(char),
    StringLit(String),

    // Template strings: `hello ${expr} world`
    TemplateLitStart,
    TemplateStringPart(String),
    TemplateExprStart,
    TemplateExprEnd,
    TemplateLitEnd,

    // ── Identifiers ──────────────────────────────────────────────────
    Ident(SmolStr),

    // ── Keywords ─────────────────────────────────────────────────────
    Let,
    Mut,
    Const,
    Fn,
    Return,
    If,
    Else,
    Match,
    For,
    In,
    While,
    Loop,
    Break,
    Continue,
    Struct,
    Impl,
    Trait,
    Enum,
    True,
    False,
    As,
    And,
    Or,
    Not,
    Pub,
    SelfLower,
    SelfUpper,
    KwNone,
    KwSome,
    KwOk,
    KwErr,

    // ── Operators ────────────────────────────────────────────────────
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    AmpAmp,
    PipePipe,
    Bang,
    Amp,
    Pipe,
    Caret,
    Tilde,
    LtLt,
    GtGt,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    LtLtEq,
    GtGtEq,
    PipeGt,
    Question,
    Spaceship,
    DotDot,
    DotDotEq,
    Dot,
    ColonColon,
    Arrow,
    FatArrow,
    At,

    // ── Delimiters ───────────────────────────────────────────────────
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Hash,

    // ── Punctuation ──────────────────────────────────────────────────
    Comma,
    Colon,
    Semicolon,
    Underscore,

    // ── Special ──────────────────────────────────────────────────────
    DocComment(String),
    Error(String),
    Eof,
}

impl TokenKind {
    /// Try to map a string slice to a keyword `TokenKind`.
    pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
        match s {
            "let" => Some(TokenKind::Let),
            "mut" => Some(TokenKind::Mut),
            "const" => Some(TokenKind::Const),
            "fn" => Some(TokenKind::Fn),
            "return" => Some(TokenKind::Return),
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "match" => Some(TokenKind::Match),
            "for" => Some(TokenKind::For),
            "in" => Some(TokenKind::In),
            "while" => Some(TokenKind::While),
            "loop" => Some(TokenKind::Loop),
            "break" => Some(TokenKind::Break),
            "continue" => Some(TokenKind::Continue),
            "struct" => Some(TokenKind::Struct),
            "impl" => Some(TokenKind::Impl),
            "trait" => Some(TokenKind::Trait),
            "enum" => Some(TokenKind::Enum),
            "true" => Some(TokenKind::True),
            "false" => Some(TokenKind::False),
            "as" => Some(TokenKind::As),
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),
            "not" => Some(TokenKind::Not),
            "pub" => Some(TokenKind::Pub),
            "self" => Some(TokenKind::SelfLower),
            "Self" => Some(TokenKind::SelfUpper),
            "None" => Some(TokenKind::KwNone),
            "Some" => Some(TokenKind::KwSome),
            "Ok" => Some(TokenKind::KwOk),
            "Err" => Some(TokenKind::KwErr),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Literals
            TokenKind::IntLit(v) => write!(f, "{v}"),
            TokenKind::FloatLit(v) => write!(f, "{v}"),
            TokenKind::BoolLit(v) => write!(f, "{v}"),
            TokenKind::CharLit(v) => write!(f, "'{v}'"),
            TokenKind::StringLit(v) => write!(f, "\"{v}\""),
            TokenKind::TemplateLitStart => write!(f, "`"),
            TokenKind::TemplateStringPart(s) => write!(f, "{s}"),
            TokenKind::TemplateExprStart => write!(f, "${{"),
            TokenKind::TemplateExprEnd => write!(f, "}}"),
            TokenKind::TemplateLitEnd => write!(f, "`"),

            // Identifier
            TokenKind::Ident(s) => write!(f, "{s}"),

            // Keywords
            TokenKind::Let => write!(f, "let"),
            TokenKind::Mut => write!(f, "mut"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Loop => write!(f, "loop"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Struct => write!(f, "struct"),
            TokenKind::Impl => write!(f, "impl"),
            TokenKind::Trait => write!(f, "trait"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::As => write!(f, "as"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::Pub => write!(f, "pub"),
            TokenKind::SelfLower => write!(f, "self"),
            TokenKind::SelfUpper => write!(f, "Self"),
            TokenKind::KwNone => write!(f, "None"),
            TokenKind::KwSome => write!(f, "Some"),
            TokenKind::KwOk => write!(f, "Ok"),
            TokenKind::KwErr => write!(f, "Err"),

            // Operators
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::BangEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::AmpAmp => write!(f, "&&"),
            TokenKind::PipePipe => write!(f, "||"),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::Amp => write!(f, "&"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::LtLt => write!(f, "<<"),
            TokenKind::GtGt => write!(f, ">>"),
            TokenKind::Eq => write!(f, "="),
            TokenKind::PlusEq => write!(f, "+="),
            TokenKind::MinusEq => write!(f, "-="),
            TokenKind::StarEq => write!(f, "*="),
            TokenKind::SlashEq => write!(f, "/="),
            TokenKind::PercentEq => write!(f, "%="),
            TokenKind::AmpEq => write!(f, "&="),
            TokenKind::PipeEq => write!(f, "|="),
            TokenKind::CaretEq => write!(f, "^="),
            TokenKind::LtLtEq => write!(f, "<<="),
            TokenKind::GtGtEq => write!(f, ">>="),
            TokenKind::PipeGt => write!(f, "|>"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Spaceship => write!(f, "<=>"),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::DotDotEq => write!(f, "..="),
            TokenKind::Dot => write!(f, "."),
            TokenKind::ColonColon => write!(f, "::"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::FatArrow => write!(f, "=>"),
            TokenKind::At => write!(f, "@"),

            // Delimiters
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Hash => write!(f, "#"),

            // Punctuation
            TokenKind::Comma => write!(f, ","),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Underscore => write!(f, "_"),

            // Special
            TokenKind::DocComment(s) => write!(f, "/// {s}"),
            TokenKind::Error(s) => write!(f, "<error: {s}>"),
            TokenKind::Eof => write!(f, "<eof>"),
        }
    }
}

// ---------------------------------------------------------------------------
// Numeric suffix enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumericSuffix {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatSuffix {
    F32,
    F64,
}
