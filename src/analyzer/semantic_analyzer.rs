use std::sync::Arc;

use crate::builtins::BUILTINS;
use crate::parser::{
    Block, Expr, IdentKind, Loop, Lvalue, Node, Preamble, Probe, Program, Statement, UndefinedFunc,
    UndefinedIdent, Walk, ast::parse,
};
use crate::server::Context;
use crate::storage::Document;
use anyhow::Result;
use std::path::Path;

fn var_prefix(kind: IdentKind) -> &'static str {
    match kind {
        IdentKind::Scratch => "$",
        IdentKind::Map => "@",
        IdentKind::Bare => "",
    }
}

fn collect_global_maps(program: &Program) -> Vec<String> {
    let mut maps = Vec::new();
    for preamble in &program.preambles {
        match preamble {
            Preamble::Probe(probe) => collect_maps_in_block(&probe.block, &mut maps),
            Preamble::Error(_) => {}
        }
    }
    maps
}

fn collect_maps_in_block(block: &Block, maps: &mut Vec<String>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign) => {
                let Lvalue::Identifier(ident) = &assign.lvalue;
                if ident.kind == IdentKind::Map {
                    maps.push(format!("@{}", ident.name));
                }
            }
            Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                Loop::For(for_loop) => {
                    if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                        if ident.kind == IdentKind::Map {
                            maps.push(format!("@{}", ident.name));
                        }
                    }
                    collect_maps_in_block(&for_loop.block, maps);
                }
                Loop::While(w) => {
                    collect_maps_in_block(&w.block, maps);
                }
            },
            Statement::IfCond(if_cond) => {
                collect_maps_in_block(&if_cond.block, maps);
            }
            Statement::Expr(_) | Statement::Error(_) => {}
        }
    }
}

pub fn variables_at(program: &Program, offset: usize) -> Vec<String> {
    let mut vars = Vec::new();
    for preamble in &program.preambles {
        if preamble.span().start() > offset {
            break;
        }
        if preamble.span().start() <= offset && offset < preamble.span().end() {
            collect_vars_in_preamble(preamble, offset, &mut vars);
        }
    }
    // @ maps are global — always visible
    let global_maps = collect_global_maps(program);
    vars.extend(global_maps);
    vars
}

fn collect_vars_in_preamble(preamble: &Preamble, offset: usize, vars: &mut Vec<String>) {
    match preamble {
        Preamble::Probe(probe) => {
            collect_vars_in_block(&probe.block, offset, vars);
        }
        Preamble::Error(_) => {}
    }
}

fn collect_vars_in_block(block: &Block, offset: usize, vars: &mut Vec<String>) {
    for stmt in &block.statements {
        if stmt.span().start() > offset {
            break;
        }
        match stmt {
            Statement::Assignment(assign) => {
                let Lvalue::Identifier(ident) = &assign.lvalue;
                if ident.kind != IdentKind::Map {
                    vars.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                }
            }
            Statement::Loop(loop_stmt) => {
                if let Loop::For(for_loop) = loop_stmt.as_ref() {
                    if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                        if ident.kind != IdentKind::Map {
                            vars.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                        }
                    }
                }
                match loop_stmt.as_ref() {
                    Loop::While(w) => {
                        if w.block.span().start() <= offset && offset < w.block.span().end() {
                            collect_vars_in_block(&w.block, offset, vars);
                        }
                    }
                    Loop::For(f) => {
                        if f.block.span().start() <= offset && offset < f.block.span().end() {
                            collect_vars_in_block(&f.block, offset, vars);
                        }
                    }
                }
            }
            Statement::IfCond(if_cond) => {
                if if_cond.block.span().start() <= offset && offset < if_cond.block.span().end() {
                    collect_vars_in_block(&if_cond.block, offset, vars);
                }
            }
            Statement::Expr(_) | Statement::Error(_) => {}
        }
    }
}

pub struct SemanticAnalyzer {
    pub content: String,
}

pub struct AnalyzedFile<'a> {
    pub document: Arc<Document>,
    pub variables: Vec<String>,
    pub ast: Program<'a>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            content: "".to_string(),
        }
    }

    pub async fn analyze(&mut self, context: &Context, path: &Path) -> Result<AnalyzedFile<'_>> {
        let document = context.storage.lock().await.read(path);
        self.content = (*document.data).clone();
        let mut ast = parse(&self.content)?;
        let mut errors = vec![];
        let global_maps = collect_global_maps(&ast);

        for preamble in &ast.preambles {
            if let Preamble::Probe(probe) = preamble {
                check_probe(probe, &global_maps, &mut errors);
            }
        }

        let mut variables = vec![];
        let root = Walk::new(ast.as_node());
        root.into_iter().for_each(|n| {
            if let Some(stmt) = n.as_statement() {
                match stmt {
                    Statement::Loop(x) => {
                        if let Loop::For(x) = x.as_ref() {
                            if let Expr::Identifier(ident) = x.lhs.as_ref() {
                                variables.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                            }
                        }
                    }
                    Statement::Assignment(a) => match &a.lvalue {
                        Lvalue::Identifier(ident) => {
                            variables.push(format!("{}{}", var_prefix(ident.kind), ident.name))
                        }
                    },
                    _ => {}
                }
            }
        });

        // TODO: append errors to their associated block
        // currently, we just append the errors to the first block (which works fine)
        ast.preambles
            .iter_mut()
            .filter_map(|x| match x {
                Preamble::Probe(p) => Some(&mut p.block),
                _ => None,
            })
            .next()
            .map(|x| x.statements.extend(errors));

        Ok(AnalyzedFile {
            document,
            ast,
            variables,
        })
    }
}

fn check_probe<'a>(
    probe: &Probe<'a>,
    global_maps: &[String],
    errors: &mut Vec<Statement<'a>>,
) {
    let mut scope = Vec::new();
    if let Some(cond) = &probe.condition {
        check_expr(cond, &scope, global_maps, errors);
    }
    check_block(&probe.block, &mut scope, global_maps, errors);
}

fn check_block<'a>(
    block: &Block<'a>,
    scope: &mut Vec<String>,
    global_maps: &[String],
    errors: &mut Vec<Statement<'a>>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign) => {
                check_expr(&assign.rvalue, scope, global_maps, errors);
                let Lvalue::Identifier(ident) = &assign.lvalue;
                if ident.kind != IdentKind::Map {
                    scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                }
            }
            Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                Loop::For(for_loop) => {
                    check_expr(&for_loop.lhs, scope, global_maps, errors);
                    check_expr(&for_loop.rhs, scope, global_maps, errors);
                    if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                        if ident.kind != IdentKind::Map {
                            scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                        }
                    }
                    let mut inner = scope.clone();
                    check_block(&for_loop.block, &mut inner, global_maps, errors);
                }
                Loop::While(w) => {
                    check_expr(&w.condition, scope, global_maps, errors);
                    let mut inner = scope.clone();
                    check_block(&w.block, &mut inner, global_maps, errors);
                }
            },
            Statement::IfCond(if_cond) => {
                check_expr(&if_cond.condition, scope, global_maps, errors);
                let mut inner = scope.clone();
                check_block(&if_cond.block, &mut inner, global_maps, errors);
            }
            Statement::Expr(expr) => {
                check_expr(expr, scope, global_maps, errors);
            }
            Statement::Error(_) => {}
        }
    }
}

fn check_expr<'a>(
    expr: &Expr<'a>,
    scope: &[String],
    global_maps: &[String],
    errors: &mut Vec<Statement<'a>>,
) {
    match expr {
        Expr::Identifier(ident) => match ident.kind {
            IdentKind::Bare => {
                if !BUILTINS.keywords.iter().any(|k| k.name == ident.name) {
                    errors.push(UndefinedIdent::new(ident.name, ident.span));
                }
            }
            IdentKind::Scratch => {
                if !scope.contains(&format!("${}", ident.name)) {
                    errors.push(UndefinedIdent::new(ident.name, ident.span));
                }
            }
            IdentKind::Map => {
                if !global_maps.contains(&format!("@{}", ident.name)) {
                    errors.push(UndefinedIdent::new(ident.name, ident.span));
                }
            }
        },
        Expr::Call(call) => {
            if !BUILTINS.functions.iter().any(|f| f.name == call.func.name) {
                errors.push(UndefinedFunc::new(call.func.name, call.span()));
            }
            for arg in &call.args {
                check_expr(arg, scope, global_maps, errors);
            }
        }
        Expr::BinaryExpr(bin) => {
            check_expr(&bin.lhs, scope, global_maps, errors);
            check_expr(&bin.rhs, scope, global_maps, errors);
        }
        Expr::UnaryExpr(unary) => {
            check_expr(&unary.expr, scope, global_maps, errors);
        }
        Expr::Integer(_) | Expr::String(_) => {}
    }
}
