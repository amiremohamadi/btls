use super::analyzer::semantic_analyzer::{self, AnalyzedFile, VarInfo};
use super::parser::{Expr, IdentKind, Identifier, Lvalue, Node, Program, Statement, Walk};
use super::server::Context;
use pest::Span;
use std::path::Path;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, ReferenceParams, Url,
};

pub async fn goto_definition(
    context: &Context,
    path: &Path,
    position: Position,
) -> Result<Option<GotoDefinitionResponse>> {
    let analyzed = analyze_document(context, path).await?;
    let Some(offset) = analyzed.document.line_index.offset(position) else {
        return Ok(None);
    };

    let line_index = &analyzed.document.line_index;
    let uri = Url::from_file_path(path).map_err(|_| Error::new(ErrorCode::InternalError))?;

    if let Some(def_span) = resolve_macro_def(analyzed.ast(), offset) {
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: line_index.range(def_span),
        })));
    }

    let Some(variable) = resolve_variable(&analyzed, offset) else {
        return Ok(None);
    };
    let Some(definition) = variable.locs.first() else {
        return Ok(None);
    };

    let def_range = Range {
        start: line_index.position(definition.offset),
        end: line_index.position(definition.offset + definition.len),
    };

    Ok(Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: def_range,
    })))
}

pub async fn references(
    context: &Context,
    path: &Path,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let analyzed = analyze_document(context, path).await?;
    let Some(offset) = analyzed
        .document
        .line_index
        .offset(params.text_document_position.position)
    else {
        return Ok(None);
    };

    let Some(variable) = resolve_variable(&analyzed, offset) else {
        return Ok(None);
    };

    let line_index = &analyzed.document.line_index;
    let uri = Url::from_file_path(path).map_err(|_| Error::new(ErrorCode::InternalError))?;

    let include_decl = params.context.include_declaration;
    let locs: Vec<Location> = variable
        .locs
        .iter()
        .enumerate()
        .filter(|(i, _)| include_decl || *i > 0)
        .map(|(_, loc)| {
            let range = Range {
                start: line_index.position(loc.offset),
                end: line_index.position(loc.offset + loc.len),
            };
            Location {
                uri: uri.clone(),
                range,
            }
        })
        .collect();

    if locs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(locs))
    }
}

async fn analyze_document<'a>(context: &Context, path: &Path) -> Result<AnalyzedFile> {
    context
        .analyzer
        .lock()
        .await
        .analyze(context, path)
        .await
        .map_err(|_| Error::new(ErrorCode::InternalError))
}

fn resolve_variable<'a>(file: &'a AnalyzedFile, offset: usize) -> Option<VarInfo> {
    let ident = identifier_at_offset(file.ast(), offset)?;
    if ident.kind == IdentKind::Bare {
        return None;
    }

    semantic_analyzer::variables_at(file, offset)
        .into_iter()
        .find(|variable| variable.name == ident.prefixed_name())
}

fn resolve_macro_def<'a>(program: &'a Program<'a>, offset: usize) -> Option<Span<'a>> {
    let ident = identifier_at_offset(program, offset)?;
    if ident.kind != IdentKind::Bare {
        return None;
    }
    semantic_analyzer::collect_macros(program)
        .get(ident.name)
        .map(|r#macro| r#macro.name.span)
}

fn identifier_at_offset<'a>(program: &'a Program<'a>, offset: usize) -> Option<&'a Identifier<'a>> {
    for node in Walk::new(program.as_node()) {
        let span = node.span();
        let check_start = span.start().saturating_sub(1);
        if check_start > offset || offset >= span.end() {
            continue;
        }

        if let Some(expr) = node.as_expr() {
            match expr {
                Expr::Identifier(ident) => {
                    if let Some(ident) = ident_at(ident, offset) {
                        return Some(ident);
                    }
                }
                Expr::MapAccess(access) => {
                    if let Some(ident) = ident_at(&access.map, offset) {
                        return Some(ident);
                    }
                }
                Expr::Call(call) => {
                    if let Some(ident) = ident_at(&call.func, offset) {
                        return Some(ident);
                    }
                }
                _ => {}
            }
        }

        if let Some(stmt) = node.as_statement() {
            if let Statement::Assignment(assign, _) = stmt {
                match &assign.lvalue {
                    Lvalue::Identifier(ident) => {
                        if let Some(ident) = ident_at(ident, offset) {
                            return Some(ident);
                        }
                    }
                    Lvalue::MapAccess(access) => {
                        if let Some(ident) = ident_at(&access.map, offset) {
                            return Some(ident);
                        }
                    }
                }
            }
        }
    }

    None
}

fn ident_at<'a>(ident: &'a Identifier<'a>, offset: usize) -> Option<&'a Identifier<'a>> {
    let span = ident_span(ident);
    (span.start() <= offset && offset < span.end()).then_some(ident)
}

fn ident_span<'a>(ident: &'a Identifier<'a>) -> Span<'a> {
    let span = ident.span;
    match ident.kind {
        IdentKind::Scratch | IdentKind::Map => {
            let start = span.start().saturating_sub(1);
            Span::new(span.get_input(), start, span.end()).unwrap_or(span)
        }
        IdentKind::Bare => span,
    }
}
