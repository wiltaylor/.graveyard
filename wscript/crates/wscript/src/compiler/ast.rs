//! Abstract syntax tree types for Wscript.

use smol_str::SmolStr;

use super::token::Span;

// ---------------------------------------------------------------------------
// Program (root node)
// ---------------------------------------------------------------------------

/// A complete Wscript program.
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Items (top-level declarations)
// ---------------------------------------------------------------------------

/// A top-level item declaration.
#[derive(Debug, Clone)]
pub enum Item {
    FnDecl(FnDecl),
    StructDecl(StructDecl),
    EnumDecl(EnumDecl),
    TraitDecl(TraitDecl),
    ImplBlock(ImplBlock),
    ConstDecl(ConstDecl),
    GlobalDecl(GlobalDecl),
    Error(Span),
}

/// A top-level `let` or `let mut` declaration — produces a WASM global.
#[derive(Debug, Clone)]
pub struct GlobalDecl {
    pub span: Span,
    pub name: SmolStr,
    pub mutable: bool,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

// ---------------------------------------------------------------------------
// Function declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub generic_params: Option<GenericParams>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub kind: ParamKind,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ParamKind {
    SelfRef {
        mutable: bool,
    },
    Named {
        name: SmolStr,
        ty: TypeExpr,
        default: Option<Expr>,
    },
}

// ---------------------------------------------------------------------------
// Struct declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub generic_params: Option<GenericParams>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub span: Span,
    pub name: SmolStr,
    pub ty: TypeExpr,
}

// ---------------------------------------------------------------------------
// Enum declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub generic_params: Option<GenericParams>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub kind: VariantKind,
}

#[derive(Debug, Clone)]
pub enum VariantKind {
    Unit,
    Tuple(Vec<TypeExpr>),
    Struct(Vec<StructField>),
}

// ---------------------------------------------------------------------------
// Trait declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub items: Vec<TraitItem>,
}

#[derive(Debug, Clone)]
pub enum TraitItem {
    FnDecl(FnDecl),
    FnSig(FnSig),
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub span: Span,
    pub attrs: Vec<Attribute>,
    pub name: SmolStr,
    pub generic_params: Option<GenericParams>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
}

// ---------------------------------------------------------------------------
// Impl block
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub span: Span,
    pub generic_params: Option<GenericParams>,
    pub self_type: TypeExpr,
    pub trait_type: Option<TypeExpr>,
    pub methods: Vec<FnDecl>,
}

// ---------------------------------------------------------------------------
// Const declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub span: Span,
    pub name: SmolStr,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

// ---------------------------------------------------------------------------
// Type expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TypeExpr {
    pub span: Span,
    pub kind: TypeExprKind,
}

#[derive(Debug, Clone)]
pub enum TypeExprKind {
    Primitive(PrimitiveType),
    StringType,
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    OptionType(Box<TypeExpr>),
    ResultType(Box<TypeExpr>, Option<Box<TypeExpr>>),
    FnType {
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
    },
    RefType {
        inner: Box<TypeExpr>,
        mutable: bool,
    },
    Tuple(Vec<TypeExpr>),
    Unit,
    Named {
        name: SmolStr,
        args: Option<Vec<TypeExpr>>,
    },
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
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
    F32,
    F64,
    Bool,
    Char,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // ── Literals ──────────────────────────────────────────────────────
    IntLit(i128),
    FloatLit(f64),
    BoolLit(bool),
    CharLit(char),
    StringLit(String),
    TemplateLit(Vec<TemplateSegment>),
    ArrayLit(Vec<Expr>),
    MapLit(Vec<(Expr, Expr)>),
    TupleLit(Vec<Expr>),
    UnitLit,

    // ── Names / paths ────────────────────────────────────────────────
    Ident(SmolStr),
    Path(Vec<SmolStr>),

    // ── Struct init ──────────────────────────────────────────────────
    StructInit {
        name: SmolStr,
        fields: Vec<FieldInit>,
    },

    // ── Operators ────────────────────────────────────────────────────
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },

    // ── Access ───────────────────────────────────────────────────────
    FieldAccess {
        object: Box<Expr>,
        field: SmolStr,
    },
    TupleIndex {
        object: Box<Expr>,
        index: u32,
    },
    MethodCall {
        object: Box<Expr>,
        method: SmolStr,
        args: Vec<CallArg>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    // ── Postfix ──────────────────────────────────────────────────────
    ErrorPropagate(Box<Expr>),
    Cast {
        expr: Box<Expr>,
        ty: TypeExpr,
    },

    // ── Call / pipe ──────────────────────────────────────────────────
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    Pipe {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    // ── Control flow (expression-position) ───────────────────────────
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_block: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    For {
        pattern: Pattern,
        iterable: Box<Expr>,
        body: Block,
    },
    While {
        condition: Box<Expr>,
        body: Block,
    },
    Loop {
        body: Block,
    },
    IfLet {
        pattern: Pattern,
        expr: Box<Expr>,
        then_block: Block,
        else_block: Option<Box<Expr>>,
    },
    WhileLet {
        pattern: Pattern,
        expr: Box<Expr>,
        body: Block,
    },

    // ── Jump expressions ─────────────────────────────────────────────
    Break(Option<Box<Expr>>),
    Continue,
    Return(Option<Box<Expr>>),

    // ── Lambda ───────────────────────────────────────────────────────
    Lambda {
        params: Vec<LambdaParam>,
        return_type: Option<Box<TypeExpr>>,
        body: Box<Expr>,
    },

    // ── Range ────────────────────────────────────────────────────────
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },

    // ── Block ────────────────────────────────────────────────────────
    Block(Block),

    // ── Macro call ───────────────────────────────────────────────────
    MacroCall {
        name: SmolStr,
        args: Vec<Expr>,
    },

    // ── Error recovery ───────────────────────────────────────────────
    Error,
}

// ---------------------------------------------------------------------------
// Template segments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TemplateSegment {
    Literal(String),
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// Block and statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Expr(ExprStmt),
    Item(Box<Item>),
    Error(Span),
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub span: Span,
    pub mutable: bool,
    pub pattern: Pattern,
    pub ty: Option<TypeExpr>,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub span: Span,
    pub expr: Expr,
    pub has_semicolon: bool,
}

// ---------------------------------------------------------------------------
// Match arms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Span),
    Ident {
        span: Span,
        name: SmolStr,
        mutable: bool,
    },
    Literal {
        span: Span,
        expr: Box<Expr>,
    },
    Tuple {
        span: Span,
        elements: Vec<Pattern>,
    },
    EnumVariant {
        span: Span,
        path: Vec<SmolStr>,
        fields: Vec<Pattern>,
    },
    Struct {
        span: Span,
        name: SmolStr,
        fields: Vec<(SmolStr, Pattern)>,
        rest: bool,
    },
    Binding {
        span: Span,
        name: SmolStr,
        subpattern: Box<Pattern>,
    },
    Range {
        span: Span,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },
    Error(Span),
}

// ---------------------------------------------------------------------------
// Call arguments, lambda params, field initialisers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<SmolStr>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: SmolStr,
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub struct FieldInit {
    pub name: SmolStr,
    pub value: Option<Expr>,
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Attribute {
    pub span: Span,
    pub name: SmolStr,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone)]
pub enum AttrArg {
    Ident(SmolStr),
    Literal(Expr),
    KeyValue(SmolStr, Expr),
}

// ---------------------------------------------------------------------------
// Generics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GenericParams {
    pub params: Vec<GenericParam>,
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: SmolStr,
    pub bounds: Vec<TraitBound>,
}

#[derive(Debug, Clone)]
pub struct TraitBound {
    pub name: SmolStr,
    pub args: Option<Vec<TypeExpr>>,
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Neq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Spaceship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    Ref,
    RefMut,
    Deref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
}
