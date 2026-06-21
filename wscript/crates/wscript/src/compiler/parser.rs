//! Recursive-descent parser for Wscript token streams.

use super::ast::*;
use super::token::{Span, Token, TokenKind};
use smol_str::SmolStr;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// A parse diagnostic.
#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    pub span: Span,
    pub message: String,
    pub code: Option<String>,
    pub hint: Option<String>,
}

impl ParseDiagnostic {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            code: None,
            hint: None,
        }
    }

    #[allow(dead_code)]
    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// The Wscript parser.
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    diagnostics: Vec<ParseDiagnostic>,
}

// ── helpers ──────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::dummy())
    }

    fn peek_second(&self) -> &TokenKind {
        self.tokens
            .get(self.pos + 1)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if !self.at_eof() {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    fn check_exact(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Span> {
        if self.check(kind) {
            Some(self.advance().span)
        } else {
            None
        }
    }

    fn eat_exact(&mut self, kind: &TokenKind) -> Option<Span> {
        if self.check_exact(kind) {
            Some(self.advance().span)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind) -> Span {
        if self.check(kind) {
            self.advance().span
        } else {
            let span = self.peek_span();
            self.error(span, format!("expected `{kind}`, found `{}`", self.peek()));
            span
        }
    }

    fn expect_ident(&mut self) -> SmolStr {
        if let TokenKind::Ident(s) = self.peek().clone() {
            self.advance();
            s
        } else {
            let span = self.peek_span();
            self.error(
                span,
                format!("expected identifier, found `{}`", self.peek()),
            );
            SmolStr::new("<error>")
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(ParseDiagnostic::new(span, message));
    }

    #[allow(dead_code)]
    fn error_with_hint(&mut self, span: Span, message: impl Into<String>, hint: impl Into<String>) {
        self.diagnostics
            .push(ParseDiagnostic::new(span, message).with_hint(hint));
    }

    /// Synchronize after an error by skipping to a recovery point.
    fn synchronize(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Eof
                | TokenKind::Semicolon
                | TokenKind::RBrace
                | TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Trait
                | TokenKind::Impl
                | TokenKind::Const
                | TokenKind::Let => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Synchronize, consuming a semicolon or RBrace if found.
    #[allow(dead_code)]
    fn synchronize_past_semi(&mut self) {
        self.synchronize();
        self.eat(&TokenKind::Semicolon);
    }
}

// ── program ──────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    /// Parse a complete program, returning the AST and any diagnostics.
    pub fn parse_program(&mut self) -> (Program, Vec<ParseDiagnostic>) {
        let start = self.peek_span();
        let mut items = Vec::new();

        while !self.at_eof() {
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {
                    let span = self.peek_span();
                    self.error(span, format!("unexpected token `{}`", self.peek()));
                    self.synchronize();
                    // consume recovery token if it's a semicolon
                    self.eat(&TokenKind::Semicolon);
                    items.push(Item::Error(span));
                }
            }
        }

        let end = self.peek_span();
        let span = if items.is_empty() {
            start
        } else {
            start.merge(end)
        };
        let diags = std::mem::take(&mut self.diagnostics);
        (Program { items, span }, diags)
    }
}

// ── items ────────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_item(&mut self) -> Option<Item> {
        // Skip doc comments at item level
        while matches!(self.peek(), TokenKind::DocComment(_)) {
            self.advance();
        }
        if self.at_eof() {
            return None;
        }

        // Collect attributes
        let attrs = self.parse_attrs();

        match self.peek() {
            TokenKind::Fn => Some(Item::FnDecl(self.parse_fn_decl(attrs))),
            TokenKind::Struct => Some(Item::StructDecl(self.parse_struct_decl(attrs))),
            TokenKind::Enum => Some(Item::EnumDecl(self.parse_enum_decl(attrs))),
            TokenKind::Trait => Some(Item::TraitDecl(self.parse_trait_decl(attrs))),
            TokenKind::Impl => Some(Item::ImplBlock(self.parse_impl_block())),
            TokenKind::Const => Some(Item::ConstDecl(self.parse_const_decl())),
            TokenKind::Let => Some(Item::GlobalDecl(self.parse_global_decl())),
            _ => {
                if !attrs.is_empty() {
                    let span = attrs[0].span;
                    self.error(span, "attributes must be followed by an item declaration");
                }
                None
            }
        }
    }

    // ── fn ───────────────────────────────────────────────────────────

    fn parse_fn_decl(&mut self, attrs: Vec<Attribute>) -> FnDecl {
        let start = self.expect(&TokenKind::Fn);
        let name = self.expect_ident();
        let generic_params = self.parse_optional_generic_params();
        let params = self.parse_fn_params();
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block();
        let span = start.merge(body.span);
        FnDecl {
            span,
            attrs,
            name,
            generic_params,
            params,
            return_type,
            body,
        }
    }

    fn parse_fn_params(&mut self) -> Vec<Param> {
        self.expect(&TokenKind::LParen);
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.at_eof() {
            params.push(self.parse_param());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen);
        params
    }

    fn parse_param(&mut self) -> Param {
        let start = self.peek_span();

        // self / &self / &mut self
        if self.check(&TokenKind::SelfLower) {
            self.advance();
            return Param {
                span: start,
                kind: ParamKind::SelfRef { mutable: false },
            };
        }
        if self.check(&TokenKind::Amp) {
            let amp_span = self.advance().span;
            let mutable = self.eat(&TokenKind::Mut).is_some();
            self.expect(&TokenKind::SelfLower);
            let span = amp_span.merge(self.tokens[self.pos - 1].span);
            return Param {
                span,
                kind: ParamKind::SelfRef { mutable },
            };
        }

        let name = self.expect_ident();
        self.expect(&TokenKind::Colon);
        let ty = self.parse_type();
        let default = if self.eat(&TokenKind::Eq).is_some() {
            Some(self.parse_expr())
        } else {
            None
        };
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Param {
            span,
            kind: ParamKind::Named { name, ty, default },
        }
    }

    // ── struct ───────────────────────────────────────────────────────

    fn parse_struct_decl(&mut self, attrs: Vec<Attribute>) -> StructDecl {
        let start = self.expect(&TokenKind::Struct);
        let name = self.expect_ident();
        let generic_params = self.parse_optional_generic_params();
        self.expect(&TokenKind::LBrace);
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let fstart = self.peek_span();
            let fname = self.expect_ident();
            self.expect(&TokenKind::Colon);
            let ty = self.parse_type();
            let fspan = fstart.merge(self.tokens[self.pos.saturating_sub(1)].span);
            fields.push(StructField {
                span: fspan,
                name: fname,
                ty,
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        StructDecl {
            span: start.merge(end),
            attrs,
            name,
            generic_params,
            fields,
        }
    }

    // ── enum ─────────────────────────────────────────────────────────

    fn parse_enum_decl(&mut self, attrs: Vec<Attribute>) -> EnumDecl {
        let start = self.expect(&TokenKind::Enum);
        let name = self.expect_ident();
        let generic_params = self.parse_optional_generic_params();
        self.expect(&TokenKind::LBrace);
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            variants.push(self.parse_enum_variant());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        EnumDecl {
            span: start.merge(end),
            attrs,
            name,
            generic_params,
            variants,
        }
    }

    fn parse_enum_variant(&mut self) -> EnumVariant {
        let variant_attrs = self.parse_attrs();
        let start = self.peek_span();
        let vname = self.expect_ident();
        let kind = if self.check(&TokenKind::LParen) {
            self.advance();
            let mut types = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.at_eof() {
                types.push(self.parse_type());
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen);
            VariantKind::Tuple(types)
        } else if self.check(&TokenKind::LBrace) {
            self.advance();
            let mut fields = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.at_eof() {
                let fstart = self.peek_span();
                let fname = self.expect_ident();
                self.expect(&TokenKind::Colon);
                let ty = self.parse_type();
                let fspan = fstart.merge(self.tokens[self.pos.saturating_sub(1)].span);
                fields.push(StructField {
                    span: fspan,
                    name: fname,
                    ty,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace);
            VariantKind::Struct(fields)
        } else {
            VariantKind::Unit
        };
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        EnumVariant {
            span,
            attrs: variant_attrs,
            name: vname,
            kind,
        }
    }

    // ── trait ────────────────────────────────────────────────────────

    fn parse_trait_decl(&mut self, attrs: Vec<Attribute>) -> TraitDecl {
        let start = self.expect(&TokenKind::Trait);
        let name = self.expect_ident();
        self.expect(&TokenKind::LBrace);
        let mut items = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let method_attrs = self.parse_attrs();
            if !self.check(&TokenKind::Fn) {
                let span = self.peek_span();
                self.error(span, "expected `fn` in trait body");
                self.synchronize();
                continue;
            }
            let fn_start = self.expect(&TokenKind::Fn);
            let fn_name = self.expect_ident();
            let generic_params = self.parse_optional_generic_params();
            let params = self.parse_fn_params();
            let return_type = if self.eat(&TokenKind::Arrow).is_some() {
                Some(self.parse_type())
            } else {
                None
            };
            if self.check(&TokenKind::LBrace) {
                let body = self.parse_block();
                let span = fn_start.merge(body.span);
                items.push(TraitItem::FnDecl(FnDecl {
                    span,
                    attrs: method_attrs,
                    name: fn_name,
                    generic_params,
                    params,
                    return_type,
                    body,
                }));
            } else {
                self.eat(&TokenKind::Semicolon);
                let span = fn_start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                items.push(TraitItem::FnSig(FnSig {
                    span,
                    attrs: method_attrs,
                    name: fn_name,
                    generic_params,
                    params,
                    return_type,
                }));
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        TraitDecl {
            span: start.merge(end),
            attrs,
            name,
            items,
        }
    }

    // ── impl ─────────────────────────────────────────────────────────

    fn parse_impl_block(&mut self) -> ImplBlock {
        let start = self.expect(&TokenKind::Impl);
        let generic_params = self.parse_optional_generic_params();
        let first_type = self.parse_type();

        // impl Trait for Type { ... }
        let (self_type, trait_type) = if self
            .eat_exact(&TokenKind::Ident(SmolStr::new("for")))
            .is_some()
            || self.eat(&TokenKind::For).is_some()
        {
            let st = self.parse_type();
            (st, Some(first_type))
        } else {
            (first_type, None)
        };

        self.expect(&TokenKind::LBrace);
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let method_attrs = self.parse_attrs();
            if self.check(&TokenKind::Fn) {
                methods.push(self.parse_fn_decl(method_attrs));
            } else {
                let span = self.peek_span();
                self.error(span, "expected `fn` in impl block");
                self.synchronize();
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        ImplBlock {
            span: start.merge(end),
            generic_params,
            self_type,
            trait_type,
            methods,
        }
    }

    // ── const ────────────────────────────────────────────────────────

    fn parse_const_decl(&mut self) -> ConstDecl {
        let start = self.expect(&TokenKind::Const);
        let name = self.expect_ident();
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect(&TokenKind::Eq);
        let value = self.parse_expr();
        self.expect(&TokenKind::Semicolon);
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        ConstDecl {
            span,
            name,
            ty,
            value,
        }
    }
}

// ── attributes ───────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_attrs(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        while self.check(&TokenKind::At) {
            attrs.push(self.parse_attribute());
        }
        attrs
    }

    fn parse_attribute(&mut self) -> Attribute {
        let start = self.expect(&TokenKind::At);
        let name = self.expect_ident();
        let args = if self.eat(&TokenKind::LParen).is_some() {
            let mut args = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.at_eof() {
                args.push(self.parse_attr_arg());
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen);
            args
        } else {
            Vec::new()
        };
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Attribute { span, name, args }
    }

    fn parse_attr_arg(&mut self) -> AttrArg {
        // key = value | literal | ident
        if let TokenKind::Ident(ref s) = self.peek().clone()
            && self.peek_second() == &TokenKind::Eq
        {
            let key = s.clone();
            self.advance(); // ident
            self.advance(); // =
            let val = self.parse_expr();
            return AttrArg::KeyValue(key, val);
        }
        // try literal
        match self.peek() {
            TokenKind::IntLit(_)
            | TokenKind::FloatLit(_)
            | TokenKind::StringLit(_)
            | TokenKind::CharLit(_)
            | TokenKind::True
            | TokenKind::False => {
                let expr = self.parse_expr();
                AttrArg::Literal(expr)
            }
            TokenKind::Ident(s) => {
                let s = s.clone();
                self.advance();
                AttrArg::Ident(s)
            }
            _ => {
                let span = self.peek_span();
                self.error(span, "expected attribute argument");
                self.advance();
                AttrArg::Ident(SmolStr::new("<error>"))
            }
        }
    }
}

// ── generics ─────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_optional_generic_params(&mut self) -> Option<GenericParams> {
        if !self.check(&TokenKind::Lt) {
            return None;
        }
        self.advance(); // <
        let mut params = Vec::new();
        while !self.check(&TokenKind::Gt) && !self.at_eof() {
            let name = self.expect_ident();
            let mut bounds = Vec::new();
            if self.eat(&TokenKind::Colon).is_some() {
                bounds.push(self.parse_trait_bound());
                while self.eat(&TokenKind::Plus).is_some() {
                    bounds.push(self.parse_trait_bound());
                }
            }
            params.push(GenericParam { name, bounds });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Gt);
        Some(GenericParams { params })
    }

    fn parse_trait_bound(&mut self) -> TraitBound {
        let name = self.expect_ident();
        let args = if self.check(&TokenKind::Lt) {
            self.advance();
            let mut args = Vec::new();
            while !self.check(&TokenKind::Gt) && !self.at_eof() {
                args.push(self.parse_type());
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::Gt);
            Some(args)
        } else {
            None
        };
        TraitBound { name, args }
    }

    // `+` token check helper (not in TokenKind as keyword, it's Plus)
    #[allow(dead_code)]
    fn eat_plus(&mut self) -> bool {
        self.eat(&TokenKind::Plus).is_some()
    }
}

// ── types ────────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_type(&mut self) -> TypeExpr {
        let start = self.peek_span();
        match self.peek().clone() {
            // Unit type ()
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen).is_some() {
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return TypeExpr {
                        span,
                        kind: TypeExprKind::Unit,
                    };
                }
                // Tuple type (T, U, ...)
                let mut types = vec![self.parse_type()];
                while self.eat(&TokenKind::Comma).is_some() {
                    if self.check(&TokenKind::RParen) {
                        break;
                    }
                    types.push(self.parse_type());
                }
                self.expect(&TokenKind::RParen);
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                if types.len() == 1 {
                    // parenthesized type — just unwrap
                    return types.into_iter().next().unwrap();
                }
                TypeExpr {
                    span,
                    kind: TypeExprKind::Tuple(types),
                }
            }

            // Reference type &T or &mut T
            TokenKind::Amp => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut).is_some();
                let inner = self.parse_type();
                let span = start.merge(inner.span);
                TypeExpr {
                    span,
                    kind: TypeExprKind::RefType {
                        inner: Box::new(inner),
                        mutable,
                    },
                }
            }

            // Fn(A, B) -> R
            TokenKind::Fn => {
                self.advance();
                self.expect(&TokenKind::LParen);
                let mut params = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.at_eof() {
                    params.push(self.parse_type());
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen);
                self.expect(&TokenKind::Arrow);
                let ret = self.parse_type();
                let span = start.merge(ret.span);
                TypeExpr {
                    span,
                    kind: TypeExprKind::FnType {
                        params,
                        ret: Box::new(ret),
                    },
                }
            }

            TokenKind::Ident(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for T[] array sugar
                if self.check(&TokenKind::LBracket) && self.peek_second() == &TokenKind::RBracket {
                    let inner_type = self.resolve_named_type(start, &name, None);
                    self.advance(); // [
                    let end = self.expect(&TokenKind::RBracket);
                    let span = start.merge(end);
                    return TypeExpr {
                        span,
                        kind: TypeExprKind::Array(Box::new(inner_type)),
                    };
                }

                // Primitive types
                if let Some(prim) = self.str_to_primitive(&name) {
                    let span = start;
                    return TypeExpr {
                        span,
                        kind: TypeExprKind::Primitive(prim),
                    };
                }

                // String type
                if name.as_str() == "String" || name.as_str() == "str" {
                    return TypeExpr {
                        span: start,
                        kind: TypeExprKind::StringType,
                    };
                }

                // Generic built-in types with <...>
                let args = if self.check(&TokenKind::Lt) {
                    Some(self.parse_type_args())
                } else {
                    None
                };

                self.resolve_named_type(start, &name, args)
            }

            TokenKind::SelfUpper => {
                self.advance();
                TypeExpr {
                    span: start,
                    kind: TypeExprKind::Named {
                        name: SmolStr::new("Self"),
                        args: None,
                    },
                }
            }

            _ => {
                self.error(start, format!("expected type, found `{}`", self.peek()));
                self.advance();
                TypeExpr {
                    span: start,
                    kind: TypeExprKind::Error,
                }
            }
        }
    }

    fn resolve_named_type(&self, start: Span, name: &str, args: Option<Vec<TypeExpr>>) -> TypeExpr {
        let last_span = self.tokens[self.pos.saturating_sub(1)].span;
        let span = start.merge(last_span);
        match name {
            "Option" => match args {
                Some(mut a) if a.len() == 1 => TypeExpr {
                    span,
                    kind: TypeExprKind::OptionType(Box::new(a.remove(0))),
                },
                other => TypeExpr {
                    span,
                    kind: TypeExprKind::Named {
                        name: SmolStr::new(name),
                        args: other,
                    },
                },
            },
            "Result" => match args {
                Some(mut a) if !a.is_empty() => {
                    let ok = a.remove(0);
                    let err = if !a.is_empty() {
                        Some(Box::new(a.remove(0)))
                    } else {
                        None
                    };
                    TypeExpr {
                        span,
                        kind: TypeExprKind::ResultType(Box::new(ok), err),
                    }
                }
                other => TypeExpr {
                    span,
                    kind: TypeExprKind::Named {
                        name: SmolStr::new(name),
                        args: other,
                    },
                },
            },
            "Map" => match args {
                Some(mut a) if a.len() == 2 => {
                    let v = a.remove(1);
                    let k = a.remove(0);
                    TypeExpr {
                        span,
                        kind: TypeExprKind::Map(Box::new(k), Box::new(v)),
                    }
                }
                other => TypeExpr {
                    span,
                    kind: TypeExprKind::Named {
                        name: SmolStr::new(name),
                        args: other,
                    },
                },
            },
            _ => TypeExpr {
                span,
                kind: TypeExprKind::Named {
                    name: SmolStr::new(name),
                    args,
                },
            },
        }
    }

    fn parse_type_args(&mut self) -> Vec<TypeExpr> {
        self.expect(&TokenKind::Lt);
        let mut args = Vec::new();
        while !self.check(&TokenKind::Gt) && !self.at_eof() {
            args.push(self.parse_type());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Gt);
        args
    }

    fn str_to_primitive(&self, s: &str) -> Option<PrimitiveType> {
        match s {
            "i8" => Some(PrimitiveType::I8),
            "i16" => Some(PrimitiveType::I16),
            "i32" => Some(PrimitiveType::I32),
            "i64" => Some(PrimitiveType::I64),
            "i128" => Some(PrimitiveType::I128),
            "u8" => Some(PrimitiveType::U8),
            "u16" => Some(PrimitiveType::U16),
            "u32" => Some(PrimitiveType::U32),
            "u64" => Some(PrimitiveType::U64),
            "u128" => Some(PrimitiveType::U128),
            "f32" => Some(PrimitiveType::F32),
            "f64" => Some(PrimitiveType::F64),
            "bool" => Some(PrimitiveType::Bool),
            "char" => Some(PrimitiveType::Char),
            _ => None,
        }
    }
}

// ── block & statements ───────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_block(&mut self) -> Block {
        let start = self.expect(&TokenKind::LBrace);
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => {
                    let span = self.peek_span();
                    self.error(span, format!("unexpected token `{}` in block", self.peek()));
                    self.synchronize();
                    self.eat(&TokenKind::Semicolon);
                    stmts.push(Stmt::Error(span));
                }
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        Block {
            span: start.merge(end),
            stmts,
        }
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.peek() {
            TokenKind::Let => Some(self.parse_let_stmt()),
            // Item-level statements inside blocks
            TokenKind::Fn
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Trait
            | TokenKind::Impl
            | TokenKind::Const => {
                let attrs = self.parse_attrs();
                self.parse_item_with_attrs(attrs)
                    .map(|item| Stmt::Item(Box::new(item)))
            }
            TokenKind::At => {
                let attrs = self.parse_attrs();
                // After attributes, expect an item
                self.parse_item_with_attrs(attrs)
                    .map(|item| Stmt::Item(Box::new(item)))
            }
            TokenKind::RBrace | TokenKind::Eof => None,
            _ => Some(self.parse_expr_stmt()),
        }
    }

    fn parse_item_with_attrs(&mut self, attrs: Vec<Attribute>) -> Option<Item> {
        match self.peek() {
            TokenKind::Fn => Some(Item::FnDecl(self.parse_fn_decl(attrs))),
            TokenKind::Struct => Some(Item::StructDecl(self.parse_struct_decl(attrs))),
            TokenKind::Enum => Some(Item::EnumDecl(self.parse_enum_decl(attrs))),
            TokenKind::Trait => Some(Item::TraitDecl(self.parse_trait_decl(attrs))),
            TokenKind::Impl => Some(Item::ImplBlock(self.parse_impl_block())),
            TokenKind::Const => Some(Item::ConstDecl(self.parse_const_decl())),
            TokenKind::Let => Some(Item::GlobalDecl(self.parse_global_decl())),
            _ => {
                let span = self.peek_span();
                self.error(span, "expected item declaration after attributes");
                None
            }
        }
    }

    fn parse_global_decl(&mut self) -> GlobalDecl {
        let start = self.expect(&TokenKind::Let);
        let mutable = self.eat(&TokenKind::Mut).is_some();
        let name = self.expect_ident();
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect(&TokenKind::Eq);
        let value = self.parse_expr();
        self.expect(&TokenKind::Semicolon);
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        GlobalDecl {
            span,
            name,
            mutable,
            ty,
            value,
        }
    }

    fn parse_let_stmt(&mut self) -> Stmt {
        let start = self.expect(&TokenKind::Let);
        let mutable = self.eat(&TokenKind::Mut).is_some();
        let pattern = self.parse_pattern();
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        let init = if self.eat(&TokenKind::Eq).is_some() {
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect(&TokenKind::Semicolon);
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Stmt::Let(LetStmt {
            span,
            mutable,
            pattern,
            ty,
            init,
        })
    }

    fn parse_expr_stmt(&mut self) -> Stmt {
        let start = self.peek_span();
        let expr = self.parse_expr();
        let has_semicolon = self.eat(&TokenKind::Semicolon).is_some();
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Stmt::Expr(ExprStmt {
            span,
            expr,
            has_semicolon,
        })
    }
}

// ── expressions ──────────────────────────────────────────────────────────
//
// Precedence (low to high):
//  1  Assignment       = += -= *= /= %= &= |= ^= <<= >>=
//  2  Pipe             |>
//  3  Or               or ||
//  4  And              and &&
//  5  Equality         == !=
//  6  Comparison       < > <= >= <=>
//  7  Bitwise or       |
//  8  Bitwise xor      ^
//  9  Bitwise and      &
// 10  Shift            << >>
// 11  Additive         + -
// 12  Multiplicative   * / %
// 13  Unary            - ! ~ & not
// 14  Postfix          . () [] ? as

impl<'a> Parser<'a> {
    fn parse_expr(&mut self) -> Expr {
        self.parse_assignment()
    }

    // ── 1. Assignment ────────────────────────────────────────────────

    fn parse_assignment(&mut self) -> Expr {
        let expr = self.parse_pipe();
        if let Some(op) = self.peek_assign_op() {
            self.advance();
            let rhs = self.parse_assignment(); // right-associative
            let span = expr.span.merge(rhs.span);
            Expr {
                span,
                kind: ExprKind::Assign {
                    op,
                    target: Box::new(expr),
                    value: Box::new(rhs),
                },
            }
        } else {
            expr
        }
    }

    fn peek_assign_op(&self) -> Option<AssignOp> {
        match self.peek() {
            TokenKind::Eq => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::AddAssign),
            TokenKind::MinusEq => Some(AssignOp::SubAssign),
            TokenKind::StarEq => Some(AssignOp::MulAssign),
            TokenKind::SlashEq => Some(AssignOp::DivAssign),
            TokenKind::PercentEq => Some(AssignOp::RemAssign),
            TokenKind::AmpEq => Some(AssignOp::BitAndAssign),
            TokenKind::PipeEq => Some(AssignOp::BitOrAssign),
            TokenKind::CaretEq => Some(AssignOp::BitXorAssign),
            TokenKind::LtLtEq => Some(AssignOp::ShlAssign),
            TokenKind::GtGtEq => Some(AssignOp::ShrAssign),
            _ => None,
        }
    }

    // ── 2. Pipe ──────────────────────────────────────────────────────

    fn parse_pipe(&mut self) -> Expr {
        let mut expr = self.parse_or();
        while self.eat(&TokenKind::PipeGt).is_some() {
            let rhs = self.parse_or();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Pipe {
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 3. Or ────────────────────────────────────────────────────────

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_and();
        while self.check(&TokenKind::Or) || self.check(&TokenKind::PipePipe) {
            self.advance();
            let rhs = self.parse_and();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 4. And ───────────────────────────────────────────────────────

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_equality();
        while self.check(&TokenKind::And) || self.check(&TokenKind::AmpAmp) {
            self.advance();
            let rhs = self.parse_equality();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 5. Equality ─────────────────────────────────────────────────

    fn parse_equality(&mut self) -> Expr {
        let mut expr = self.parse_comparison();
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Neq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_comparison();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 6. Comparison ───────────────────────────────────────────────

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_bitwise_or();
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::LtEq,
                TokenKind::GtEq => BinOp::GtEq,
                TokenKind::Spaceship => BinOp::Spaceship,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_bitwise_or();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 7. Bitwise or ───────────────────────────────────────────────

    fn parse_bitwise_or(&mut self) -> Expr {
        let mut expr = self.parse_bitwise_xor();
        while self.check(&TokenKind::Pipe) {
            self.advance();
            let rhs = self.parse_bitwise_xor();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::BitOr,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 8. Bitwise xor ─────────────────────────────────────────────

    fn parse_bitwise_xor(&mut self) -> Expr {
        let mut expr = self.parse_bitwise_and();
        while self.check(&TokenKind::Caret) {
            self.advance();
            let rhs = self.parse_bitwise_and();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::BitXor,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 9. Bitwise and ─────────────────────────────────────────────

    fn parse_bitwise_and(&mut self) -> Expr {
        let mut expr = self.parse_shift();
        while self.check(&TokenKind::Amp) {
            self.advance();
            let rhs = self.parse_shift();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::BitAnd,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 10. Shift ───────────────────────────────────────────────────

    fn parse_shift(&mut self) -> Expr {
        let mut expr = self.parse_additive();
        loop {
            let op = match self.peek() {
                TokenKind::LtLt => BinOp::Shl,
                TokenKind::GtGt => BinOp::Shr,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_additive();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 11. Additive ────────────────────────────────────────────────

    fn parse_additive(&mut self) -> Expr {
        let mut expr = self.parse_multiplicative();
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 12. Multiplicative ──────────────────────────────────────────

    fn parse_multiplicative(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary();
            let span = expr.span.merge(rhs.span);
            expr = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
            };
        }
        expr
    }

    // ── 13. Unary ───────────────────────────────────────────────────

    fn parse_unary(&mut self) -> Expr {
        let start = self.peek_span();
        match self.peek() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(operand.span);
                Expr {
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                }
            }
            TokenKind::Bang | TokenKind::Not => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(operand.span);
                Expr {
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                }
            }
            TokenKind::Tilde => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(operand.span);
                Expr {
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::BitNot,
                        operand: Box::new(operand),
                    },
                }
            }
            TokenKind::Amp => {
                self.advance();
                let (op, operand) = if self.eat(&TokenKind::Mut).is_some() {
                    (UnaryOp::RefMut, self.parse_unary())
                } else {
                    (UnaryOp::Ref, self.parse_unary())
                };
                let span = start.merge(operand.span);
                Expr {
                    span,
                    kind: ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                }
            }
            TokenKind::Star => {
                self.advance();
                let operand = self.parse_unary();
                let span = start.merge(operand.span);
                Expr {
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::Deref,
                        operand: Box::new(operand),
                    },
                }
            }
            _ => self.parse_postfix(),
        }
    }

    // ── 14. Postfix ─────────────────────────────────────────────────

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            match self.peek() {
                // field access / method call / tuple index: expr.name or expr.0
                TokenKind::Dot => {
                    self.advance();
                    match self.peek().clone() {
                        TokenKind::Ident(field) => {
                            self.advance();
                            if self.check(&TokenKind::LParen) {
                                // method call
                                let args = self.parse_call_args();
                                let span = expr
                                    .span
                                    .merge(self.tokens[self.pos.saturating_sub(1)].span);
                                expr = Expr {
                                    span,
                                    kind: ExprKind::MethodCall {
                                        object: Box::new(expr),
                                        method: field,
                                        args,
                                    },
                                };
                            } else {
                                let span = expr
                                    .span
                                    .merge(self.tokens[self.pos.saturating_sub(1)].span);
                                expr = Expr {
                                    span,
                                    kind: ExprKind::FieldAccess {
                                        object: Box::new(expr),
                                        field,
                                    },
                                };
                            }
                        }
                        TokenKind::IntLit(idx) => {
                            self.advance();
                            let span = expr
                                .span
                                .merge(self.tokens[self.pos.saturating_sub(1)].span);
                            expr = Expr {
                                span,
                                kind: ExprKind::TupleIndex {
                                    object: Box::new(expr),
                                    index: idx as u32,
                                },
                            };
                        }
                        _ => {
                            let span = self.peek_span();
                            self.error(span, "expected field name or tuple index after `.`");
                            break;
                        }
                    }
                }
                // function call: expr(args)
                TokenKind::LParen => {
                    let args = self.parse_call_args();
                    let span = expr
                        .span
                        .merge(self.tokens[self.pos.saturating_sub(1)].span);
                    expr = Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                    };
                }
                // index: expr[idx]
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.parse_expr();
                    self.expect(&TokenKind::RBracket);
                    let span = expr
                        .span
                        .merge(self.tokens[self.pos.saturating_sub(1)].span);
                    expr = Expr {
                        span,
                        kind: ExprKind::Index {
                            object: Box::new(expr),
                            index: Box::new(index),
                        },
                    };
                }
                // error propagation: expr?
                TokenKind::Question => {
                    self.advance();
                    let span = expr
                        .span
                        .merge(self.tokens[self.pos.saturating_sub(1)].span);
                    expr = Expr {
                        span,
                        kind: ExprKind::ErrorPropagate(Box::new(expr)),
                    };
                }
                // cast: expr as Type
                TokenKind::As => {
                    self.advance();
                    let ty = self.parse_type();
                    let span = expr.span.merge(ty.span);
                    expr = Expr {
                        span,
                        kind: ExprKind::Cast {
                            expr: Box::new(expr),
                            ty,
                        },
                    };
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_call_args(&mut self) -> Vec<CallArg> {
        self.expect(&TokenKind::LParen);
        let mut args = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.at_eof() {
            args.push(self.parse_call_arg());
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen);
        args
    }

    fn parse_call_arg(&mut self) -> CallArg {
        // Check for named argument: name: value
        if let TokenKind::Ident(ref name) = self.peek().clone()
            && self.peek_second() == &TokenKind::Colon
        {
            let name = name.clone();
            self.advance(); // ident
            self.advance(); // :
            let value = self.parse_expr();
            return CallArg {
                name: Some(name),
                value,
            };
        }
        let value = self.parse_expr();
        CallArg { name: None, value }
    }

    // ── primary expressions ─────────────────────────────────────────

    fn parse_primary(&mut self) -> Expr {
        let start = self.peek_span();
        match self.peek().clone() {
            // Integer literal
            TokenKind::IntLit(v) => {
                self.advance();
                // Check for range: ..
                if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEq) {
                    return self.parse_range_from(Some(Expr {
                        span: start,
                        kind: ExprKind::IntLit(v),
                    }));
                }
                Expr {
                    span: start,
                    kind: ExprKind::IntLit(v),
                }
            }
            TokenKind::FloatLit(v) => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::FloatLit(v),
                }
            }
            TokenKind::True => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::BoolLit(true),
                }
            }
            TokenKind::False => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::BoolLit(false),
                }
            }
            TokenKind::BoolLit(v) => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::BoolLit(v),
                }
            }
            TokenKind::CharLit(v) => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::CharLit(v),
                }
            }
            TokenKind::StringLit(v) => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::StringLit(v),
                }
            }

            // Template literal
            TokenKind::TemplateLitStart => self.parse_template_lit(),

            // Identifier (variable, path, struct init)
            TokenKind::Ident(name) => {
                self.advance();

                // Path: a::b::c
                if self.check(&TokenKind::ColonColon) {
                    return self.parse_path_or_call(start, name);
                }

                // Struct init: Name { field: val, ... }
                if self.check(&TokenKind::LBrace) && self.looks_like_struct_init() {
                    return self.parse_struct_init(start, name);
                }

                // Macro call: name!(args)
                if self.check(&TokenKind::Bang) && self.peek_second() == &TokenKind::LParen {
                    self.advance(); // !
                    self.expect(&TokenKind::LParen);
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        args.push(self.parse_expr());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return Expr {
                        span,
                        kind: ExprKind::MacroCall { name, args },
                    };
                }

                Expr {
                    span: start,
                    kind: ExprKind::Ident(name),
                }
            }

            TokenKind::SelfLower => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("self")),
                }
            }
            TokenKind::SelfUpper => {
                self.advance();
                if self.check(&TokenKind::ColonColon) {
                    return self.parse_path_or_call(start, SmolStr::new("Self"));
                }
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("Self")),
                }
            }

            // Keywords that start expressions: KwNone, KwSome, KwOk, KwErr
            TokenKind::KwNone => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("None")),
                }
            }
            TokenKind::KwSome => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    let args = self.parse_call_args();
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(Expr {
                                span: start,
                                kind: ExprKind::Ident(SmolStr::new("Some")),
                            }),
                            args,
                        },
                    };
                }
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("Some")),
                }
            }
            TokenKind::KwOk => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    let args = self.parse_call_args();
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(Expr {
                                span: start,
                                kind: ExprKind::Ident(SmolStr::new("Ok")),
                            }),
                            args,
                        },
                    };
                }
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("Ok")),
                }
            }
            TokenKind::KwErr => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    let args = self.parse_call_args();
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(Expr {
                                span: start,
                                kind: ExprKind::Ident(SmolStr::new("Err")),
                            }),
                            args,
                        },
                    };
                }
                Expr {
                    span: start,
                    kind: ExprKind::Ident(SmolStr::new("Err")),
                }
            }

            // Parenthesized expression or tuple: (expr) or (a, b, ...)
            TokenKind::LParen => {
                self.advance();
                // Unit literal: ()
                if self.eat(&TokenKind::RParen).is_some() {
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    return Expr {
                        span,
                        kind: ExprKind::UnitLit,
                    };
                }
                let first = self.parse_expr();
                if self.eat(&TokenKind::Comma).is_some() {
                    // Tuple literal
                    let mut elems = vec![first];
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        elems.push(self.parse_expr());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Expr {
                        span,
                        kind: ExprKind::TupleLit(elems),
                    }
                } else {
                    // Parenthesized expression
                    self.expect(&TokenKind::RParen);
                    first
                }
            }

            // Array literal: [a, b, c]
            TokenKind::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while !self.check(&TokenKind::RBracket) && !self.at_eof() {
                    elems.push(self.parse_expr());
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket);
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Expr {
                    span,
                    kind: ExprKind::ArrayLit(elems),
                }
            }

            // Map literal: { key: val, ... } — but only if it looks like one
            // Block expression: { stmts }
            TokenKind::LBrace => {
                if self.looks_like_map_lit() {
                    self.parse_map_lit()
                } else {
                    let block = self.parse_block();
                    let span = block.span;
                    Expr {
                        span,
                        kind: ExprKind::Block(block),
                    }
                }
            }

            // If expression
            TokenKind::If => self.parse_if_expr(),

            // Match expression
            TokenKind::Match => self.parse_match_expr(),

            // For expression
            TokenKind::For => self.parse_for_expr(),

            // While expression
            TokenKind::While => self.parse_while_expr(),

            // Loop expression
            TokenKind::Loop => {
                self.advance();
                let body = self.parse_block();
                let span = start.merge(body.span);
                Expr {
                    span,
                    kind: ExprKind::Loop { body },
                }
            }

            // Return expression
            TokenKind::Return => {
                self.advance();
                let value = if !self.check(&TokenKind::Semicolon)
                    && !self.check(&TokenKind::RBrace)
                    && !self.at_eof()
                {
                    Some(Box::new(self.parse_expr()))
                } else {
                    None
                };
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Expr {
                    span,
                    kind: ExprKind::Return(value),
                }
            }

            // Break expression
            TokenKind::Break => {
                self.advance();
                let value = if !self.check(&TokenKind::Semicolon)
                    && !self.check(&TokenKind::RBrace)
                    && !self.at_eof()
                {
                    Some(Box::new(self.parse_expr()))
                } else {
                    None
                };
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Expr {
                    span,
                    kind: ExprKind::Break(value),
                }
            }

            // Continue expression
            TokenKind::Continue => {
                self.advance();
                Expr {
                    span: start,
                    kind: ExprKind::Continue,
                }
            }

            // Lambda: |params| body  or  |params| -> Type body
            TokenKind::Pipe => self.parse_lambda(),
            TokenKind::PipePipe => {
                // || is empty param list lambda
                self.advance();
                let return_type = if self.eat(&TokenKind::Arrow).is_some() {
                    Some(Box::new(self.parse_type()))
                } else {
                    None
                };
                let body = self.parse_expr();
                let span = start.merge(body.span);
                Expr {
                    span,
                    kind: ExprKind::Lambda {
                        params: Vec::new(),
                        return_type,
                        body: Box::new(body),
                    },
                }
            }

            // Range without start: ..end or ..=end
            TokenKind::DotDot | TokenKind::DotDotEq => self.parse_range_from(None),

            _ => {
                let span = self.peek_span();
                self.error(
                    span,
                    format!("expected expression, found `{}`", self.peek()),
                );
                self.advance();
                Expr {
                    span,
                    kind: ExprKind::Error,
                }
            }
        }
    }

    // ── path / enum variant call ────────────────────────────────────

    fn parse_path_or_call(&mut self, start: Span, first: SmolStr) -> Expr {
        let mut segments = vec![first];
        while self.eat(&TokenKind::ColonColon).is_some() {
            let name = self.expect_ident();
            segments.push(name);
        }
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);

        // Struct init: Path { ... }
        if !segments.is_empty() && self.check(&TokenKind::LBrace) && self.looks_like_struct_init() {
            let name = segments.last().unwrap().clone();
            return self.parse_struct_init(start, name);
        }

        if segments.len() == 1 {
            Expr {
                span,
                kind: ExprKind::Ident(segments.into_iter().next().unwrap()),
            }
        } else {
            Expr {
                span,
                kind: ExprKind::Path(segments),
            }
        }
    }

    // ── struct init ─────────────────────────────────────────────────

    fn looks_like_struct_init(&self) -> bool {
        // Peek ahead: { ident : ... } or { ident , ... } or { }
        // vs block: { stmt; ... }
        if !self.check(&TokenKind::LBrace) {
            return false;
        }
        let saved = self.pos + 1;
        match self.tokens.get(saved).map(|t| &t.kind) {
            Some(TokenKind::RBrace) => true,
            Some(TokenKind::Ident(_)) => matches!(
                self.tokens.get(saved + 1).map(|t| &t.kind),
                Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace)
            ),
            _ => false,
        }
    }

    fn parse_struct_init(&mut self, start: Span, name: SmolStr) -> Expr {
        self.expect(&TokenKind::LBrace);
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let fname = self.expect_ident();
            let value = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_expr())
            } else {
                None
            };
            fields.push(FieldInit { name: fname, value });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBrace);
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Expr {
            span,
            kind: ExprKind::StructInit { name, fields },
        }
    }

    // ── map literal ─────────────────────────────────────────────────

    fn looks_like_map_lit(&self) -> bool {
        // { expr : expr, ... } — first token after { followed by : (not ident: type)
        // We check: { <non-ident-or-keyword> or { ident : expr :
        // Heuristic: { <literal> :
        if !self.check(&TokenKind::LBrace) {
            return false;
        }
        let p1 = self.tokens.get(self.pos + 1).map(|t| &t.kind);
        let p2 = self.tokens.get(self.pos + 2).map(|t| &t.kind);
        match (p1, p2) {
            (Some(TokenKind::StringLit(_)), Some(TokenKind::Colon)) => true,
            (Some(TokenKind::IntLit(_)), Some(TokenKind::Colon)) => true,
            (Some(TokenKind::RBrace), _) => false, // empty block, not map
            _ => false,
        }
    }

    fn parse_map_lit(&mut self) -> Expr {
        let start = self.expect(&TokenKind::LBrace);
        let mut entries = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let key = self.parse_expr();
            self.expect(&TokenKind::Colon);
            let value = self.parse_expr();
            entries.push((key, value));
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self.expect(&TokenKind::RBrace);
        Expr {
            span: start.merge(end),
            kind: ExprKind::MapLit(entries),
        }
    }

    // ── template literal ────────────────────────────────────────────

    fn parse_template_lit(&mut self) -> Expr {
        let start = self.expect(&TokenKind::TemplateLitStart);
        let mut segments = Vec::new();
        loop {
            match self.peek().clone() {
                TokenKind::TemplateStringPart(s) => {
                    self.advance();
                    segments.push(TemplateSegment::Literal(s));
                }
                TokenKind::TemplateExprStart => {
                    self.advance();
                    let expr = self.parse_expr();
                    segments.push(TemplateSegment::Expr(expr));
                    self.expect(&TokenKind::TemplateExprEnd);
                }
                TokenKind::TemplateLitEnd => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => {
                    self.error(self.peek_span(), "unterminated template literal");
                    break;
                }
                _ => {
                    self.error(self.peek_span(), "unexpected token in template literal");
                    self.advance();
                    break;
                }
            }
        }
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Expr {
            span,
            kind: ExprKind::TemplateLit(segments),
        }
    }

    // ── range ───────────────────────────────────────────────────────

    fn parse_range_from(&mut self, start_expr: Option<Expr>) -> Expr {
        let range_start = start_expr
            .as_ref()
            .map(|e| e.span)
            .unwrap_or(self.peek_span());
        let inclusive = self.check(&TokenKind::DotDotEq);
        self.advance(); // consume .. or ..=

        // end expression (optional)
        let end_expr = if !self.check(&TokenKind::Semicolon)
            && !self.check(&TokenKind::RBrace)
            && !self.check(&TokenKind::RParen)
            && !self.check(&TokenKind::RBracket)
            && !self.check(&TokenKind::Comma)
            && !self.at_eof()
        {
            Some(Box::new(self.parse_pipe()))
        } else {
            None
        };
        let span = range_start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Expr {
            span,
            kind: ExprKind::Range {
                start: start_expr.map(Box::new),
                end: end_expr,
                inclusive,
            },
        }
    }

    // ── lambda ──────────────────────────────────────────────────────

    fn parse_lambda(&mut self) -> Expr {
        let start = self.expect(&TokenKind::Pipe);
        let mut params = Vec::new();
        while !self.check(&TokenKind::Pipe) && !self.at_eof() {
            let name = self.expect_ident();
            let ty = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_type())
            } else {
                None
            };
            params.push(LambdaParam { name, ty });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Pipe);
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(Box::new(self.parse_type()))
        } else {
            None
        };
        let body = self.parse_expr();
        let span = start.merge(body.span);
        Expr {
            span,
            kind: ExprKind::Lambda {
                params,
                return_type,
                body: Box::new(body),
            },
        }
    }

    // ── if ──────────────────────────────────────────────────────────

    fn parse_if_expr(&mut self) -> Expr {
        let start = self.expect(&TokenKind::If);

        // if let pattern = expr { ... }
        if self.check(&TokenKind::Let) {
            self.advance();
            let pattern = self.parse_pattern();
            self.expect(&TokenKind::Eq);
            let scrutinee = self.parse_expr_no_struct_init();
            let then_block = self.parse_block();
            let else_block = if self.eat(&TokenKind::Else).is_some() {
                Some(Box::new(if self.check(&TokenKind::If) {
                    self.parse_if_expr()
                } else {
                    let block = self.parse_block();
                    let span = block.span;
                    Expr {
                        span,
                        kind: ExprKind::Block(block),
                    }
                }))
            } else {
                None
            };
            let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
            return Expr {
                span,
                kind: ExprKind::IfLet {
                    pattern,
                    expr: Box::new(scrutinee),
                    then_block,
                    else_block,
                },
            };
        }

        let condition = self.parse_expr_no_struct_init();
        let then_block = self.parse_block();
        let else_block = if self.eat(&TokenKind::Else).is_some() {
            Some(Box::new(if self.check(&TokenKind::If) {
                self.parse_if_expr()
            } else {
                let block = self.parse_block();
                let span = block.span;
                Expr {
                    span,
                    kind: ExprKind::Block(block),
                }
            }))
        } else {
            None
        };
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Expr {
            span,
            kind: ExprKind::If {
                condition: Box::new(condition),
                then_block,
                else_block,
            },
        }
    }

    /// Parse an expression but disallow struct init at top level (to avoid
    /// ambiguity with block in `if cond { ... }`).
    fn parse_expr_no_struct_init(&mut self) -> Expr {
        // Same as parse_expr but the primary ident branch won't parse struct init
        // We use a simple approach: parse normally and it works because `looks_like_struct_init`
        // is a heuristic. For conditions, we parse pipe-level to avoid assignment ambiguity too.
        self.parse_pipe_no_struct()
    }

    fn parse_pipe_no_struct(&mut self) -> Expr {
        // For now, just delegate. The struct init heuristic already requires
        // `ident { ident : ...}` which wouldn't appear in conditions.
        self.parse_pipe()
    }

    // ── match ───────────────────────────────────────────────────────

    fn parse_match_expr(&mut self) -> Expr {
        let start = self.expect(&TokenKind::Match);
        let scrutinee = self.parse_expr_no_struct_init();
        self.expect(&TokenKind::LBrace);
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            arms.push(self.parse_match_arm());
            // allow optional comma between arms
            self.eat(&TokenKind::Comma);
        }
        let end = self.expect(&TokenKind::RBrace);
        Expr {
            span: start.merge(end),
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
        }
    }

    fn parse_match_arm(&mut self) -> MatchArm {
        let start = self.peek_span();
        let pattern = self.parse_pattern();
        let guard = if self.check(&TokenKind::If) {
            self.advance();
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect(&TokenKind::FatArrow);
        let body = self.parse_expr();
        let span = start.merge(body.span);
        MatchArm {
            span,
            pattern,
            guard,
            body,
        }
    }

    // ── for ─────────────────────────────────────────────────────────

    fn parse_for_expr(&mut self) -> Expr {
        let start = self.expect(&TokenKind::For);
        let pattern = self.parse_pattern();
        self.expect(&TokenKind::In);
        let iterable = self.parse_expr_no_struct_init();
        let body = self.parse_block();
        let span = start.merge(body.span);
        Expr {
            span,
            kind: ExprKind::For {
                pattern,
                iterable: Box::new(iterable),
                body,
            },
        }
    }

    // ── while ───────────────────────────────────────────────────────

    fn parse_while_expr(&mut self) -> Expr {
        let start = self.expect(&TokenKind::While);

        // while let pattern = expr { ... }
        if self.check(&TokenKind::Let) {
            self.advance();
            let pattern = self.parse_pattern();
            self.expect(&TokenKind::Eq);
            let scrutinee = self.parse_expr_no_struct_init();
            let body = self.parse_block();
            let span = start.merge(body.span);
            return Expr {
                span,
                kind: ExprKind::WhileLet {
                    pattern,
                    expr: Box::new(scrutinee),
                    body,
                },
            };
        }

        let condition = self.parse_expr_no_struct_init();
        let body = self.parse_block();
        let span = start.merge(body.span);
        Expr {
            span,
            kind: ExprKind::While {
                condition: Box::new(condition),
                body,
            },
        }
    }
}

// ── patterns ─────────────────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_pattern(&mut self) -> Pattern {
        let start = self.peek_span();
        let mut pat = match self.peek().clone() {
            // Wildcard
            TokenKind::Underscore => {
                self.advance();
                Pattern::Wildcard(start)
            }

            // Literal patterns
            TokenKind::IntLit(v) => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::IntLit(v),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::FloatLit(v) => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::FloatLit(v),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::StringLit(v) => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::StringLit(v),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::CharLit(v) => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::CharLit(v),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::True => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::BoolLit(true),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::False => {
                self.advance();
                let expr = Expr {
                    span: start,
                    kind: ExprKind::BoolLit(false),
                };
                Pattern::Literal {
                    span: start,
                    expr: Box::new(expr),
                }
            }
            TokenKind::Minus => {
                // Negative literal pattern
                self.advance();
                match self.peek().clone() {
                    TokenKind::IntLit(v) => {
                        let lit_span = self.advance().span;
                        let span = start.merge(lit_span);
                        let expr = Expr {
                            span,
                            kind: ExprKind::IntLit(-v),
                        };
                        Pattern::Literal {
                            span,
                            expr: Box::new(expr),
                        }
                    }
                    TokenKind::FloatLit(v) => {
                        let lit_span = self.advance().span;
                        let span = start.merge(lit_span);
                        let expr = Expr {
                            span,
                            kind: ExprKind::FloatLit(-v),
                        };
                        Pattern::Literal {
                            span,
                            expr: Box::new(expr),
                        }
                    }
                    _ => {
                        self.error(
                            self.peek_span(),
                            "expected numeric literal after `-` in pattern",
                        );
                        Pattern::Error(start)
                    }
                }
            }

            // Tuple pattern
            TokenKind::LParen => {
                self.advance();
                let mut elements = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.at_eof() {
                    elements.push(self.parse_pattern());
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen);
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Pattern::Tuple { span, elements }
            }

            // Identifier, enum variant path, struct pattern, or binding
            TokenKind::Ident(name) => {
                self.advance();

                // Path: A::B::C(patterns) — enum variant
                if self.check(&TokenKind::ColonColon) {
                    let mut path = vec![name];
                    while self.eat(&TokenKind::ColonColon).is_some() {
                        path.push(self.expect_ident());
                    }
                    if self.check(&TokenKind::LParen) {
                        // Enum tuple variant pattern
                        self.advance();
                        let mut fields = Vec::new();
                        while !self.check(&TokenKind::RParen) && !self.at_eof() {
                            fields.push(self.parse_pattern());
                            if self.eat(&TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                        self.expect(&TokenKind::RParen);
                        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                        Pattern::EnumVariant { span, path, fields }
                    } else {
                        // Unit enum variant
                        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                        Pattern::EnumVariant {
                            span,
                            path,
                            fields: Vec::new(),
                        }
                    }
                }
                // Struct pattern: Name { field: pat, ... }
                else if self.check(&TokenKind::LBrace) {
                    self.advance();
                    let mut fields = Vec::new();
                    let mut rest = false;
                    while !self.check(&TokenKind::RBrace) && !self.at_eof() {
                        if self.check(&TokenKind::DotDot) {
                            self.advance();
                            rest = true;
                            break;
                        }
                        let fname = self.expect_ident();
                        let pat = if self.eat(&TokenKind::Colon).is_some() {
                            self.parse_pattern()
                        } else {
                            // Shorthand: Name { x } == Name { x: x }
                            Pattern::Ident {
                                span: self.tokens[self.pos.saturating_sub(1)].span,
                                name: fname.clone(),
                                mutable: false,
                            }
                        };
                        fields.push((fname, pat));
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RBrace);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::Struct {
                        span,
                        name,
                        fields,
                        rest,
                    }
                }
                // Binding: name @ pattern
                else if self.check(&TokenKind::At) {
                    self.advance();
                    let subpattern = self.parse_pattern();
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::Binding {
                        span,
                        name,
                        subpattern: Box::new(subpattern),
                    }
                }
                // Enum variant with tuple destructuring (no path separator)
                else if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        fields.push(self.parse_pattern());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::EnumVariant {
                        span,
                        path: vec![name],
                        fields,
                    }
                } else {
                    // Check for `mut name` pattern — we already consumed name, but
                    // if it was preceded by `mut` we need to handle that at the call site.
                    Pattern::Ident {
                        span: start,
                        name,
                        mutable: false,
                    }
                }
            }

            // Mutable binding: mut name
            TokenKind::Mut => {
                self.advance();
                let name = self.expect_ident();
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Pattern::Ident {
                    span,
                    name,
                    mutable: true,
                }
            }

            // KwNone, KwSome, KwOk, KwErr as patterns
            TokenKind::KwNone => {
                self.advance();
                Pattern::EnumVariant {
                    span: start,
                    path: vec![SmolStr::new("None")],
                    fields: Vec::new(),
                }
            }
            TokenKind::KwSome => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        fields.push(self.parse_pattern());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::EnumVariant {
                        span,
                        path: vec![SmolStr::new("Some")],
                        fields,
                    }
                } else {
                    Pattern::EnumVariant {
                        span: start,
                        path: vec![SmolStr::new("Some")],
                        fields: Vec::new(),
                    }
                }
            }
            TokenKind::KwOk => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        fields.push(self.parse_pattern());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::EnumVariant {
                        span,
                        path: vec![SmolStr::new("Ok")],
                        fields,
                    }
                } else {
                    Pattern::EnumVariant {
                        span: start,
                        path: vec![SmolStr::new("Ok")],
                        fields: Vec::new(),
                    }
                }
            }
            TokenKind::KwErr => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        fields.push(self.parse_pattern());
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    Pattern::EnumVariant {
                        span,
                        path: vec![SmolStr::new("Err")],
                        fields,
                    }
                } else {
                    Pattern::EnumVariant {
                        span: start,
                        path: vec![SmolStr::new("Err")],
                        fields: Vec::new(),
                    }
                }
            }

            // Range pattern starting with ..
            TokenKind::DotDot | TokenKind::DotDotEq => {
                let inclusive = self.check(&TokenKind::DotDotEq);
                self.advance();
                let end = if !self.check(&TokenKind::FatArrow)
                    && !self.check(&TokenKind::Comma)
                    && !self.check(&TokenKind::RParen)
                    && !self.check(&TokenKind::RBrace)
                    && !self.at_eof()
                {
                    Some(Box::new(self.parse_primary()))
                } else {
                    None
                };
                let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
                Pattern::Range {
                    span,
                    start: None,
                    end,
                    inclusive,
                }
            }

            _ => {
                self.error(start, format!("expected pattern, found `{}`", self.peek()));
                self.advance();
                Pattern::Error(start)
            }
        };

        // Check for range pattern: pat .. pat or pat ..= pat
        if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEq) {
            let inclusive = self.check(&TokenKind::DotDotEq);
            self.advance();
            let start_expr = self.pattern_to_expr(&pat);
            let end = if !self.check(&TokenKind::FatArrow)
                && !self.check(&TokenKind::Comma)
                && !self.check(&TokenKind::RParen)
                && !self.check(&TokenKind::RBrace)
                && !self.at_eof()
            {
                Some(Box::new(self.parse_primary()))
            } else {
                None
            };
            let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
            pat = Pattern::Range {
                span,
                start: start_expr.map(Box::new),
                end,
                inclusive,
            };
        }

        pat
    }

    /// Try to convert a simple pattern back to an expression for range patterns.
    fn pattern_to_expr(&self, pat: &Pattern) -> Option<Expr> {
        match pat {
            Pattern::Literal { expr, .. } => Some((**expr).clone()),
            Pattern::Ident { span, name, .. } => Some(Expr {
                span: *span,
                kind: ExprKind::Ident(name.clone()),
            }),
            _ => None,
        }
    }
}

// ── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span::dummy(),
        }
    }

    fn make_tokens(kinds: Vec<TokenKind>) -> Vec<Token> {
        let mut tokens: Vec<_> = kinds.into_iter().map(make_token).collect();
        tokens.push(make_token(TokenKind::Eof));
        tokens
    }

    #[test]
    fn parse_empty_program() {
        let tokens = make_tokens(vec![]);
        let mut parser = Parser::new(&tokens);
        let (prog, diags) = parser.parse_program();
        assert!(prog.items.is_empty());
        assert!(diags.is_empty());
    }

    #[test]
    fn parse_simple_fn() {
        let tokens = make_tokens(vec![
            TokenKind::Fn,
            TokenKind::Ident(SmolStr::new("main")),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        let mut parser = Parser::new(&tokens);
        let (prog, diags) = parser.parse_program();
        assert_eq!(prog.items.len(), 1);
        assert!(diags.is_empty());
        match &prog.items[0] {
            Item::FnDecl(f) => {
                assert_eq!(f.name.as_str(), "main");
                assert!(f.params.is_empty());
                assert!(f.return_type.is_none());
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn parse_const_decl() {
        let tokens = make_tokens(vec![
            TokenKind::Const,
            TokenKind::Ident(SmolStr::new("X")),
            TokenKind::Eq,
            TokenKind::IntLit(42),
            TokenKind::Semicolon,
        ]);
        let mut parser = Parser::new(&tokens);
        let (prog, diags) = parser.parse_program();
        assert_eq!(prog.items.len(), 1);
        assert!(diags.is_empty());
    }

    #[test]
    fn error_recovery() {
        let tokens = make_tokens(vec![
            TokenKind::Star, // unexpected
            TokenKind::Fn,
            TokenKind::Ident(SmolStr::new("foo")),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        let mut parser = Parser::new(&tokens);
        let (prog, diags) = parser.parse_program();
        // Should recover and find the fn
        assert!(!prog.items.is_empty());
        assert!(!diags.is_empty());
    }
}
