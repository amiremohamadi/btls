use std::sync::Arc;

use crate::builtins::BUILTINS;
use crate::common::utils::OwnedLineIndex;
use crate::parser::{
    Block, Expr, IdentKind, Loop, Lvalue, Node, Preamble, Probe, Program, Statement, UndefinedFunc,
    UndefinedIdent,
};
use crate::server::Context;
use crate::storage::Document;
use anyhow::Result;
use std::path::Path;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::OwnedAst;

fn var_prefix(kind: IdentKind) -> &'static str {
    match kind {
        IdentKind::Scratch => "$",
        IdentKind::Map => "@",
        IdentKind::Bare => "",
    }
}

#[derive(Debug, Clone)]
pub struct VarLoc {
    pub offset: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub name: String,
    pub locs: Vec<VarLoc>,
}

impl VarInfo {
    pub fn defined_at(&self) -> usize {
        self.locs[0].offset
    }

    pub fn summary(&self, line_index: &OwnedLineIndex) -> String {
        let line = line_index.position(self.defined_at()).line + 1;
        if self.locs.len() == 1 {
            format!("defined at line {line}")
        } else {
            format!("defined and modified in {} locations", self.locs.len())
        }
    }

    pub fn documentation(&self, line_index: &OwnedLineIndex, file_uri: &tower_lsp::lsp_types::Url) -> String {
        let pos = line_index.position(self.defined_at());
        let (line, col) = (pos.line + 1, pos.character + 1);
        if self.locs.len() == 1 {
            let label = format!("{}:{}:{}", file_uri.path(), line, col);
            let link = format!("{file_uri}#L{line},{col}");
            format!("Defined at [{label}]({link})")
        } else {
            format!("Defined and modified in {} locations", self.locs.len())
        }
    }
}

type RawVar = (String, VarLoc);

fn merge_vars(raw: Vec<RawVar>) -> Vec<VarInfo> {
    let mut map: std::collections::HashMap<String, Vec<VarLoc>> =
        std::collections::HashMap::new();
    for (name, loc) in raw {
        map.entry(name).or_default().push(loc);
    }
    map.into_iter()
        .map(|(name, mut locs)| {
            locs.sort_by_key(|l| l.offset);
            VarInfo { name, locs }
        })
        .collect()
}

fn collect_global_maps(program: &Program) -> Vec<RawVar> {
    let mut maps = Vec::new();
    for preamble in &program.preambles {
        match preamble {
            Preamble::Probe(probe) => collect_maps_in_block(&probe.block, &mut maps),
            Preamble::Error(_) => {}
        }
    }
    maps
}

fn collect_maps_in_block(block: &Block, maps: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign) => {
                let Lvalue::Identifier(ident) = &assign.lvalue;
                if ident.kind == IdentKind::Map {
                    maps.push((
                        format!("@{}", ident.name),
                        VarLoc {
                            offset: assign.span.start(),
                            text: assign.span.as_str().to_string(),
                        },
                    ));
                }
            }
            Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                Loop::For(for_loop) => {
                    if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                        if ident.kind == IdentKind::Map {
                            maps.push((
                                format!("@{}", ident.name),
                                VarLoc {
                                    offset: loop_stmt.span().start(),
                                    text: loop_stmt.span().as_str().to_string(),
                                },
                            ));
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

pub fn variables_at(program: &Program, offset: usize) -> Vec<VarInfo> {
    let mut raw = Vec::new();
    for preamble in &program.preambles {
        if preamble.span().start() > offset {
            break;
        }
        if preamble.span().start() <= offset && offset < preamble.span().end() {
            collect_vars_in_preamble(preamble, offset, &mut raw);
        }
    }
    raw.extend(collect_global_maps(program));
    merge_vars(raw)
}

fn collect_vars_in_preamble(preamble: &Preamble, offset: usize, vars: &mut Vec<RawVar>) {
    match preamble {
        Preamble::Probe(probe) => {
            collect_vars_in_block(&probe.block, offset, vars);
        }
        Preamble::Error(_) => {}
    }
}

fn collect_vars_in_block(block: &Block, offset: usize, vars: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        if stmt.span().start() > offset {
            break;
        }
        match stmt {
            Statement::Assignment(assign) => {
                let Lvalue::Identifier(ident) = &assign.lvalue;
                if ident.kind != IdentKind::Map {
                    vars.push((
                        format!("{}{}", var_prefix(ident.kind), ident.name),
                        VarLoc {
                            offset: assign.span.start(),
                            text: assign.span.as_str().to_string(),
                        },
                    ));
                }
            }
            Statement::Loop(loop_stmt) => {
                if let Loop::For(for_loop) = loop_stmt.as_ref() {
                    if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                        if ident.kind != IdentKind::Map {
                            vars.push((
                                format!("{}{}", var_prefix(ident.kind), ident.name),
                                VarLoc {
                                    offset: loop_stmt.span().start(),
                                    text: loop_stmt.span().as_str().to_string(),
                                },
                            ));
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

pub struct SemanticAnalyzer;

pub struct AnalyzedFile {
    pub document: Arc<Document>,
    pub variables: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    ast_cell: OwnedAst,
}

impl AnalyzedFile {
    pub fn ast(&self) -> &Program<'_> {
        self.ast_cell.program()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self
    }

    fn walk_variables(program: &Program) -> Vec<String> {
        let mut variables = vec![];
        for preamble in &program.preambles {
            if let Preamble::Probe(probe) = preamble {
                Self::walk_vars_in_block(&probe.block, &mut variables);
            }
        }
        variables
    }

    fn walk_vars_in_block(block: &Block, variables: &mut Vec<String>) {
        for stmt in &block.statements {
            match stmt {
                Statement::Assignment(a) => {
                    let Lvalue::Identifier(ident) = &a.lvalue;
                    variables.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                }
                Statement::Loop(loop_stmt) => {
                    if let Loop::For(for_loop) = loop_stmt.as_ref() {
                        if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                            variables.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                        }
                        Self::walk_vars_in_block(&for_loop.block, variables);
                    }
                    if let Loop::While(w) = loop_stmt.as_ref() {
                        Self::walk_vars_in_block(&w.block, variables);
                    }
                }
                Statement::IfCond(if_cond) => {
                    Self::walk_vars_in_block(&if_cond.block, variables);
                }
                Statement::Error(_) | Statement::Expr(_) => {}
            }
        }
    }

    fn collect_errors(
        program: &Program,
        global_maps: &[VarInfo],
        line_index: &OwnedLineIndex,
    ) -> Vec<Diagnostic> {
        let mut errors = vec![];
        for preamble in &program.preambles {
            match preamble {
                Preamble::Probe(probe) => {
                    Self::check_probe_errors(probe, global_maps, &mut errors, line_index);
                }
                Preamble::Error(e) => {
                    errors.push(Diagnostic {
                        range: line_index.range(e.span()),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: e.diagnosis(),
                        ..Default::default()
                    });
                }
            }
        }
        errors
    }

    fn emit_diag(
        stmt: &Statement,
        line_index: &OwnedLineIndex,
        out: &mut Vec<Diagnostic>,
    ) {
        if let Statement::Error(e) = stmt {
            out.push(Diagnostic {
                range: line_index.range(e.span()),
                severity: Some(DiagnosticSeverity::ERROR),
                message: e.diagnosis(),
                ..Default::default()
            });
        }
    }

    fn check_probe_errors(
        probe: &Probe,
        global_maps: &[VarInfo],
        out: &mut Vec<Diagnostic>,
        line_index: &OwnedLineIndex,
    ) {
        let mut scope = Vec::new();
        if let Some(cond) = &probe.condition {
            Self::check_expr_errors(cond, &scope, global_maps, out, line_index);
        }
        Self::check_block_errors(&probe.block, &mut scope, global_maps, out, line_index);
    }

    fn check_block_errors(
        block: &Block,
        scope: &mut Vec<String>,
        global_maps: &[VarInfo],
        out: &mut Vec<Diagnostic>,
        line_index: &OwnedLineIndex,
    ) {
        for stmt in &block.statements {
            Self::emit_diag(stmt, line_index, out);
            match stmt {
                Statement::Assignment(assign) => {
                    Self::check_expr_errors(&assign.rvalue, scope, global_maps, out, line_index);
                    let Lvalue::Identifier(ident) = &assign.lvalue;
                    if ident.kind != IdentKind::Map {
                        scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                    }
                }
                Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                    Loop::For(for_loop) => {
                        Self::check_expr_errors(&for_loop.rhs, scope, global_maps, out, line_index);
                        if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                            if ident.kind != IdentKind::Map {
                                scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                            }
                        }
                        let mut inner = scope.clone();
                        Self::check_block_errors(
                            &for_loop.block,
                            &mut inner,
                            global_maps,
                            out,
                            line_index,
                        );
                    }
                    Loop::While(w) => {
                        Self::check_expr_errors(
                            &w.condition,
                            scope,
                            global_maps,
                            out,
                            line_index,
                        );
                        let mut inner = scope.clone();
                        Self::check_block_errors(
                            &w.block,
                            &mut inner,
                            global_maps,
                            out,
                            line_index,
                        );
                    }
                },
                Statement::IfCond(if_cond) => {
                    Self::check_expr_errors(
                        &if_cond.condition,
                        scope,
                        global_maps,
                        out,
                        line_index,
                    );
                    let mut inner = scope.clone();
                    Self::check_block_errors(
                        &if_cond.block,
                        &mut inner,
                        global_maps,
                        out,
                        line_index,
                    );
                }
                Statement::Expr(expr) => {
                    Self::check_expr_errors(expr, scope, global_maps, out, line_index);
                }
                Statement::Error(_) => {}
            }
        }
    }

    fn check_expr_errors(
        expr: &Expr,
        scope: &[String],
        global_maps: &[VarInfo],
        out: &mut Vec<Diagnostic>,
        line_index: &OwnedLineIndex,
    ) {
        match expr {
            Expr::Identifier(ident) => match ident.kind {
                IdentKind::Bare => {
                    if !BUILTINS.keywords.iter().any(|k| k.name == ident.name) {
                        Self::emit_diag(
                            &UndefinedIdent::new(ident.name, ident.span),
                            line_index,
                            out,
                        );
                    }
                }
                IdentKind::Scratch => {
                    if !scope.contains(&format!("${}", ident.name)) {
                        Self::emit_diag(
                            &UndefinedIdent::new(ident.name, ident.span),
                            line_index,
                            out,
                        );
                    }
                }
                IdentKind::Map => {
                    if !global_maps
                        .iter()
                        .any(|m| m.name == format!("@{}", ident.name))
                    {
                        Self::emit_diag(
                            &UndefinedIdent::new(ident.name, ident.span),
                            line_index,
                            out,
                        );
                    }
                }
            },
            Expr::Call(call) => {
                if !BUILTINS.functions.iter().any(|f| f.name == call.func.name) {
                    Self::emit_diag(
                        &UndefinedFunc::new(call.func.name, call.span()),
                        line_index,
                        out,
                    );
                }
                for arg in &call.args {
                    Self::check_expr_errors(arg, scope, global_maps, out, line_index);
                }
            }
            Expr::BinaryExpr(bin) => {
                Self::check_expr_errors(&bin.lhs, scope, global_maps, out, line_index);
                Self::check_expr_errors(&bin.rhs, scope, global_maps, out, line_index);
            }
            Expr::UnaryExpr(unary) => {
                Self::check_expr_errors(&unary.expr, scope, global_maps, out, line_index);
            }
            Expr::Integer(_) | Expr::String(_) => {}
        }
    }

    pub async fn analyze(&self, context: &Context, path: &Path) -> Result<AnalyzedFile> {
        let document = context.storage.lock().await.read(path);
        let source = document.data.clone();
        let line_index = document.line_index.clone();
        let ast_cell = OwnedAst::new(source)?;

        let program = ast_cell.program();
        let global_maps = merge_vars(collect_global_maps(program));
        let diagnostics = Self::collect_errors(program, &global_maps, &line_index);
        let variables = Self::walk_variables(program);

        Ok(AnalyzedFile {
            document,
            ast_cell,
            variables,
            diagnostics,
        })
    }
}
