use std::fmt;
use std::sync::Arc;

use super::OwnedAnalyzedProgram;
use super::analyzed::{
    AnalyzedBlock, AnalyzedElseBranch, AnalyzedPreamble, AnalyzedProgram, AnalyzedStatement,
};
use crate::btf::{self, BtfScope};
use crate::builtins::BUILTINS;
use crate::common::utils::OwnedLineIndex;
use crate::parser::{
    Block, CDef, Else, Expr, FieldMember, IdentKind, Loop, Lvalue, MacroDefinition, MacroParam,
    MacroParamKind, Node, Preamble, Probe, Program, Statement, TypeKind, TypeName, UnaryOp,
    UndefinedFunc, UndefinedIdent,
};
use crate::server::Context;
use crate::storage::Document;
use anyhow::Result;
use pest::Span;
use std::collections::HashMap;
use std::path::Path;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

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
    pub len: usize,
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
                        len: define.span.as_str().len(),
                    },
                ));
            }
        }
    }
    defines
}

pub fn collect_macros<'a>(program: &'a Program<'a>) -> HashMap<String, &'a MacroDefinition<'a>> {
    let mut macros = HashMap::new();
    for preamble in &program.preambles {
        if let Preamble::Macro(r#macro) = preamble {
            macros.insert(r#macro.name.name.to_string(), r#macro.as_ref());
        }
    }
    macros
}

pub fn macros_at<'a>(
    analyzed: &'a AnalyzedProgram<'a>,
    offset: usize,
) -> Vec<&'a MacroDefinition<'a>> {
    analyzed
        .preambles
        .iter()
        .filter_map(|preamble| match preamble {
            AnalyzedPreamble::Macro(m, _) if m.span.start() <= offset => Some(*m),
            _ => None,
        })
        .collect()
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

#[derive(Clone, Default)]
pub struct StructScope<'a> {
    layers: Vec<&'a HashMap<String, StructInfo>>,
}

impl<'a> StructScope<'a> {
    pub fn new(base: &'a HashMap<String, StructInfo>) -> Self {
        Self { layers: vec![base] }
    }

    pub fn with(&self, layer: &'a HashMap<String, StructInfo>) -> Self {
        let mut layers = Vec::with_capacity(self.layers.len() + 1);
        layers.push(layer);
        layers.extend_from_slice(&self.layers);
        Self { layers }
    }

    pub fn get(&self, name: &str) -> Option<&'a StructInfo> {
        self.layers.iter().find_map(|layer| layer.get(name))
    }
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

fn probe_args_layer(
    probe: &Probe,
    btf_scopes: &HashMap<String, Arc<BtfScope>>,
) -> HashMap<String, StructInfo> {
    let mut layer = HashMap::new();
    if let Some(scope) = btf::probe_func(&probe.attach_points).and_then(|func| btf_scopes.get(func))
    {
        layer.insert("args".to_string(), scope.args.clone());
    }
    layer
}

fn block_var_types(block: &AnalyzedBlock, structs: &StructScope) -> HashMap<String, TypeInfo> {
    let mut types = HashMap::new();
    collect_var_types_in_block(block, structs, &mut types);
    types
}

pub fn var_types_at(
    program: &AnalyzedProgram,
    offset: usize,
    base: &HashMap<String, StructInfo>,
    btf_scopes: &HashMap<String, Arc<BtfScope>>,
) -> HashMap<String, TypeInfo> {
    let scope = StructScope::new(base);
    for preamble in &program.preambles {
        if !(preamble.span().start() <= offset && offset < preamble.span().end()) {
            continue;
        }
        match preamble {
            AnalyzedPreamble::Probe(probe, block) => {
                let args = probe_args_layer(probe, btf_scopes);
                return block_var_types(block, &scope.with(&args));
            }
            AnalyzedPreamble::Macro(_, block) => {
                return block_var_types(block, &scope);
            }
            _ => {}
        }
    }
    HashMap::new()
}

fn collect_var_types_in_block(
    block: &AnalyzedBlock,
    structs: &StructScope,
    types: &mut HashMap<String, TypeInfo>,
) {
    for stmt in &block.statements {
        match stmt {
            AnalyzedStatement::Assignment(a) => {
                if let Lvalue::Identifier(ident) = &a.assignment.lvalue {
                    if ident.kind == IdentKind::Scratch {
                        if let Some(type_info) =
                            resolve_expr_type(&a.assignment.rvalue, structs, types)
                        {
                            types.insert(format!("${}", ident.name), type_info);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for stmt in &block.statements {
        for scope in stmt.subscopes() {
            collect_var_types_in_block(scope, structs, types);
        }
    }
}

pub fn resolve_expr_type(
    expr: &Expr,
    structs: &StructScope,
    var_types: &HashMap<String, TypeInfo>,
) -> Option<TypeInfo> {
    match expr {
        Expr::Cast(cast) => Some(TypeInfo::from(&cast.type_name)),
        Expr::Identifier(ident) if ident.kind == IdentKind::Scratch => {
            var_types.get(&format!("${}", ident.name)).cloned()
        }
        Expr::Identifier(ident) if ident.kind == IdentKind::Bare && ident.name == "args" => {
            Some(TypeInfo::StructLike {
                kind: TypeKind::Struct,
                name: "args".to_string(),
                pointers: 0,
            })
        }
        Expr::UnaryExpr(u) if u.op == UnaryOp::Deref => {
            resolve_expr_type(&u.expr, structs, var_types)
        }
        Expr::Field(fa) => {
            let base_type = resolve_expr_type(&fa.base, structs, var_types)?;
            let field = fa.field.as_ref()?;
            let field_name = match field {
                FieldMember::Name(ident) => ident.name,
                FieldMember::Index(idx, _) => &idx.to_string(),
            };
            let TypeInfo::StructLike { name, .. } = &base_type else {
                return None;
            };
            structs
                .get(name)?
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .map(|f| f.type_name.clone())
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
                        len: assign.span.as_str().len(),
                    },
                ));
            }
            Statement::Loop(loop_stmt) => match &**loop_stmt {
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
                            len: loop_stmt.span().as_str().len(),
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
                                len: unary.span.as_str().len(),
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

pub fn variables_at(file: &AnalyzedFile, offset: usize) -> Vec<VarInfo> {
    let mut raw = Vec::new();
    for preamble in &file.analyzed().preambles {
        let span = preamble.span();
        if span.start() > offset {
            break;
        }
        if span.start() <= offset && offset < span.end() {
            collect_vars_in_preamble(preamble, offset, &mut raw);
        }
    }
    raw.extend(collect_global_maps(file.ast()));
    raw.extend(collect_defines(file.ast()));
    merge_vars(raw)
}

fn collect_vars_in_preamble(preamble: &AnalyzedPreamble, offset: usize, vars: &mut Vec<RawVar>) {
    match preamble {
        AnalyzedPreamble::Probe(_, block) => {
            collect_vars_in_block(block, offset, vars);
        }
        _ => {}
    }
}

fn collect_vars_in_block(block: &AnalyzedBlock, offset: usize, vars: &mut Vec<RawVar>) {
    for stmt in &block.statements {
        if stmt.span().start() > offset {
            break;
        }
        match stmt {
            AnalyzedStatement::Assignment(a) => {
                if let Some(ident) = match &a.assignment.lvalue {
                    Lvalue::Identifier(ident) if ident.kind != IdentKind::Map => Some(ident),
                    _ => None,
                } {
                    vars.push((
                        ident.prefixed_name(),
                        VarLoc {
                            offset: a.assignment.span.start(),
                            len: a.assignment.span.as_str().len(),
                        },
                    ));
                }
            }
            AnalyzedStatement::Loop(l) => {
                if let Some(ident) = l.variable {
                    if ident.kind != IdentKind::Map {
                        vars.push((
                            ident.prefixed_name(),
                            VarLoc {
                                offset: l.loop_stmt.span().start(),
                                len: l.loop_stmt.span().as_str().len(),
                            },
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    for stmt in &block.statements {
        for scope in stmt.subscopes() {
            if scope.span.start() <= offset && offset < scope.span.end() {
                collect_vars_in_block(scope, offset, vars);
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
    btf_scopes: &'a HashMap<String, Arc<BtfScope>>,
    line_index: &'a OwnedLineIndex,
    out: Vec<Diagnostic>,
}

impl<'a> ErrorChecker<'a> {
    fn check_program(&mut self, analyzed: &'a AnalyzedProgram<'a>) {
        for preamble in &analyzed.preambles {
            match preamble {
                AnalyzedPreamble::Probe(probe, block) => self.check_probe(probe, block),
                AnalyzedPreamble::CDef(cdef) => {
                    if let CDef::Struct(def) = cdef {
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
                AnalyzedPreamble::Macro(m, block) => self.check_macro(m, block),
                AnalyzedPreamble::Config(_) => {}
                AnalyzedPreamble::Error(e) => {
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

    fn check_probe(&mut self, probe: &Probe, block: &AnalyzedBlock) {
        let args = probe_args_layer(probe, self.btf_scopes);
        let structs = StructScope::new(self.struct_defs).with(&args);
        let var_types = block_var_types(block, &structs);
        let mut scope = ScopeTracker::new();
        scope.define("args", IdentKind::Bare);
        if let Some(cond) = &probe.condition {
            self.check_expr(cond, &scope, &structs, &var_types);
        }
        self.check_analyzed_block(block, &mut scope, &structs, &var_types);
    }

    fn check_macro(&mut self, r#macro: &'a MacroDefinition<'a>, block: &AnalyzedBlock) {
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

        let structs = StructScope::new(self.struct_defs);
        let var_types = block_var_types(block, &structs);
        self.check_analyzed_block(block, &mut scope, &structs, &var_types);
    }

    fn check_analyzed_block(
        &mut self,
        block: &AnalyzedBlock,
        scope: &mut ScopeTracker,
        structs: &StructScope,
        var_types: &HashMap<String, TypeInfo>,
    ) {
        let mut statements = block.statements.iter().peekable();
        while let Some(stmt) = statements.next() {
            // hacky approach to make it optional for the last statement
            if statements.peek().is_some() {
                if !stmt.has_semicolon() {
                    self.push_span(
                        stmt.span(),
                        DiagnosticSeverity::ERROR,
                        "Expected ';' after statement".to_string(),
                    );
                }
            }
            match stmt {
                AnalyzedStatement::Assignment(a) => {
                    self.check_expr(&a.assignment.rvalue, scope, structs, var_types);
                    match &a.assignment.lvalue {
                        Lvalue::Identifier(ident) if ident.kind != IdentKind::Map => {
                            scope.define(ident.name, ident.kind);
                        }
                        Lvalue::MapAccess(access) => {
                            for key in &access.keys {
                                self.check_expr(key, scope, structs, var_types);
                            }
                        }
                        _ => {}
                    }
                }
                AnalyzedStatement::Loop(l) => {
                    match l.loop_stmt {
                        Loop::For(for_loop) => {
                            self.check_expr(&for_loop.rhs, scope, structs, var_types);
                            if let Some(ident) = l.variable {
                                if ident.kind != IdentKind::Map {
                                    scope.define(ident.name, ident.kind);
                                }
                            }
                        }
                        Loop::While(w) => {
                            self.check_expr(&w.condition, scope, structs, var_types);
                        }
                        Loop::Unroll(_) => {}
                    }
                    let mut inner = scope.clone();
                    self.check_analyzed_block(&l.body_block, &mut inner, structs, var_types);
                }
                AnalyzedStatement::IfCond(cond) => {
                    self.check_expr(&cond.if_cond.condition, scope, structs, var_types);
                    let mut inner = scope.clone();
                    self.check_analyzed_block(&cond.then_block, &mut inner, structs, var_types);
                    if let Some(else_branch) = &cond.else_branch {
                        self.check_analyzed_else(else_branch, scope, structs, var_types);
                    }
                }
                AnalyzedStatement::Expr(e) => {
                    self.check_expr(e.expr, scope, structs, var_types);
                }
                AnalyzedStatement::Error(e) => {
                    self.push_span(e.span(), DiagnosticSeverity::ERROR, e.diagnosis());
                }
            }
        }
    }

    fn check_analyzed_else(
        &mut self,
        else_branch: &AnalyzedElseBranch,
        scope: &ScopeTracker,
        structs: &StructScope,
        var_types: &HashMap<String, TypeInfo>,
    ) {
        match else_branch {
            AnalyzedElseBranch::IfCond(cond) => {
                self.check_expr(&cond.if_cond.condition, scope, structs, var_types);
                let mut inner = scope.clone();
                self.check_analyzed_block(&cond.then_block, &mut inner, structs, var_types);
                if let Some(next_else) = &cond.else_branch {
                    self.check_analyzed_else(next_else, scope, structs, var_types);
                }
            }
            AnalyzedElseBranch::Block(block) => {
                let mut inner = scope.clone();
                self.check_analyzed_block(block, &mut inner, structs, var_types);
            }
        }
    }

    fn check_expr(
        &mut self,
        expr: &Expr,
        scope: &ScopeTracker,
        structs: &StructScope,
        var_types: &HashMap<String, TypeInfo>,
    ) {
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
                if let Some(r#macro) = self.macros.get(call.func.name) {
                    self.check_macro_call(r#macro, call.args.as_slice(), call.span());
                } else if !BUILTINS.functions.iter().any(|f| f.name == call.func.name) {
                    self.emit_diag(&UndefinedFunc::new(call.func.name, call.span()));
                }
                for arg in &call.args {
                    self.check_expr(arg, scope, structs, var_types);
                }
            }
            Expr::BinaryExpr(bin) => {
                self.check_expr(&bin.lhs, scope, structs, var_types);
                self.check_expr(&bin.rhs, scope, structs, var_types);
            }
            Expr::UnaryExpr(unary) => {
                self.check_expr(&unary.expr, scope, structs, var_types);
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
                    self.check_expr(key, scope, structs, var_types);
                }
            }
            Expr::Integer(_) | Expr::String(_) | Expr::ArgN(_) => {}
            Expr::Cast(cast) => {
                self.check_expr(&cast.expr, scope, structs, var_types);
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elements {
                    self.check_expr(element, scope, structs, var_types);
                }
            }
            Expr::Field(field) => {
                self.check_expr(&field.base, scope, structs, var_types);
                let Some(member) = &field.field else {
                    return;
                };
                let Some(base_type) = resolve_expr_type(&field.base, structs, var_types) else {
                    return;
                };
                let TypeInfo::StructLike { name, .. } = &base_type else {
                    self.push_span(
                        member.span(),
                        DiagnosticSeverity::ERROR,
                        format!("Field access is not supported on \"{}\"", base_type.name()),
                    );
                    return;
                };
                let Some(info) = structs.get(name) else {
                    return;
                };
                let member_name = match member {
                    FieldMember::Name(ident) => ident.name,
                    FieldMember::Index(idx, _) => &idx.to_string(),
                };
                let has_field = info.fields.iter().any(|decl| decl.name == member_name);
                if !has_field {
                    self.push_span(
                        member.span(),
                        DiagnosticSeverity::ERROR,
                        format!("\"{}\" has no field \"{}\"", base_type.name(), member_name),
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

    fn into_diagnostics(mut self, analyzed: &'a AnalyzedProgram<'a>) -> Vec<Diagnostic> {
        self.check_program(analyzed);
        self.out
    }
}

pub struct SemanticAnalyzer;

pub struct AnalyzedFile {
    pub document: Arc<Document>,
    pub struct_defs: HashMap<String, StructInfo>,
    pub btf_scopes: HashMap<String, Arc<BtfScope>>,
    diagnostics: Vec<Diagnostic>,
    ast_cell: super::OwnedAst,
    analyzed_program: OwnedAnalyzedProgram,
}

impl AnalyzedFile {
    pub fn ast(&self) -> &Program<'_> {
        self.ast_cell.program()
    }

    pub fn analyzed(&self) -> &AnalyzedProgram<'_> {
        self.analyzed_program.get()
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
        let ast_cell = super::OwnedAst::new(source)?;
        let analyzed_program = OwnedAnalyzedProgram::new(ast_cell.clone());

        let program = ast_cell.program();
        let mut struct_defs = collect_structs(program);
        let btf_scopes = resolve_btf_scopes(context, program, &mut struct_defs);
        let diagnostics = compute_diagnostics(
            &analyzed_program,
            program,
            &line_index,
            &struct_defs,
            &btf_scopes,
        );

        Ok(AnalyzedFile {
            document,
            ast_cell,
            analyzed_program,
            struct_defs,
            btf_scopes,
            diagnostics,
        })
    }
}

fn resolve_btf_scopes(
    context: &Context,
    program: &Program,
    struct_defs: &mut HashMap<String, StructInfo>,
) -> HashMap<String, Arc<BtfScope>> {
    let mut scopes = HashMap::new();
    for preamble in &program.preambles {
        let Preamble::Probe(probe) = preamble else {
            continue;
        };
        let Some(func) = btf::probe_func(&probe.attach_points) else {
            continue;
        };
        if scopes.contains_key(func) {
            continue;
        }
        let Some(scope) = context.btf.probe_scope(func) else {
            continue;
        };
        for (name, info) in &scope.structs {
            struct_defs
                .entry(name.clone())
                .or_insert_with(|| info.clone());
        }
        scopes.insert(func.to_string(), scope);
    }
    scopes
}

fn compute_diagnostics(
    analyzed_program: &OwnedAnalyzedProgram,
    program: &Program,
    line_index: &OwnedLineIndex,
    struct_defs: &HashMap<String, StructInfo>,
    btf_scopes: &HashMap<String, Arc<BtfScope>>,
) -> Vec<Diagnostic> {
    let global_maps = merge_vars(collect_global_maps(program));
    let defines = merge_vars(collect_defines(program));
    let macros = collect_macros(program);

    ErrorChecker {
        global_maps: &global_maps,
        defines: &defines,
        macros: &macros,
        struct_defs,
        btf_scopes,
        line_index,
        out: Vec::new(),
    }
    .into_diagnostics(analyzed_program.get())
}
