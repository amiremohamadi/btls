use pest::Span;

use crate::parser::{
    Assignment, Block, CDef, Config, Else, ErrorPreamble, ErrorStatement, Expr, FieldDecl,
    Identifier, If, Loop, MacroDefinition, Node, Preamble, Probe, Program, Statement,
};

#[derive(Clone)]
pub struct AnalyzedBlock<'a> {
    pub statements: Vec<AnalyzedStatement<'a>>,
    pub span: Span<'a>,
}

#[derive(Clone)]
pub enum AnalyzedStatement<'a> {
    Assignment(Box<AnalyzedAssignment<'a>>),
    IfCond(Box<AnalyzedIfCond<'a>>),
    Loop(Box<AnalyzedLoop<'a>>),
    Expr(Box<AnalyzedExprStmt<'a>>),
    Error(&'a ErrorStatement<'a>),
}

impl<'a> AnalyzedStatement<'a> {
    pub fn span(&self) -> Span<'a> {
        match self {
            Self::Assignment(a) => a.assignment.span,
            Self::IfCond(c) => c.if_cond.span,
            Self::Loop(l) => l.loop_stmt.span(),
            Self::Expr(e) => e.expr.span(),
            Self::Error(e) => e.span(),
        }
    }

    pub fn has_semicolon(&self) -> bool {
        match self {
            Self::Assignment(a) => a.has_semicolon,
            Self::Expr(e) => e.has_semicolon,
            _ => true,
        }
    }

    pub fn subscopes(&self) -> Vec<&AnalyzedBlock<'a>> {
        match self {
            Self::IfCond(cond) => {
                let mut blocks = vec![&cond.then_block];
                let mut current = cond.else_branch.as_deref();
                while let Some(else_branch) = current {
                    match else_branch {
                        AnalyzedElseBranch::IfCond(next) => {
                            blocks.push(&next.then_block);
                            current = next.else_branch.as_deref();
                        }
                        AnalyzedElseBranch::Block(block) => {
                            blocks.push(block);
                            break;
                        }
                    }
                }
                blocks
            }
            Self::Loop(l) => vec![&l.body_block],
            Self::Assignment(_) | Self::Expr(_) | Self::Error(_) => vec![],
        }
    }
}

#[derive(Clone)]
pub struct AnalyzedAssignment<'a> {
    pub assignment: &'a Assignment<'a>,
    pub has_semicolon: bool,
}

#[derive(Clone)]
pub struct AnalyzedIfCond<'a> {
    pub if_cond: &'a If<'a>,
    pub then_block: AnalyzedBlock<'a>,
    pub else_branch: Option<Box<AnalyzedElseBranch<'a>>>,
}

#[derive(Clone)]
pub enum AnalyzedElseBranch<'a> {
    IfCond(Box<AnalyzedIfCond<'a>>),
    Block(AnalyzedBlock<'a>),
}

#[derive(Clone)]
pub struct AnalyzedLoop<'a> {
    pub loop_stmt: &'a Loop<'a>,
    pub body_block: AnalyzedBlock<'a>,
    pub variable: Option<&'a Identifier<'a>>,
}

#[derive(Clone)]
pub struct AnalyzedExprStmt<'a> {
    pub expr: &'a Expr<'a>,
    pub has_semicolon: bool,
}

#[derive(Clone)]
pub struct AnalyzedFieldDecl<'a> {
    pub field: &'a FieldDecl<'a>,
    pub has_semicolon: bool,
}

impl<'a> AnalyzedFieldDecl<'a> {
    pub fn has_semicolon(&self) -> bool {
        self.has_semicolon
    }
}

#[derive(Clone)]
pub struct AnalyzedStructDef<'a> {
    pub fields: Vec<AnalyzedFieldDecl<'a>>,
    pub span: pest::Span<'a>,
}

#[derive(Clone)]
pub enum AnalyzedPreamble<'a> {
    Probe(&'a Probe<'a>, AnalyzedBlock<'a>),
    Macro(&'a MacroDefinition<'a>, AnalyzedBlock<'a>),
    CDef(&'a CDef<'a>),
    AnalyzedStruct(AnalyzedStructDef<'a>),
    Config(&'a Config<'a>),
    Error(&'a ErrorPreamble<'a>),
}

impl<'a> AnalyzedPreamble<'a> {
    pub fn span(&self) -> Span<'a> {
        match self {
            Self::Probe(p, _) => p.span(),
            Self::Macro(m, _) => m.span(),
            Self::CDef(c) => c.span(),
            Self::AnalyzedStruct(s) => s.span,
            Self::Config(c) => c.span(),
            Self::Error(e) => e.span(),
        }
    }
}

#[derive(Clone)]
pub struct AnalyzedProgram<'a> {
    pub preambles: Vec<AnalyzedPreamble<'a>>,
    #[allow(dead_code)]
    pub span: Span<'a>,
}

pub fn analyze_program<'a>(program: &'a Program<'a>) -> AnalyzedProgram<'a> {
    AnalyzedProgram {
        preambles: program.preambles.iter().map(analyze_preamble).collect(),
        span: program.span,
    }
}

fn analyze_preamble<'a>(preamble: &'a Preamble<'a>) -> AnalyzedPreamble<'a> {
    match preamble {
        Preamble::Probe(probe) => AnalyzedPreamble::Probe(probe, analyze_block(&probe.block)),
        Preamble::Macro(m) => AnalyzedPreamble::Macro(m.as_ref(), analyze_block(&m.body)),
        Preamble::CDef(cdef) => match cdef.as_ref() {
            CDef::Struct(def) => {
                let fields: Vec<AnalyzedFieldDecl> = def
                    .fields
                    .iter()
                    .map(|(f, has_semicolon)| AnalyzedFieldDecl {
                        field: f,
                        has_semicolon: *has_semicolon,
                    })
                    .collect();
                AnalyzedPreamble::AnalyzedStruct(AnalyzedStructDef {
                    fields,
                    span: def.span,
                })
            }
            _ => AnalyzedPreamble::CDef(cdef.as_ref()),
        },
        Preamble::Config(c) => AnalyzedPreamble::Config(c.as_ref()),
        Preamble::Error(e) => AnalyzedPreamble::Error(e.as_ref()),
    }
}

fn analyze_block<'a>(block: &'a Block<'a>) -> AnalyzedBlock<'a> {
    AnalyzedBlock {
        statements: block.statements.iter().map(analyze_statement).collect(),
        span: block.span,
    }
}

fn analyze_statement<'a>(stmt: &'a Statement<'a>) -> AnalyzedStatement<'a> {
    match stmt {
        Statement::Assignment(assign, has_semicolon) => {
            AnalyzedStatement::Assignment(Box::new(AnalyzedAssignment {
                assignment: assign.as_ref(),
                has_semicolon: *has_semicolon,
            }))
        }
        Statement::IfCond(if_cond) => AnalyzedStatement::IfCond(Box::new(analyze_if_cond(if_cond))),
        Statement::Loop(loop_stmt) => AnalyzedStatement::Loop(Box::new(analyze_loop(loop_stmt))),
        Statement::Expr(expr, has_semicolon) => {
            AnalyzedStatement::Expr(Box::new(AnalyzedExprStmt {
                expr: expr.as_ref(),
                has_semicolon: *has_semicolon,
            }))
        }
        Statement::Error(e) => AnalyzedStatement::Error(e.as_ref()),
    }
}

fn analyze_if_cond<'a>(if_cond: &'a If<'a>) -> AnalyzedIfCond<'a> {
    AnalyzedIfCond {
        if_cond,
        then_block: analyze_block(&if_cond.block),
        else_branch: if_cond.else_branch.as_deref().map(analyze_else),
    }
}

fn analyze_else<'a>(else_branch: &'a Else<'a>) -> Box<AnalyzedElseBranch<'a>> {
    Box::new(match else_branch {
        Else::IfCond(if_cond) => AnalyzedElseBranch::IfCond(Box::new(analyze_if_cond(if_cond))),
        Else::Block(block) => AnalyzedElseBranch::Block(analyze_block(block)),
    })
}

fn analyze_loop<'a>(loop_stmt: &'a Loop<'a>) -> AnalyzedLoop<'a> {
    let (body, variable) = match loop_stmt {
        Loop::For(for_loop) => (&for_loop.block, extract_for_variable(&for_loop.lhs)),
        Loop::While(w) => (&w.block, None),
        Loop::Unroll(u) => (&u.block, None),
    };
    AnalyzedLoop {
        loop_stmt,
        body_block: analyze_block(body),
        variable,
    }
}

fn extract_for_variable<'a>(lhs: &'a Expr<'a>) -> Option<&'a Identifier<'a>> {
    match lhs {
        Expr::Identifier(ident) => Some(ident),
        _ => None,
    }
}
