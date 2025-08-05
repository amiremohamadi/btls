mod ast;
pub mod semantic_analyzer;
mod tests;

use pest::Span;
use std::iter::FilterMap;

pub trait Node<'a> {
    fn as_node(&self) -> &dyn Node<'a>;
    fn children(&self) -> Vec<&dyn Node<'a>>;
    fn span(&self) -> Span<'a>;

    fn as_error<'b>(&'b self) -> Option<ErrorRef<'a, 'b>> {
        None
    }

    fn errors<'b>(&'b self) -> FilterWalk<'a, 'b, ErrorRef<'a, 'b>> {
        FilterWalk::new(self.as_node(), |node| node.as_error())
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

pub struct FilterWalk<'a, 'b, T> {
    inner: FilterMap<Walk<'a, 'b>, fn(&'b dyn Node<'a>) -> Option<T>>,
}

impl<'a, 'b, T> FilterWalk<'a, 'b, T> {
    pub fn new(node: &'b dyn Node<'a>, filter: fn(&'b dyn Node<'a>) -> Option<T>) -> Self {
        FilterWalk {
            inner: Walk::new(node).filter_map(filter),
        }
    }
}

impl<T> Iterator for FilterWalk<'_, '_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
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
}

impl<'a> ErrorStatement<'a> {
    pub fn diagnosis(&self) -> String {
        match self {
            Self::UnknownStatement(stmt) => stmt.diagnosis(),
        }
    }
}

impl<'a> Node<'a> for ErrorStatement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::UnknownStatement(stmt) => vec![stmt.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::UnknownStatement(stmt) => stmt.span(),
        }
    }

    fn as_error<'b>(&'b self) -> Option<ErrorRef<'a, 'b>> {
        Some(ErrorRef::Statement(self))
    }
}

#[derive(Debug)]
pub struct Identifier<'a> {
    pub name: &'a str,
    pub span: Span<'a>,
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
    pub value: &'a str,
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
    pub value: i64,
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
}

impl<'a> Node<'a> for Lvalue<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Identifier(ident) => vec![ident.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Identifier(ident) => ident.span(),
        }
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

#[derive(Debug)]
pub enum Expr<'a> {
    Identifier(Box<Identifier<'a>>),
    Integer(Box<IntegerLiteral<'a>>),
    String(Box<StringLiteral<'a>>),
    BinaryExpr(Box<BinaryExpr<'a>>),
}

impl<'a> Node<'a> for Expr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Integer(n) => vec![n.as_node()],
            Self::String(s) => vec![s.as_node()],
            Self::Identifier(ident) => vec![ident.as_node()],
            Self::BinaryExpr(expr) => vec![expr.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Integer(n) => n.span(),
            Self::String(s) => s.span(),
            Self::Identifier(ident) => ident.span(),
            Self::BinaryExpr(expr) => expr.span(),
        }
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
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
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
pub enum Statement<'a> {
    Error(Box<ErrorStatement<'a>>),
    Assignment(Box<Assignment<'a>>),
    Call(Box<Call<'a>>),
}

impl<'a> Node<'a> for Statement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Error(e) => vec![e.as_node()],
            Self::Assignment(assign) => vec![assign.as_node()],
            Self::Call(c) => vec![c.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Error(e) => e.span(),
            Self::Assignment(assign) => assign.span(),
            Self::Call(c) => c.span(),
        }
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
}

impl<'a> ErrorPreamble<'a> {
    pub fn diagnosis(&self) -> String {
        match self {
            Self::UnknownPreamble(pream) => pream.diagnosis(),
        }
    }
}

impl<'a> Node<'a> for ErrorPreamble<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::UnknownPreamble(p) => vec![p.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::UnknownPreamble(p) => p.span(),
        }
    }

    fn as_error<'b>(&'b self) -> Option<ErrorRef<'a, 'b>> {
        Some(ErrorRef::Preamble(self))
    }
}

#[derive(Debug)]
pub enum Preamble<'a> {
    Probe(Probe<'a>),
    Error(Box<ErrorPreamble<'a>>),
}

impl<'a> Node<'a> for Preamble<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Probe(p) => p.children(),
            Self::Error(e) => vec![e.as_node()],
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Probe(p) => p.span(),
            Self::Error(e) => e.span(),
        }
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

#[derive(Clone, Copy, Debug)]
pub enum ErrorRef<'a, 'b> {
    Statement(&'b ErrorStatement<'a>),
    Preamble(&'b ErrorPreamble<'a>),
}

impl<'a, 'b> ErrorRef<'a, 'b> {
    pub fn diagnosis(&self) -> String {
        match self {
            Self::Statement(stmt) => stmt.diagnosis(),
            Self::Preamble(pream) => pream.diagnosis(),
        }
    }
}

impl<'a, 'b> Node<'a> for ErrorRef<'a, 'b> {
    fn as_node(&self) -> &'b dyn Node<'a> {
        match self {
            Self::Statement(stmt) => stmt.as_node(),
            Self::Preamble(pream) => pream.as_node(),
        }
    }

    fn children(&self) -> Vec<&'b dyn Node<'a>> {
        match self {
            Self::Statement(stmt) => stmt.children(),
            Self::Preamble(pream) => pream.children(),
        }
    }

    fn span(&self) -> Span<'a> {
        match self {
            Self::Statement(stmt) => stmt.span(),
            Self::Preamble(pream) => pream.span(),
        }
    }
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
