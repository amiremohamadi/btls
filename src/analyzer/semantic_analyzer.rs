use std::fmt;
use std::sync::Arc;

use crate::builtins::BUILTINS;
use crate::common::utils::OwnedLineIndex;
use crate::parser::{
    Block, CDef, Else, Expr, IdentKind, Loop, Lvalue, MacroDefinition, MacroParam, MacroParamKind,
    Node, Preamble, Probe, Program, Statement, TypeKind, TypeName, UnaryOp, UndefinedFunc,
    UndefinedIdent,
};
use crate::server::Context;
use crate::storage::Document;
use anyhow::Result;
use pest::Span;
use std::collections::HashMap;
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

impl fmt::Display for MacroParamKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacroParamKind::Expr => write!(f, "Expression"),
            MacroParamKind::Scratch => write!(f, "Scratch Variable"),
            MacroParamKind::Map => write!(f, "Map"),
        }
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

fn collect_macros<'a>(program: &'a Program<'a>) -> HashMap<String, &'a MacroDefinition<'a>> {
    let mut macros = HashMap::new();
    for preamble in &program.preambles {
        if let Preamble::Macro(mac) = preamble {
            macros.insert(mac.name.name.to_string(), mac.as_ref());
        }
    }
    macros
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub type_name: TypeInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    Builtin {
        name: String,
        pointers: usize,
    },
    StructLike {
        kind: TypeKind,
        name: String,
        pointers: usize,
    },
    Invalid {
        name: String,
        pointers: usize,
    },
}

impl TypeInfo {
    pub fn name(&self) -> &str {
        match self {
            Self::Builtin { name, .. }
            | Self::StructLike { name, .. }
            | Self::Invalid { name, .. } => name,
        }
    }

    pub fn pointers(&self) -> usize {
        match self {
            Self::Builtin { pointers, .. }
            | Self::StructLike { pointers, .. }
            | Self::Invalid { pointers, .. } => *pointers,
        }
    }
}

impl fmt::Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pointers() > 0 {
            write!(f, "{} {}", self.name(), "*".repeat(self.pointers()))
        } else {
            write!(f, "{}", self.name())
        }
    }
}

impl<'a> From<&'a TypeName<'a>> for TypeInfo {
    fn from(type_name: &'a TypeName<'a>) -> Self {
        match type_name.kind {
            TypeKind::Builtin => TypeInfo::Builtin {
                name: type_name.name.to_string(),
                pointers: type_name.pointers,
            },
            TypeKind::Struct | TypeKind::Union => TypeInfo::StructLike {
                kind: type_name.kind,
                name: type_name.name.to_string(),
                pointers: type_name.pointers,
            },
            TypeKind::Invalid => TypeInfo::Invalid {
                name: type_name.name.to_string(),
                pointers: type_name.pointers,
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StructInfo {
    pub fields: Vec<FieldInfo>,
}

pub fn collect_structs(program: &Program) -> HashMap<String, StructInfo> {
    let mut structs = HashMap::new();
    for preamble in &program.preambles {
        if let Preamble::CDef(cdef) = preamble {
            if let CDef::Struct(def) = cdef.as_ref() {
                let info = StructInfo {
                    fields: def
                        .fields
                        .iter()
                        .filter(|f| f.type_name.validate(&structs))
                        .map(|f| FieldInfo {
                            name: f.name.name.to_string(),
                            type_name: TypeInfo::from(&f.type_name),
                        })
                        .collect(),
                };
                structs.insert(def.name.name.to_string(), info);
            }
        }
    }
    structs
}

pub fn collect_var_types(program: &Program) -> HashMap<String, TypeInfo> {
    let structs = collect_structs(program);
    let mut types = HashMap::new();
    for preamble in &program.preambles {
        if let Preamble::Probe(probe) = preamble {
            _collect_var_types(&probe.block, &structs, &mut types);
        }
    }
    types
}

fn _collect_var_types(
    block: &Block,
    structs: &HashMap<String, StructInfo>,
    types: &mut HashMap<String, TypeInfo>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign, _) => {
                if let Lvalue::Identifier(ident) = &assign.lvalue {
                    if ident.kind == IdentKind::Scratch {
                        if let Some(type_info) = resolve_expr_type(&assign.rvalue, structs, types) {
                            types.insert(format!("${}", ident.name), type_info);
                        }
                    }
                }
            }
            Statement::Loop(loop_stmt) => {
                let block = match loop_stmt.as_ref() {
                    Loop::For(for_loop) => &for_loop.block,
                    Loop::While(w) => &w.block,
                    Loop::Unroll(u) => &u.block,
                };
                _collect_var_types(block, structs, types);
            }
            Statement::IfCond(if_cond) => {
                _collect_var_types(&if_cond.block, structs, types);

                let mut next_else = if_cond.else_branch.as_deref();
                while let Some(else_branch) = next_else {
                    match else_branch {
                        Else::IfCond(next_if) => {
                            _collect_var_types(&next_if.block, structs, types);
                            next_else = next_if.else_branch.as_deref();
                        }
                        Else::Block(block) => {
                            _collect_var_types(block, structs, types);
                            break;
                        }
                    }
                }
            }
            Statement::Expr(_, _) | Statement::Error(_) => {}
        }
    }
}

pub fn resolve_expr_type(
    expr: &Expr,
    structs: &HashMap<String, StructInfo>,
    var_types: &HashMap<String, TypeInfo>,
) -> Option<TypeInfo> {
    match expr {
        Expr::Cast(cast) => Some(TypeInfo::from(&cast.type_name)),
        Expr::Identifier(ident) if ident.kind == IdentKind::Scratch => {
            var_types.get(&format!("${}", ident.name)).cloned()
        }
        Expr::UnaryExpr(u) if u.op == UnaryOp::Deref => {
            resolve_expr_type(&u.expr, structs, var_types)
        }
        Expr::Field(fa) => {
            let base_type = resolve_expr_type(&fa.base, structs, var_types)?;
            let field = fa.field.as_ref()?;
            match &base_type {
                TypeInfo::StructLike { name, .. } => structs
                    .get(name)?
                    .fields
                    .iter()
                    .find(|f| f.name == field.name)
                    .map(|f| f.type_name.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn collect_global_maps(program: &Program) -> Vec<RawVar> {
    let mut maps = Vec::new();
    for preamble in &program.preambles {
        match preamble {
            Preamble::Probe(probe) => collect_maps_in_block(&probe.block, &mut maps),
            Preamble::CDef(_) | Preamble::Macro(_) | Preamble::Config(_) | Preamble::Error(_) => {}
        }
    }
    maps
}

fn collect_maps_in_block(block: &Block, maps: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(assign, _) => {
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
                Loop::Unroll(u) => {
                    collect_maps_in_block(&u.block, maps);
                }
            },
            Statement::IfCond(if_cond) => {
                collect_maps_in_block(&if_cond.block, maps);
                if let Some(else_branch) = &if_cond.else_branch {
                    collect_maps_in_else(else_branch, maps);
                }
            }
            Statement::Expr(expr, _) => {
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
        Preamble::CDef(_) | Preamble::Macro(_) | Preamble::Config(_) | Preamble::Error(_) => {}
    }
}

fn collect_vars_in_block(block: &Block, offset: usize, vars: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        if stmt.span().start() > offset {
            break;
        }
        match stmt {
            Statement::Assignment(assign, _) => {
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
                    Loop::Unroll(u) => {
                        if u.block.span().start() <= offset && offset < u.block.span().end() {
                            collect_vars_in_block(&u.block, offset, vars);
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
            Statement::Expr(expr, _) => {
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

#[derive(Clone)]
struct ScopeTracker {
    vars: Vec<(String, IdentKind)>,
}

impl ScopeTracker {
    fn new() -> Self {
        Self { vars: Vec::new() }
    }

    fn define(&mut self, name: &str, kind: IdentKind) {
        self.vars.push((name.to_string(), kind));
    }

    fn is_defined(&self, name: &str, kind: IdentKind) -> bool {
        self.vars.iter().any(|(n, k)| n == name && *k == kind)
    }
}

struct ErrorChecker<'a> {
    global_maps: &'a [VarInfo],
    defines: &'a [VarInfo],
    macros: &'a HashMap<String, &'a MacroDefinition<'a>>,
    struct_defs: &'a HashMap<String, StructInfo>,
    var_types: &'a HashMap<String, TypeInfo>,
    line_index: &'a OwnedLineIndex,
    out: Vec<Diagnostic>,
}

impl<'a> ErrorChecker<'a> {
    fn check_program(&mut self, program: &'a Program<'a>) {
        for preamble in &program.preambles {
            match preamble {
                Preamble::Probe(probe) => self.check_probe(probe),
                Preamble::CDef(cdef) => {
                    if let CDef::Struct(def) = cdef.as_ref() {
                        for field in &def.fields {
                            if !field.type_name.validate(self.struct_defs) {
                                self.push_span(
                                    field.span,
                                    DiagnosticSeverity::ERROR,
                                    format!(
                                        "Unsupported field type \"{}\"",
                                        field.type_name.text()
                                    ),
                                );
                            }
                        }
                    }
                }
                Preamble::Macro(mac) => self.check_macro(mac),
                Preamble::Config(_) => {}
                Preamble::Error(e) => {
                    self.push_span(e.span(), DiagnosticSeverity::ERROR, e.diagnosis())
                }
            }
        }
    }

    fn push_span(&mut self, span: Span, severity: DiagnosticSeverity, message: String) {
        self.out.push(Diagnostic {
            range: self.line_index.range(span),
            severity: Some(severity),
            message,
            ..Default::default()
        });
    }

    fn emit_diag(&mut self, stmt: &Statement) {
        if let Statement::Error(e) = stmt {
            self.push_span(e.span(), DiagnosticSeverity::ERROR, e.diagnosis());
        }
    }

    fn check_semicolon(&mut self, stmt: &Statement) {
        if !stmt.has_semicolon() {
            self.push_span(
                stmt.span(),
                DiagnosticSeverity::ERROR,
                "Expected ';' after statement".to_string(),
            );
        }
    }

    fn check_probe(&mut self, probe: &Probe) {
        let mut scope = ScopeTracker::new();
        if let Some(cond) = &probe.condition {
            self.check_expr(cond, &scope);
        }
        self.check_block(&probe.block, &mut scope);
    }

    fn check_macro(&mut self, r#macro: &'a MacroDefinition<'a>) {
        for param in &r#macro.params {
            if let MacroParam::Map {
                keyed: true, span, ..
            } = param
            {
                self.push_span(
                    *span,
                    DiagnosticSeverity::ERROR,
                    "Map access is not allowed".to_string(),
                );
            }
        }

        let mut scope = ScopeTracker::new();
        for param in &r#macro.params {
            let name = param.name();
            scope.define(name.name, name.kind);
        }
        self.check_block(&r#macro.body, &mut scope);
    }

    fn check_block(&mut self, block: &Block, scope: &mut ScopeTracker) {
        for stmt in &block.statements {
            self.emit_diag(stmt);
            self.check_semicolon(stmt);
            match stmt {
                Statement::Assignment(assign, _) => {
                    self.check_expr(&assign.rvalue, scope);
                    match &assign.lvalue {
                        Lvalue::Identifier(ident) if ident.kind != IdentKind::Map => {
                            scope.define(ident.name, ident.kind);
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
                                scope.define(ident.name, ident.kind);
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
                    Loop::Unroll(u) => {
                        let mut inner = scope.clone();
                        self.check_block(&u.block, &mut inner);
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
                Statement::Expr(expr, _) => {
                    self.check_expr(expr, scope);
                }
                Statement::Error(_) => {}
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, scope: &ScopeTracker) {
        match expr {
            Expr::Identifier(ident) => match ident.kind {
                IdentKind::Bare => {
                    if !scope.is_defined(ident.name, IdentKind::Bare)
                        && !BUILTINS.keywords.iter().any(|k| k.name == ident.name)
                        && !self.defines.iter().any(|d| d.name == ident.name)
                    {
                        self.emit_diag(&UndefinedIdent::new(ident.name, ident.span));
                    }
                }
                IdentKind::Scratch => {
                    if !scope.is_defined(ident.name, IdentKind::Scratch) {
                        self.emit_diag(&UndefinedIdent::new(ident.name, ident.span));
                    }
                }
                IdentKind::Map => {
                    if !scope.is_defined(ident.name, IdentKind::Map)
                        && !self
                            .global_maps
                            .iter()
                            .any(|m| m.name == format!("@{}", ident.name))
                    {
                        self.push_span(
                            ident.span,
                            DiagnosticSeverity::WARNING,
                            format!("Undefined map \"@{}\"", ident.name),
                        );
                    }
                }
            },
            Expr::Call(call) => {
                if let Some(mac) = self.macros.get(call.func.name) {
                    self.check_macro_call(mac, call.args.as_slice(), call.span());
                } else if !BUILTINS.functions.iter().any(|f| f.name == call.func.name) {
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
                    && !scope.is_defined(access.map.name, IdentKind::Map)
                    && !self
                        .global_maps
                        .iter()
                        .any(|m| m.name == format!("@{}", access.map.name))
                {
                    self.push_span(
                        access.map.span,
                        DiagnosticSeverity::WARNING,
                        format!("Undefined map \"@{}\"", access.map.name),
                    );
                }
                for key in &access.keys {
                    self.check_expr(key, scope);
                }
            }
            Expr::Integer(_) | Expr::String(_) | Expr::ArgN(_) => {}
            Expr::Cast(cast) => {
                self.check_expr(&cast.expr, scope);
            }
            Expr::Field(field) => {
                self.check_expr(&field.base, scope);
                let Some(member) = &field.field else {
                    return;
                };
                let Some(base_type) =
                    resolve_expr_type(&field.base, self.struct_defs, self.var_types)
                else {
                    return;
                };
                let TypeInfo::StructLike { name, .. } = &base_type else {
                    self.push_span(
                        member.span,
                        DiagnosticSeverity::ERROR,
                        format!("Field access is not supported on \"{}\"", base_type.name()),
                    );
                    return;
                };
                let Some(info) = self.struct_defs.get(name) else {
                    return;
                };
                if !info.fields.iter().any(|decl| decl.name == member.name) {
                    self.push_span(
                        member.span,
                        DiagnosticSeverity::ERROR,
                        format!("\"{}\" has no field \"{}\"", base_type.name(), member.name),
                    );
                }
            }
        }
    }

    fn check_macro_call(&mut self, r#macro: &MacroDefinition, args: &[Expr], span: Span) {
        if r#macro.params.len() != args.len() {
            self.push_span(
                span,
                DiagnosticSeverity::ERROR,
                format!(
                    "Macro \"{}\" expects {} args but got {}",
                    r#macro.name.name,
                    r#macro.params.len(),
                    args.len()
                ),
            );
            return;
        }

        for (param, arg) in r#macro.params.iter().zip(args.iter()) {
            let arg_matches = match (param.kind(), arg) {
                (MacroParamKind::Expr, _) => true,
                (MacroParamKind::Scratch, Expr::Identifier(ident)) => {
                    ident.kind == IdentKind::Scratch
                }
                (MacroParamKind::Map, Expr::Identifier(ident)) => ident.kind == IdentKind::Map,
                (MacroParamKind::Map, Expr::MapAccess(access)) => access.map.kind == IdentKind::Map,
                _ => false,
            };

            if !arg_matches {
                self.push_span(
                    arg.span(),
                    DiagnosticSeverity::ERROR,
                    format!(
                        "Macro \"{}\" parameter \"{}\" expects {}",
                        r#macro.name.name,
                        param.name().name,
                        param.kind()
                    ),
                );
            }
        }
    }

    fn check_else(&mut self, else_branch: &Else, scope: &ScopeTracker) {
        match else_branch {
            Else::IfCond(if_cond) => {
                self.check_expr(&if_cond.condition, scope);
                let mut inner = scope.clone();
                self.check_block(&if_cond.block, &mut inner);
                if let Some(next_else) = &if_cond.else_branch {
                    self.check_else(next_else, scope);
                }
            }
            Else::Block(block) => {
                let mut inner = scope.clone();
                self.check_block(block, &mut inner);
            }
        }
    }

    fn into_diagnostics(mut self, program: &'a Program<'a>) -> Vec<Diagnostic> {
        self.check_program(program);
        self.out
    }
}

pub struct SemanticAnalyzer;

pub struct AnalyzedFile {
    pub document: Arc<Document>,
    pub struct_defs: HashMap<String, StructInfo>,
    pub var_types: HashMap<String, TypeInfo>,
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

    pub async fn analyze(&self, context: &Context, path: &Path) -> Result<AnalyzedFile> {
        let document = context.storage.lock().await.read(path);
        let source = document.data.clone();
        let line_index = document.line_index.clone();
        let ast_cell = OwnedAst::new(source)?;

        let program = ast_cell.program();
        let struct_defs = collect_structs(program);
        let var_types = collect_var_types(program);
        let diagnostics = compute_diagnostics(program, &line_index, &struct_defs, &var_types);

        Ok(AnalyzedFile {
            document,
            ast_cell,
            struct_defs,
            var_types,
            diagnostics,
        })
    }
}

fn compute_diagnostics(
    program: &Program,
    line_index: &OwnedLineIndex,
    struct_defs: &HashMap<String, StructInfo>,
    var_types: &HashMap<String, TypeInfo>,
) -> Vec<Diagnostic> {
    let global_maps = merge_vars(collect_global_maps(program));
    let defines = merge_vars(collect_defines(program));
    let macros = collect_macros(program);

    ErrorChecker {
        global_maps: &global_maps,
        defines: &defines,
        macros: &macros,
        struct_defs,
        var_types,
        line_index,
        out: Vec::new(),
    }
    .into_diagnostics(program)
}
