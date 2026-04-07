//! Semantic token support for the LSP server.

use crate::compiler::token::TokenKind;
use tower_lsp::lsp_types::*;

/// Semantic token types used by our language.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::COMMENT,
    SemanticTokenType::TYPE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::MACRO,
    SemanticTokenType::ENUM_MEMBER,
];

/// Semantic token modifiers.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
];

const TT_KEYWORD: u32 = 0;
const _TT_FUNCTION: u32 = 1;
const TT_VARIABLE: u32 = 2;
const TT_STRING: u32 = 3;
const TT_NUMBER: u32 = 4;
const TT_OPERATOR: u32 = 5;
const TT_COMMENT: u32 = 6;
const _TT_TYPE: u32 = 7;
const _TT_PARAMETER: u32 = 8;
const TT_MACRO: u32 = 9;
const _TT_ENUM_MEMBER: u32 = 10;

/// Generate semantic tokens from a token stream.
pub fn semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let mut lexer = crate::compiler::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();

    let mut result = Vec::new();
    let mut prev_line: u32 = 0;
    let mut prev_col: u32 = 0;

    for token in &tokens {
        let token_type = match &token.kind {
            // Keywords
            TokenKind::Let
            | TokenKind::Mut
            | TokenKind::Const
            | TokenKind::Fn
            | TokenKind::Return
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Match
            | TokenKind::For
            | TokenKind::In
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Struct
            | TokenKind::Impl
            | TokenKind::Trait
            | TokenKind::Enum
            | TokenKind::As
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Not
            | TokenKind::Pub
            | TokenKind::SelfLower
            | TokenKind::SelfUpper => Some(TT_KEYWORD),

            TokenKind::True | TokenKind::False => Some(TT_KEYWORD),
            TokenKind::KwNone | TokenKind::KwSome | TokenKind::KwOk | TokenKind::KwErr => {
                Some(TT_KEYWORD)
            }

            // Literals
            TokenKind::IntLit(_) | TokenKind::FloatLit(_) => Some(TT_NUMBER),
            TokenKind::StringLit(_) | TokenKind::CharLit(_) => Some(TT_STRING),
            TokenKind::TemplateLitStart
            | TokenKind::TemplateLitEnd
            | TokenKind::TemplateStringPart(_) => Some(TT_STRING),

            // Comments
            TokenKind::DocComment(_) => Some(TT_COMMENT),

            // Operators
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::EqEq
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::LtEq
            | TokenKind::GtEq
            | TokenKind::AmpAmp
            | TokenKind::PipePipe
            | TokenKind::Bang
            | TokenKind::Amp
            | TokenKind::Pipe
            | TokenKind::Caret
            | TokenKind::Tilde
            | TokenKind::LtLt
            | TokenKind::GtGt
            | TokenKind::Eq
            | TokenKind::PipeGt
            | TokenKind::Question
            | TokenKind::Spaceship
            | TokenKind::DotDot
            | TokenKind::DotDotEq
            | TokenKind::Arrow
            | TokenKind::FatArrow => Some(TT_OPERATOR),

            // Attributes
            TokenKind::At => Some(TT_MACRO),

            // Identifiers — we'd need semantic info for better classification
            TokenKind::Ident(_) => Some(TT_VARIABLE),

            _ => None,
        };

        if let Some(tt) = token_type {
            let line = token.span.line.saturating_sub(1); // LSP uses 0-based
            let col = token.span.col.saturating_sub(1);
            let length = token.span.end.saturating_sub(token.span.start);

            let delta_line = line.saturating_sub(prev_line);
            let delta_start = if delta_line == 0 {
                col.saturating_sub(prev_col)
            } else {
                col
            };

            result.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: tt,
                token_modifiers_bitset: 0,
            });

            prev_line = line;
            prev_col = col;
        }
    }

    result
}
