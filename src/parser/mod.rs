pub mod ast;
mod tests;

use pest::Span;
use std::fmt;

pub trait Node<'a> {
    fn as_node(&self) -> &dyn Node<'a>;
    fn children(&self) -> Vec<&dyn Node<'a>>;
    fn span(&self) -> Span<'a>;

    fn as_statement(&self) -> Option<&Statement<'a>> {
        None
    }

    fn as_expr(&self) -> Option<&Expr<'a>> {
        None
    }

    fn as_config(&self) -> Option<&Config<'a>> {
        None
    }
}

pub struct Walk<'a, 'b> {
    stack: Vec<&'b dyn Node<'a>>,
}

impl<'a, 'b> Walk<'a, 'b> {
    pub fn new(node: &'b dyn Node<'a>) -> Self {
        Walk { stack: vec![node] }
    }
}

impl<'a, 'b> Iterator for Walk<'a, 'b> {
    type Item = &'b dyn Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack.extend(node.children().into_iter().rev());
        Some(node)
    }
}

#[derive(Debug)]
pub struct UndefinedFunc<'a> {
    pub text: &'a str,
    pub span: Span<'a>,
}

impl<'a> UndefinedFunc<'a> {
    pub fn new(text: &'a str, span: Span<'a>) -> Statement<'a> {
        Statement::Error(Box::new(ErrorStatement::UndefinedFunc(Box::new(Self {
            text,
            span,
        }))))
    }

    pub fn diagnosis(&self) -> String {
        format!("Undefined function \"{}\"", self.text.trim())
    }
}

impl<'a> Node<'a> for UndefinedFunc<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct UndefinedIdent<'a> {
    pub text: &'a str,
    pub span: Span<'a>,
}

impl<'a> UndefinedIdent<'a> {
    pub fn new(text: &'a str, span: Span<'a>) -> Statement<'a> {
        Statement::Error(Box::new(ErrorStatement::UndefinedIdent(Box::new(Self {
            text,
            span,
        }))))
    }

    pub fn diagnosis(&self) -> String {
        format!("Undefined Identifier \"{}\"", self.text.trim())
    }
}

impl<'a> Node<'a> for UndefinedIdent<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct UnknownStatement<'a> {
    pub text: &'a str,
    pub span: Span<'a>,
}

impl<'a> UnknownStatement<'a> {
    pub fn diagnosis(&self) -> String {
        format!("Unknown statement \"{}\"", self.text.trim())
    }
}

impl<'a> Node<'a> for UnknownStatement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum ErrorStatement<'a> {
    UnknownStatement(Box<UnknownStatement<'a>>),
    UndefinedIdent(Box<UndefinedIdent<'a>>),
    UndefinedFunc(Box<UndefinedFunc<'a>>),
}

impl<'a> ErrorStatement<'a> {
    pub fn diagnosis(&self) -> String {
        match self {
            Self::UnknownStatement(e) => e.diagnosis(),
            Self::UndefinedIdent(e) => e.diagnosis(),
            Self::UndefinedFunc(e) => e.diagnosis(),
        }
    }
}

impl<'a> Node<'a> for ErrorStatement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::UnknownStatement(e) => vec![e.as_node()],
            Self::UndefinedIdent(e) => vec![e.as_node()],
            Self::UndefinedFunc(e) => vec![e.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::UnknownStatement(e) => e.span(),
            Self::UndefinedIdent(e) => e.span(),
            Self::UndefinedFunc(e) => e.span(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IdentKind {
    Bare,
    Scratch,
    Map,
}

impl IdentKind {
    pub fn prefix(self) -> &'static str {
        match self {
            IdentKind::Scratch => "$",
            IdentKind::Map => "@",
            IdentKind::Bare => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroParamKind {
    Expr,
    Scratch,
    Map,
}

#[derive(Debug)]
pub struct Identifier<'a> {
    pub name: &'a str,
    pub span: Span<'a>,
    pub kind: IdentKind,
}

impl<'a> Identifier<'a> {
    pub fn prefixed_name(&self) -> String {
        format!("{}{}", self.kind.prefix(), self.name)
    }
}

impl<'a> Node<'a> for Identifier<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct StringLiteral<'a> {
    pub span: Span<'a>,
}

impl<'a> Node<'a> for StringLiteral<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct IntegerLiteral<'a> {
    pub span: Span<'a>,
}

impl<'a> Node<'a> for IntegerLiteral<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum Lvalue<'a> {
    Identifier(Box<Identifier<'a>>),
    MapAccess(Box<MapAccess<'a>>),
}

impl<'a> Node<'a> for Lvalue<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Identifier(ident) => vec![ident.as_node()],
            Self::MapAccess(access) => access.children(),
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Identifier(ident) => ident.span(),
            Self::MapAccess(access) => access.span,
        }
    }
}

#[derive(Debug)]
pub struct MapAccess<'a> {
    pub map: Identifier<'a>,
    pub keys: Vec<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for MapAccess<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node> = vec![&self.map];
        children.extend(self.keys.iter().map(|k| k.as_node()));
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct BinaryExpr<'a> {
    pub lhs: Box<Expr<'a>>,
    pub rhs: Box<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for BinaryExpr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.lhs, &*self.rhs]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
    Inc,
    Dec,
    Deref,
}

#[derive(Debug)]
pub struct UnaryExpr<'a> {
    pub op: UnaryOp,
    pub expr: Box<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for UnaryExpr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.expr]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum Expr<'a> {
    Identifier(Box<Identifier<'a>>),
    Integer(Box<IntegerLiteral<'a>>),
    String(Box<StringLiteral<'a>>),
    Call(Box<Call<'a>>),
    BinaryExpr(Box<BinaryExpr<'a>>),
    UnaryExpr(Box<UnaryExpr<'a>>),
    MapAccess(Box<MapAccess<'a>>),
    Cast(Box<CastExpr<'a>>),
    ArgN(Box<ArgNExpr<'a>>),
    Field(Box<FieldAccess<'a>>),
    Tuple(Box<Tuple<'a>>),
}

impl<'a> Node<'a> for Expr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn as_expr(&self) -> Option<&Expr<'a>> {
        Some(self)
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Integer(n) => vec![n.as_node()],
            Self::String(s) => vec![s.as_node()],
            Self::Identifier(ident) => vec![ident.as_node()],
            Self::Call(func) => vec![func.as_node()],
            Self::BinaryExpr(expr) => vec![expr.as_node()],
            Self::UnaryExpr(expr) => vec![expr.as_node()],
            Self::MapAccess(access) => access.children(),
            Self::Cast(cast) => cast.children(),
            Self::ArgN(arg) => vec![arg.as_node()],
            Self::Field(field) => field.children(),
            Self::Tuple(tuple) => tuple.children(),
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Integer(n) => n.span(),
            Self::String(s) => s.span(),
            Self::Identifier(ident) => ident.span(),
            Self::Call(func) => func.span(),
            Self::BinaryExpr(expr) => expr.span(),
            Self::UnaryExpr(expr) => expr.span(),
            Self::MapAccess(access) => access.span,
            Self::Cast(cast) => cast.span(),
            Self::ArgN(arg) => arg.span(),
            Self::Field(field) => field.span(),
            Self::Tuple(tuple) => tuple.span(),
        }
    }
}

#[derive(Debug)]
pub struct CastExpr<'a> {
    pub type_name: TypeName<'a>,
    pub expr: Box<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for CastExpr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.expr]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Builtin,
    Struct,
    Union,
    Invalid,
}

#[derive(Debug)]
pub struct TypeName<'a> {
    pub kind: TypeKind,
    pub name: &'a str,
    pub pointers: usize,
    pub span: Span<'a>,
}

impl<'a> TypeName<'a> {
    pub fn text(&self) -> &'a str {
        self.span.as_str().trim()
    }

    pub fn validate<T>(&self, structs: &std::collections::HashMap<String, T>) -> bool {
        match self.kind {
            TypeKind::Builtin => crate::builtins::DATA_TYPES
                .iter()
                .any(|ty| ty.name == self.name),
            TypeKind::Struct | TypeKind::Union => structs.contains_key(self.name),
            TypeKind::Invalid => false,
        }
    }
}

#[derive(Debug)]
pub enum FieldMember<'a> {
    Name(Identifier<'a>),
    Index(u64, Span<'a>),
}

impl<'a> FieldMember<'a> {
    pub fn span(&self) -> Span<'a> {
        match self {
            Self::Name(ident) => ident.span,
            Self::Index(_, span) => *span,
        }
    }
}

#[derive(Debug)]
pub struct FieldAccess<'a> {
    pub base: Box<Expr<'a>>,
    pub field: Option<FieldMember<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for FieldAccess<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node<'a>> = vec![&*self.base];
        if let Some(FieldMember::Name(ident)) = &self.field {
            children.push(ident.as_node());
        }
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Tuple<'a> {
    pub elements: Vec<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Tuple<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.elements.iter().map(|e| e.as_node()).collect()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct ArgNExpr<'a> {
    pub span: Span<'a>,
}

impl<'a> Node<'a> for ArgNExpr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Call<'a> {
    pub func: Identifier<'a>,
    pub args: Vec<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Call<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node> = vec![&self.func];
        children.extend(self.args.iter().map(|x| x.as_node()));
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum MacroParam<'a> {
    Expr {
        name: Identifier<'a>,
        span: Span<'a>,
    },
    Scratch {
        name: Identifier<'a>,
        span: Span<'a>,
    },
    Map {
        name: Identifier<'a>,
        keyed: bool,
        span: Span<'a>,
    },
}

impl<'a> MacroParam<'a> {
    pub fn name(&self) -> &Identifier<'a> {
        match self {
            MacroParam::Expr { name, .. }
            | MacroParam::Scratch { name, .. }
            | MacroParam::Map { name, .. } => name,
        }
    }

    pub fn kind(&self) -> MacroParamKind {
        match self {
            MacroParam::Expr { .. } => MacroParamKind::Expr,
            MacroParam::Scratch { .. } => MacroParamKind::Scratch,
            MacroParam::Map { .. } => MacroParamKind::Map,
        }
    }
}

impl<'a> Node<'a> for MacroParam<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![self.name().as_node()]
    }

    fn span(&self) -> Span<'a> {
        match self {
            MacroParam::Expr { span, .. }
            | MacroParam::Scratch { span, .. }
            | MacroParam::Map { span, .. } => *span,
        }
    }
}

#[derive(Debug)]
pub struct MacroDefinition<'a> {
    pub name: Identifier<'a>,
    pub params: Vec<MacroParam<'a>>,
    pub body: Block<'a>,
    pub comments: Vec<&'a str>,
    pub span: Span<'a>,
}

impl fmt::Display for MacroDefinition<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = self
            .params
            .iter()
            .map(|param| param.name().prefixed_name())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "macro {}({})", self.name.name, params)
    }
}

impl<'a> Node<'a> for MacroDefinition<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node<'a>> = vec![self.name.as_node()];
        children.extend(self.params.iter().map(|param| param.as_node()));
        children.push(self.body.as_node());
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
}

#[derive(Debug)]
pub struct Assignment<'a> {
    pub lvalue: Lvalue<'a>,
    pub rvalue: Box<Expr<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Assignment<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&self.lvalue, &*self.rvalue]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum Loop<'a> {
    While(Box<While<'a>>),
    For(Box<For<'a>>),
    Unroll(Box<Unroll<'a>>),
}

impl<'a> Node<'a> for Loop<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::While(w) => w.children(),
            Self::For(f) => f.children(),
            Self::Unroll(u) => u.children(),
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::While(w) => w.span(),
            Self::For(f) => f.span(),
            Self::Unroll(u) => u.span(),
        }
    }
}

#[derive(Debug)]
pub struct While<'a> {
    pub condition: Box<Expr<'a>>,
    pub block: Block<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for While<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.condition, &self.block]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct For<'a> {
    pub lhs: Box<Expr<'a>>,
    pub rhs: Box<Expr<'a>>,
    pub block: Block<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for For<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.lhs, &*self.rhs, &self.block]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Unroll<'a> {
    pub count: Box<IntegerLiteral<'a>>,
    pub block: Block<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Unroll<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![&*self.count, &self.block]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct If<'a> {
    pub condition: Box<Expr<'a>>,
    pub block: Block<'a>,
    pub else_branch: Option<Box<Else<'a>>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for If<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node<'a>> = vec![&*self.condition, &self.block];
        if let Some(else_branch) = &self.else_branch {
            children.push(else_branch.as_node());
        }
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum Else<'a> {
    IfCond(Box<If<'a>>),
    Block(Block<'a>),
}

impl<'a> Node<'a> for Else<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::IfCond(c) => vec![c.as_node()],
            Self::Block(block) => vec![block],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::IfCond(c) => c.span(),
            Self::Block(block) => block.span(),
        }
    }
}

#[derive(Debug)]
pub enum Statement<'a> {
    Error(Box<ErrorStatement<'a>>),
    Assignment(Box<Assignment<'a>>, bool),
    IfCond(Box<If<'a>>),
    Loop(Box<Loop<'a>>),
    Expr(Box<Expr<'a>>, bool),
}

impl<'a> Node<'a> for Statement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn as_statement(&self) -> Option<&Statement<'a>> {
        Some(self)
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Error(e) => vec![e.as_node()],
            Self::Assignment(assign, _) => vec![assign.as_node()],
            Self::IfCond(c) => vec![c.as_node()],
            Self::Loop(c) => vec![c.as_node()],
            Self::Expr(e, _) => vec![e.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Error(e) => e.span(),
            Self::Assignment(assign, _) => assign.span(),
            Self::IfCond(c) => c.span(),
            Self::Loop(c) => c.span(),
            Self::Expr(e, _) => e.span(),
        }
    }
}

#[derive(Debug)]
pub struct UnmatchedBrace<'a> {
    pub span: Span<'a>,
}

impl<'a> UnmatchedBrace<'a> {
    pub fn diagnosis(&self) -> String {
        "Unmatched brace".to_string()
    }
}

impl<'a> Node<'a> for UnmatchedBrace<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct UnknownPreamble<'a> {
    pub text: &'a str,
    pub span: Span<'a>,
}

impl<'a> UnknownPreamble<'a> {
    pub fn diagnosis(&self) -> String {
        format!("Unknown preamble \"{}\"", self.text.trim())
    }
}

impl<'a> Node<'a> for UnknownPreamble<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum ErrorPreamble<'a> {
    UnknownPreamble(Box<UnknownPreamble<'a>>),
    UnmatchedBrace(Box<UnmatchedBrace<'a>>),
}

impl<'a> ErrorPreamble<'a> {
    pub fn diagnosis(&self) -> String {
        match self {
            Self::UnknownPreamble(e) => e.diagnosis(),
            Self::UnmatchedBrace(e) => e.diagnosis(),
        }
    }
}

impl<'a> Node<'a> for ErrorPreamble<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::UnknownPreamble(x) => vec![x.as_node()],
            Self::UnmatchedBrace(x) => vec![x.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::UnknownPreamble(x) => x.span(),
            Self::UnmatchedBrace(x) => x.span(),
        }
    }
}

#[derive(Debug)]
pub struct ConfigAssignment<'a> {
    pub key: &'a str,
    pub value: Option<&'a str>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for ConfigAssignment<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Config<'a> {
    pub assignments: Vec<ConfigAssignment<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Config<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn as_config(&self) -> Option<&Config<'a>> {
        Some(self)
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.assignments.iter().map(|a| a.as_node()).collect()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub enum Preamble<'a> {
    Probe(Probe<'a>),
    CDef(Box<CDef<'a>>),
    Macro(Box<MacroDefinition<'a>>),
    Config(Box<Config<'a>>),
    Error(Box<ErrorPreamble<'a>>),
}

impl<'a> Node<'a> for Preamble<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn as_config(&self) -> Option<&Config<'a>> {
        match self {
            Self::Config(c) => Some(c.as_ref()),
            _ => None,
        }
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Probe(p) => p.children(),
            Self::CDef(c) => vec![c.as_node()],
            Self::Macro(m) => vec![m.as_node()],
            Self::Config(c) => vec![c.as_node()],
            Self::Error(e) => vec![e.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Probe(p) => p.span(),
            Self::CDef(c) => c.span(),
            Self::Macro(m) => m.span(),
            Self::Config(c) => c.span(),
            Self::Error(e) => e.span(),
        }
    }
}

#[derive(Debug)]
pub enum CDef<'a> {
    Include(Box<Include<'a>>),
    Define(Box<Define<'a>>),
    Struct(Box<StructDef<'a>>),
}

impl<'a> Node<'a> for CDef<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Include(i) => vec![i.as_node()],
            Self::Define(d) => vec![d.as_node()],
            Self::Struct(s) => vec![s.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Include(i) => i.span(),
            Self::Define(d) => d.span(),
            Self::Struct(s) => s.span(),
        }
    }
}

#[derive(Debug)]
pub struct StructDef<'a> {
    pub name: Identifier<'a>,
    pub fields: Vec<FieldDecl<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for StructDef<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        let mut children: Vec<&dyn Node<'a>> = vec![self.name.as_node()];
        children.extend(self.fields.iter().map(|f| f.as_node()));
        children
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct FieldDecl<'a> {
    pub name: Identifier<'a>,
    pub type_name: TypeName<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for FieldDecl<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![self.name.as_node()]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Include<'a> {
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Include<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Define<'a> {
    pub name: Identifier<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Define<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        vec![self.name.as_node()]
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Probe<'a> {
    pub attach_points: Vec<&'a str>,
    pub condition: Option<Expr<'a>>,
    pub block: Block<'a>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Probe<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.block.children()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Program<'a> {
    pub preambles: Vec<Preamble<'a>>,
    // pub probes: Vec<Probe<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Program<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.preambles.iter().map(|p| p.as_node()).collect()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}

#[derive(Debug)]
pub struct Block<'a> {
    pub statements: Vec<Statement<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Block<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.statements.iter().map(|s| s.as_node()).collect()
    }

    fn span(&self) -> Span<'a> {
        self.span
    }
}
