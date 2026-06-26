use std::sync::Arc;

use crate::builtins::BUILTINS;
use crate::common::utils::OwnedLineIndex;
use crate::parser::{
    Block, CDef, Else, Expr, IdentKind, Loop, Lvalue, Node, Preamble, Probe, Program, Statement,
    UnaryOp, UndefinedFunc, UndefinedIdent,
};
use crate::server::Context;
use crate::storage::Document;
use anyhow::Result;
use pest::Span;
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

    pub fn documentation(
        &self,
        line_index: &OwnedLineIndex,
        file_uri: &tower_lsp::lsp_types::Url,
    ) -> String {
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
    let mut map: std::collections::HashMap<String, Vec<VarLoc>> = std::collections::HashMap::new();
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

fn collect_defines(program: &Program) -> Vec<RawVar> {
    let mut defines = Vec::new();
    for preamble in &program.preambles {
        if let Preamble::CDef(cdef) = preamble {
            if let CDef::Define(define) = cdef.as_ref() {
                defines.push((
                    define.name.name.to_string(),
                    VarLoc {
                        offset: define.span.start(),
                        text: define.span.as_str().to_string(),
                    },
                ));
            }
        }
    }
    defines
}

fn collect_global_maps(program: &Program) -> Vec<RawVar> {
    let mut maps = Vec::new();
    for preamble in &program.preambles {
        match preamble {
            Preamble::Probe(probe) => collect_maps_in_block(&probe.block, &mut maps),
            Preamble::CDef(_) | Preamble::Config(_) | Preamble::Error(_) => {}
        }
    }
    maps
}

fn collect_maps_in_block(block: &Block, maps: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign) => {
                let map_name = match &assign.lvalue {
                    Lvalue::Identifier(ident) if ident.kind == IdentKind::Map => ident.name,
                    Lvalue::MapAccess(access) => access.map.name,
                    _ => continue,
                };
                maps.push((
                    format!("@{}", map_name),
                    VarLoc {
                        offset: assign.span.start(),
                        text: assign.span.as_str().to_string(),
                    },
                ));
            }
            Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                Loop::For(for_loop) => {
                    let map_name = match for_loop.lhs.as_ref() {
                        Expr::Identifier(ident) if ident.kind == IdentKind::Map => ident.name,
                        Expr::MapAccess(access) => access.map.name,
                        _ => continue,
                    };
                    maps.push((
                        format!("@{}", map_name),
                        VarLoc {
                            offset: loop_stmt.span().start(),
                            text: loop_stmt.span().as_str().to_string(),
                        },
                    ));
                    collect_maps_in_block(&for_loop.block, maps);
                }
                Loop::While(w) => {
                    collect_maps_in_block(&w.block, maps);
                }
            },
            Statement::IfCond(if_cond) => {
                collect_maps_in_block(&if_cond.block, maps);
                if let Some(else_branch) = &if_cond.else_branch {
                    collect_maps_in_else(else_branch, maps);
                }
            }
            Statement::Expr(expr) => {
                if let Expr::UnaryExpr(unary) = expr.as_ref() {
                    if matches!(unary.op, UnaryOp::Inc | UnaryOp::Dec) {
                        let map_name = match unary.expr.as_ref() {
                            Expr::Identifier(ident) if ident.kind == IdentKind::Map => ident.name,
                            Expr::MapAccess(access) => access.map.name,
                            _ => continue,
                        };
                        maps.push((
                            format!("@{}", map_name),
                            VarLoc {
                                offset: unary.span.start(),
                                text: unary.span.as_str().to_string(),
                            },
                        ));
                    }
                }
            }
            Statement::Error(_) => {}
        }
    }
}

fn collect_maps_in_else(else_branch: &Else, maps: &mut Vec<RawVar>) {
    match else_branch {
        Else::IfCond(if_cond) => {
            collect_maps_in_block(&if_cond.block, maps);
            if let Some(next_else) = &if_cond.else_branch {
                collect_maps_in_else(next_else, maps);
            }
        }
        Else::Block(block) => collect_maps_in_block(block, maps),
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
    raw.extend(collect_defines(program));
    merge_vars(raw)
}

fn collect_vars_in_preamble(preamble: &Preamble, offset: usize, vars: &mut Vec<RawVar>) {
    match preamble {
        Preamble::Probe(probe) => {
            collect_vars_in_block(&probe.block, offset, vars);
        }
        Preamble::CDef(_) | Preamble::Config(_) | Preamble::Error(_) => {}
    }
}

fn collect_vars_in_block(block: &Block, offset: usize, vars: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        if stmt.span().start() > offset {
            break;
        }
        match stmt {
            Statement::Assignment(assign) => {
                if let Some(ident) = match &assign.lvalue {
                    Lvalue::Identifier(ident) if ident.kind != IdentKind::Map => Some(ident),
                    _ => None,
                } {
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
                if let Some(else_branch) = &if_cond.else_branch {
                    collect_vars_in_else(else_branch, offset, vars);
                }
            }
            Statement::Expr(expr) => {
                if let Expr::UnaryExpr(unary) = expr.as_ref() {
                    if matches!(unary.op, UnaryOp::Inc | UnaryOp::Dec) {
                        if let Some(ident) = match unary.expr.as_ref() {
                            Expr::Identifier(ident) if ident.kind != IdentKind::Map => Some(ident),
                            _ => None,
                        } {
                            vars.push((
                                format!("{}{}", var_prefix(ident.kind), ident.name),
                                VarLoc {
                                    offset: unary.span.start(),
                                    text: unary.span.as_str().to_string(),
                                },
                            ));
                        }
                    }
                }
            }
            Statement::Error(_) => {}
        }
    }
}

fn collect_vars_in_else(else_branch: &Else, offset: usize, vars: &mut Vec<RawVar>) {
    match else_branch {
        Else::IfCond(if_cond) => {
            if if_cond.block.span().start() <= offset && offset < if_cond.block.span().end() {
                collect_vars_in_block(&if_cond.block, offset, vars);
            }
            if let Some(next_else) = &if_cond.else_branch {
                collect_vars_in_else(next_else, offset, vars);
            }
        }
        Else::Block(block) => {
            if block.span().start() <= offset && offset < block.span().end() {
                collect_vars_in_block(block, offset, vars);
            }
        }
    }
}

struct ErrorChecker<'a> {
    global_maps: &'a [VarInfo],
    defines: &'a [VarInfo],
    line_index: &'a OwnedLineIndex,
    out: Vec<Diagnostic>,
}

impl ErrorChecker<'_> {
    fn check_program(&mut self, program: &Program) {
        for preamble in &program.preambles {
            match preamble {
                Preamble::Probe(probe) => self.check_probe(probe),
                Preamble::CDef(_) | Preamble::Config(_) => {}
                Preamble::Error(e) => self.push_span_error(e.span(), e.diagnosis()),
            }
        }
    }

    fn push_span_error(&mut self, span: Span, message: String) {
        self.out.push(Diagnostic {
            range: self.line_index.range(span),
            severity: Some(DiagnosticSeverity::ERROR),
            message,
            ..Default::default()
        });
    }

    fn emit_diag(&mut self, stmt: &Statement) {
        if let Statement::Error(e) = stmt {
            self.push_span_error(e.span(), e.diagnosis());
        }
    }

    fn check_probe(&mut self, probe: &Probe) {
        let mut scope = Vec::new();
        if let Some(cond) = &probe.condition {
            self.check_expr(cond, &scope);
        }
        self.check_block(&probe.block, &mut scope);
    }

    fn check_block(&mut self, block: &Block, scope: &mut Vec<String>) {
        for stmt in &block.statements {
            self.emit_diag(stmt);
            match stmt {
                Statement::Assignment(assign) => {
                    self.check_expr(&assign.rvalue, scope);
                    match &assign.lvalue {
                        Lvalue::Identifier(ident) if ident.kind != IdentKind::Map => {
                            scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                        }
                        Lvalue::MapAccess(access) => {
                            for key in &access.keys {
                                self.check_expr(key, scope);
                            }
                        }
                        _ => {}
                    }
                }
                Statement::Loop(loop_stmt) => match loop_stmt.as_ref() {
                    Loop::For(for_loop) => {
                        self.check_expr(&for_loop.rhs, scope);
                        if let Expr::Identifier(ident) = for_loop.lhs.as_ref() {
                            if ident.kind != IdentKind::Map {
                                scope.push(format!("{}{}", var_prefix(ident.kind), ident.name));
                            }
                        }
                        let mut inner = scope.clone();
                        self.check_block(&for_loop.block, &mut inner);
                    }
                    Loop::While(w) => {
                        self.check_expr(&w.condition, scope);
                        let mut inner = scope.clone();
                        self.check_block(&w.block, &mut inner);
                    }
                },
                Statement::IfCond(if_cond) => {
                    self.check_expr(&if_cond.condition, scope);
                    let mut inner = scope.clone();
                    self.check_block(&if_cond.block, &mut inner);
                    if let Some(else_branch) = &if_cond.else_branch {
                        self.check_else(else_branch, scope);
                    }
                }
                Statement::Expr(expr) => {
                    self.check_expr(expr, scope);
                }
                Statement::Error(_) => {}
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, scope: &[String]) {
        match expr {
            Expr::Identifier(ident) => match ident.kind {
                IdentKind::Bare => {
                    if !BUILTINS.keywords.iter().any(|k| k.name == ident.name)
                        && !self.defines.iter().any(|d| d.name == ident.name)
                    {
                        self.emit_diag(&UndefinedIdent::new(ident.name, ident.span));
                    }
                }
                IdentKind::Scratch => {
                    if !scope.contains(&format!("${}", ident.name)) {
                        self.emit_diag(&UndefinedIdent::new(ident.name, ident.span));
                    }
                }
                IdentKind::Map => {
                    if !self
                        .global_maps
                        .iter()
                        .any(|m| m.name == format!("@{}", ident.name))
                    {
                        self.out.push(Diagnostic {
                            range: self.line_index.range(ident.span),
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!("Undefined map \"@{}\"", ident.name),
                            ..Default::default()
                        });
                    }
                }
            },
            Expr::Call(call) => {
                if !BUILTINS.functions.iter().any(|f| f.name == call.func.name) {
                    self.emit_diag(&UndefinedFunc::new(call.func.name, call.span()));
                }
                for arg in &call.args {
                    self.check_expr(arg, scope);
                }
            }
            Expr::BinaryExpr(bin) => {
                self.check_expr(&bin.lhs, scope);
                self.check_expr(&bin.rhs, scope);
            }
            Expr::UnaryExpr(unary) => {
                self.check_expr(&unary.expr, scope);
            }
            Expr::MapAccess(access) => {
                if !access.map.name.is_empty()
                    && !self
                        .global_maps
                        .iter()
                        .any(|m| m.name == format!("@{}", access.map.name))
                {
                    self.out.push(Diagnostic {
                        range: self.line_index.range(access.map.span),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Undefined map \"@{}\"", access.map.name),
                        ..Default::default()
                    });
                }
                for key in &access.keys {
                    self.check_expr(key, scope);
                }
            }
            Expr::Integer(_) | Expr::String(_) => {}
        }
    }

    fn check_else(&mut self, else_branch: &Else, scope: &[String]) {
        match else_branch {
            Else::IfCond(if_cond) => {
                self.check_expr(&if_cond.condition, scope);
                let mut inner = scope.to_vec();
                self.check_block(&if_cond.block, &mut inner);
                if let Some(next_else) = &if_cond.else_branch {
                    self.check_else(next_else, scope);
                }
            }
            Else::Block(block) => {
                let mut inner = scope.to_vec();
                self.check_block(block, &mut inner);
            }
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
                    let prefix = match &a.lvalue {
                        Lvalue::Identifier(ident) => {
                            Some(format!("{}{}", var_prefix(ident.kind), ident.name))
                        }
                        Lvalue::MapAccess(access) => Some(format!(
                            "{}{}",
                            var_prefix(access.map.kind),
                            access.map.name
                        )),
                    };
                    if let Some(v) = prefix {
                        variables.push(v);
                    }
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
                    if let Some(else_branch) = &if_cond.else_branch {
                        Self::walk_vars_in_else(else_branch, variables);
                    }
                }
                Statement::Expr(expr) => {
                    if let Expr::UnaryExpr(unary) = expr.as_ref() {
                        if matches!(unary.op, UnaryOp::Inc | UnaryOp::Dec) {
                            match unary.expr.as_ref() {
                                Expr::Identifier(ident) => {
                                    variables.push(format!(
                                        "{}{}",
                                        var_prefix(ident.kind),
                                        ident.name
                                    ));
                                }
                                Expr::MapAccess(access) => {
                                    variables.push(format!(
                                        "{}{}",
                                        var_prefix(access.map.kind),
                                        access.map.name
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Statement::Error(_) => {}
            }
        }
    }

    fn walk_vars_in_else(else_branch: &Else, variables: &mut Vec<String>) {
        match else_branch {
            Else::IfCond(if_cond) => {
                Self::walk_vars_in_block(&if_cond.block, variables);
                if let Some(next_else) = &if_cond.else_branch {
                    Self::walk_vars_in_else(next_else, variables);
                }
            }
            Else::Block(block) => Self::walk_vars_in_block(block, variables),
        }
    }

    fn collect_errors(
        program: &Program,
        global_maps: &[VarInfo],
        defines: &[VarInfo],
        line_index: &OwnedLineIndex,
    ) -> Vec<Diagnostic> {
        let mut checker = ErrorChecker {
            global_maps,
            defines,
            line_index,
            out: Vec::new(),
        };
        checker.check_program(program);
        checker.out
    }

    pub async fn analyze(&self, context: &Context, path: &Path) -> Result<AnalyzedFile> {
        let document = context.storage.lock().await.read(path);
        let source = document.data.clone();
        let line_index = document.line_index.clone();
        let ast_cell = OwnedAst::new(source)?;

        let program = ast_cell.program();
        let global_maps = merge_vars(collect_global_maps(program));
        let defines = merge_vars(collect_defines(program));
        let diagnostics = Self::collect_errors(program, &global_maps, &defines, &line_index);
        let variables = Self::walk_variables(program);

        Ok(AnalyzedFile {
            document,
            ast_cell,
            variables,
            diagnostics,
        })
    }
}
