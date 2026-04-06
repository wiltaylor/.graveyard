use smol_str::SmolStr;

use super::token::Span;

// ---------------------------------------------------------------------------
// Module (top-level IR container)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrModule {
    pub functions: Vec<IrFunction>,
    pub struct_layouts: Vec<StructLayout>,
    pub enum_layouts: Vec<EnumLayout>,
    pub string_constants: Vec<String>,
    pub globals: Vec<IrGlobal>,
    /// User-registered host function imports to emit in the WASM module.
    /// Resolved in the runtime linker under module name "host".
    pub user_host_imports: Vec<IrHostImport>,
}

#[derive(Debug, Clone)]
pub struct IrHostImport {
    pub name: SmolStr,
    pub params: Vec<IrType>,
    pub ret: IrType,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: SmolStr,
    pub params: Vec<IrParam>,
    pub ret_type: IrType,
    pub locals: Vec<IrLocal>,
    pub body: Vec<IrStmt>,
    pub is_export: bool,
    pub source_span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: SmolStr,
    pub ty: IrType,
}

#[derive(Debug, Clone)]
pub struct IrLocal {
    pub name: SmolStr,
    pub ty: IrType,
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct IrGlobal {
    pub name: SmolStr,
    pub ty: IrType,
    pub init: Option<IrExpr>,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct IrFnSig {
    pub name: SmolStr,
    pub params: Vec<IrType>,
    pub ret: IrType,
}

// ---------------------------------------------------------------------------
// Types (WASM-level)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrType {
    I32,
    I64,
    F32,
    F64,
    Bool,
    Ptr,
    Unit,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IrStmt {
    Let {
        local: u32,
        ty: IrType,
        value: Option<IrExpr>,
    },
    Assign {
        target: IrLValue,
        value: IrExpr,
    },
    Expr(IrExpr),
    Return(Option<IrExpr>),
    If {
        condition: IrExpr,
        then_body: Vec<IrStmt>,
        else_body: Vec<IrStmt>,
    },
    Loop {
        body: Vec<IrStmt>,
    },
    Break,
    Continue,
    DebugProbe {
        span: Span,
        locals: Vec<u32>,
    },
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IrExpr {
    IntConst(i64),
    FloatConst(f64),
    BoolConst(bool),

    LocalGet(u32),
    LocalSet(u32, Box<IrExpr>),

    BinOp {
        op: IrBinOp,
        lhs: Box<IrExpr>,
        rhs: Box<IrExpr>,
    },
    UnaryOp {
        op: IrUnaryOp,
        operand: Box<IrExpr>,
    },

    // Heap / reference-counting
    HeapAlloc {
        size: u32,
    },
    HeapLoad {
        addr: Box<IrExpr>,
        offset: u32,
        ty: IrType,
    },
    HeapStore {
        addr: Box<IrExpr>,
        offset: u32,
        value: Box<IrExpr>,
        ty: IrType,
    },
    RcIncr(Box<IrExpr>),
    RcDecr(Box<IrExpr>),

    // Calls
    Call {
        func: SmolStr,
        args: Vec<IrExpr>,
    },
    CallIndirect {
        table_index: u32,
        func_index: Box<IrExpr>,
        args: Vec<IrExpr>,
        sig: IrFnSig,
    },
    HostCall {
        module: SmolStr,
        name: SmolStr,
        args: Vec<IrExpr>,
        ret: IrType,
    },

    // Struct / enum operations
    StructNew {
        layout_index: u32,
        fields: Vec<IrExpr>,
    },
    FieldGet {
        object: Box<IrExpr>,
        layout_index: u32,
        field_index: u32,
    },
    FieldSet {
        object: Box<IrExpr>,
        layout_index: u32,
        field_index: u32,
        value: Box<IrExpr>,
    },
    EnumTag(Box<IrExpr>),
    EnumPayload {
        object: Box<IrExpr>,
        variant_index: u32,
    },

    // String constants
    StringConst(u32),

    /// Read a user-declared top-level global (produced by a top-level `let`).
    GlobalGet(SmolStr),

    // Cast between IR types
    Cast {
        expr: Box<IrExpr>,
        from: IrType,
        to: IrType,
    },

    // Control flow expressions
    IfExpr {
        condition: Box<IrExpr>,
        then_expr: Box<IrExpr>,
        else_expr: Box<IrExpr>,
    },
    Block {
        stmts: Vec<IrStmt>,
        result: Box<IrExpr>,
    },
    Seq(Vec<IrExpr>),
}

// ---------------------------------------------------------------------------
// L-values (assignment targets)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IrLValue {
    Local(u32),
    Global(SmolStr),
    Field {
        object: Box<IrExpr>,
        layout_index: u32,
        field_index: u32,
    },
    HeapAddr {
        addr: Box<IrExpr>,
        offset: u32,
    },
}

// ---------------------------------------------------------------------------
// Binary operators (WASM-level, typed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrBinOp {
    // Integer add
    AddI32,
    AddI64,
    // Float add
    AddF32,
    AddF64,
    // Integer sub
    SubI32,
    SubI64,
    // Float sub
    SubF32,
    SubF64,
    // Integer mul
    MulI32,
    MulI64,
    // Float mul
    MulF32,
    MulF64,
    // Signed integer div
    DivI32S,
    DivI64S,
    // Unsigned integer div
    DivI32U,
    DivI64U,
    // Float div
    DivF32,
    DivF64,
    // Signed integer rem
    RemI32S,
    RemI64S,
    // Unsigned integer rem
    RemI32U,
    RemI64U,
    // Bitwise
    AndI32,
    AndI64,
    OrI32,
    OrI64,
    XorI32,
    XorI64,
    ShlI32,
    ShlI64,
    ShrI32S,
    ShrI64S,
    ShrI32U,
    ShrI64U,
    // Comparison (integer, signed)
    EqI32,
    EqI64,
    NeI32,
    NeI64,
    LtI32S,
    LtI64S,
    LtI32U,
    LtI64U,
    GtI32S,
    GtI64S,
    GtI32U,
    GtI64U,
    LeI32S,
    LeI64S,
    LeI32U,
    LeI64U,
    GeI32S,
    GeI64S,
    GeI32U,
    GeI64U,
    // Comparison (float)
    EqF32,
    EqF64,
    NeF32,
    NeF64,
    LtF32,
    LtF64,
    GtF32,
    GtF64,
    LeF32,
    LeF64,
    GeF32,
    GeF64,
}

// ---------------------------------------------------------------------------
// Unary operators (WASM-level)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrUnaryOp {
    NegI32,
    NegI64,
    NegF32,
    NegF64,
    NotI32,
    EqzI32,
    EqzI64,
    // Wrap / extend / convert
    WrapI64ToI32,
    ExtendI32SToI64,
    ExtendI32UToI64,
    ConvertI32SToF32,
    ConvertI32SToF64,
    ConvertI64SToF32,
    ConvertI64SToF64,
    ConvertI32UToF32,
    ConvertI32UToF64,
    ConvertI64UToF32,
    ConvertI64UToF64,
    TruncF32ToI32S,
    TruncF32ToI64S,
    TruncF64ToI32S,
    TruncF64ToI64S,
    PromoteF32ToF64,
    DemoteF64ToF32,
}

// ---------------------------------------------------------------------------
// Struct layouts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StructLayout {
    pub name: SmolStr,
    pub fields: Vec<StructFieldLayout>,
    pub size: u32,
    pub field_offsets: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct StructFieldLayout {
    pub name: SmolStr,
    pub ty: IrType,
    pub offset: u32,
    /// If this field is a named user type (struct or string), the type name.
    /// Used by reflection to resolve nested struct fields and string fields.
    pub type_name: Option<SmolStr>,
}

// ---------------------------------------------------------------------------
// Enum layouts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EnumLayout {
    pub name: SmolStr,
    pub variants: Vec<EnumVariantLayout>,
    pub tag_size: u32,
    pub max_payload_size: u32,
}

#[derive(Debug, Clone)]
pub struct EnumVariantLayout {
    pub name: SmolStr,
    pub tag: u32,
    pub payload_types: Vec<IrType>,
    pub payload_size: u32,
}
