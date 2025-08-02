mod parser;
mod tests;

use pest::Span;
use std::iter::FilterMap;

pub use parser::*;

pub static ATTACH_POINTS: &[&str] = &[
    "BEGIN",
    "END",
    "tracepoint:",
    "kprobe",
    "kfunc",
    "kretfunc",
    "kretprobe",
    "uprobe",
    "uretprobe",
    "iter",
    "hardware",
    "software:",
    "rawtracepoint",
];

pub trait Node<'a> {
    fn as_node(&self) -> &dyn Node<'a>;
    fn children(&self) -> Vec<&dyn Node<'a>>;

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

impl<'a> Node<'a> for UnknownStatement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        Vec::new()
    }
}

#[derive(Debug)]
pub enum ErrorStatement<'a> {
    UnknownStatement(Box<UnknownStatement<'a>>),
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
}

#[derive(Debug)]
pub enum Expr<'a> {
    Identifier(Box<Identifier<'a>>),
    Integer(Box<IntegerLiteral<'a>>),
}

impl<'a> Node<'a> for Expr<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Integer(n) => vec![n.as_node()],
            Self::Identifier(ident) => vec![ident.as_node()],
        }
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
}

#[derive(Debug)]
pub enum Statement<'a> {
    Error(Box<ErrorStatement<'a>>),
    Assignment(Box<Assignment<'a>>),
}

impl<'a> Node<'a> for Statement<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        match self {
            Self::Error(e) => vec![e.as_node()],
            Self::Assignment(assign) => vec![assign.as_node()],
        }
    }
}

#[derive(Debug)]
pub struct Probe<'a> {
    pub attach_point: &'a str,
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
}

#[derive(Debug)]
pub struct Program<'a> {
    pub probes: Vec<Probe<'a>>,
    pub span: Span<'a>,
}

impl<'a> Node<'a> for Program<'a> {
    fn as_node(&self) -> &dyn Node<'a> {
        self
    }

    fn children(&self) -> Vec<&dyn Node<'a>> {
        self.probes.iter().map(|p| p.as_node()).collect()
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
}

impl<'a, 'b> Node<'a> for ErrorRef<'a, 'b> {
    fn as_node(&self) -> &'b dyn Node<'a> {
        match self {
            Self::Statement(stmt) => stmt.as_node(),
        }
    }

    fn children(&self) -> Vec<&'b dyn Node<'a>> {
        match self {
            Self::Statement(stmt) => stmt.children(),
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
}
