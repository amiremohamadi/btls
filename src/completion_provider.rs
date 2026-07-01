use super::analyzer::semantic_analyzer;
use super::analyzer::semantic_analyzer::{StructInfo, TypeInfo};
use super::builtins::{BUILTINS, DATA_TYPES, SYNTAX_KEYWORDS};
use super::parser::{Config, Expr, FieldAccess, Node, Program, Walk};
use super::server::Context;
use std::collections::HashMap;
use std::path::Path;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, MarkupContent,
    MarkupKind, Position, Url,
};

macro_rules! builtin_to_completion_item {
    ($collection:expr, $kind:expr) => {
        $collection.iter().map(|x| CompletionItem {
            label: x.name.to_string(),
            kind: Some($kind),
            detail: Some(x.detail.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: x.documentation.to_string(),
            })),
            ..Default::default()
        })
    };
}

pub async fn completion(
    context: &Context,
    path: &Path,
    position: Position,
) -> Result<Option<CompletionResponse>> {
    let analyzer = context.analyzer.lock().await;
    let analyzed = analyzer
        .analyze(context, path)
        .await
        .map_err(|_| Error::new(ErrorCode::InternalError))?;

    let Some(offset) = analyzed.document.line_index.offset(position) else {
        return Ok(None);
    };

    if let Some(config) = cfg_block_at(analyzed.ast(), offset) {
        let items = cfg_completion(config, offset);
        return Ok(Some(CompletionResponse::Array(items)));
    }

    if let Some(field) = field_access_at(analyzed.ast(), offset) {
        let items = field_completion(field, &analyzed.struct_defs, &analyzed.var_types);
        return Ok(Some(CompletionResponse::Array(items)));
    }

    let file_uri =
        Url::from_file_path(path).unwrap_or_else(|_| Url::parse("file:///unknown").unwrap());
    let li = &analyzed.document.line_index;
    let variables = semantic_analyzer::variables_at(analyzed.ast(), offset)
        .into_iter()
        .map(|v| CompletionItem {
            label: v.name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(v.summary(li)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: v.documentation(li, &file_uri),
            })),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    let builtin_keywords =
        builtin_to_completion_item!(BUILTINS.keywords, CompletionItemKind::KEYWORD);
    let syntax_keywords = builtin_to_completion_item!(SYNTAX_KEYWORDS, CompletionItemKind::KEYWORD);
    let data_types = builtin_to_completion_item!(DATA_TYPES, CompletionItemKind::KEYWORD);
    let builtin_funcs =
        builtin_to_completion_item!(BUILTINS.functions, CompletionItemKind::FUNCTION);

    Ok(Some(CompletionResponse::Array(
        variables
            .into_iter()
            .chain(builtin_keywords)
            .chain(syntax_keywords)
            .chain(data_types)
            .chain(builtin_funcs)
            .collect(),
    )))
}

enum ConfigContext {
    Key,
    Value(String),
}

fn cfg_block_at<'a>(program: &'a Program<'a>, offset: usize) -> Option<&'a Config<'a>> {
    let parents: Vec<_> = Walk::new(program.as_node())
        .filter(|node| {
            let span = node.span();
            span.start() <= offset && offset < span.end()
        })
        .collect();

    for node in parents.into_iter().rev() {
        if let Some(config) = node.as_config() {
            return Some(config);
        }
    }
    None
}

fn field_access_at<'a>(program: &'a Program<'a>, offset: usize) -> Option<&'a FieldAccess<'a>> {
    let mut innermost: Option<&'a FieldAccess<'a>> = None;
    for node in Walk::new(program.as_node()) {
        if let Some(Expr::Field(field)) = node.as_expr() {
            let base_end = field.base.span().end();
            if base_end < offset
                && offset <= field.span.end()
                && innermost.is_none_or(|current| base_end > current.base.span().end())
            {
                innermost = Some(field);
            }
        }
    }
    innermost
}

fn field_completion(
    field: &FieldAccess,
    structs: &HashMap<String, StructInfo>,
    var_types: &HashMap<String, TypeInfo>,
) -> Vec<CompletionItem> {
    let Some(base_type) = semantic_analyzer::resolve_expr_type(&field.base, structs, var_types)
    else {
        // unknown base type
        return Vec::new();
    };
    let TypeInfo::StructLike { name, .. } = &base_type else {
        return Vec::new();
    };
    let Some(info) = structs.get(name) else {
        return Vec::new();
    };
    info.fields
        .iter()
        .map(|f| CompletionItem {
            label: f.name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(f.type_name.to_string()),
            ..Default::default()
        })
        .collect()
}

fn cfg_context(config: &Config, offset: usize) -> ConfigContext {
    for assign in &config.assignments {
        let span = assign.span;
        if span.start() <= offset && offset < span.end() {
            let key_end = span.start() + assign.key.len();
            return if offset > key_end {
                ConfigContext::Value(assign.key.to_string())
            } else {
                ConfigContext::Key
            };
        }
        // TODO: any better approaches?
        // right now we walk the AST to check if the cursor is right after '='
        if offset == span.end() && assign.value.is_none() {
            return ConfigContext::Value(assign.key.to_string());
        }
    }
    ConfigContext::Key
}

fn cfg_completion(config: &Config, offset: usize) -> Vec<CompletionItem> {
    match cfg_context(config, offset) {
        ConfigContext::Key => BUILTINS
            .config_vars
            .iter()
            .map(|v| CompletionItem {
                label: v.name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(v.detail.to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: v.documentation.to_string(),
                })),
                ..Default::default()
            })
            .collect(),
        ConfigContext::Value(key) => BUILTINS
            .config_vars
            .iter()
            .find(|v| v.name == key)
            .map(|v| {
                v.values
                    .iter()
                    .map(|val| CompletionItem {
                        label: val.to_string(),
                        kind: Some(CompletionItemKind::VALUE),
                        detail: Some(v.name.to_string()),
                        ..Default::default()
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{self, ast};

    #[test]
    fn test_smoke() {
        let src = "config = {\n stack_mode =  }";
        let program = ast::parse(src).unwrap();

        let parser::Preamble::Config(c) = &program.preambles[0] else {
            panic!("not a config block");
        };

        let cursor = src.find('{').unwrap() + 1;
        let items = cfg_completion(c, cursor);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(labels.contains(&"stack_mode"));
        assert!(labels.contains(&"max_map_keys"));

        let cursor = src.rfind('=').unwrap() + 1;
        let items = cfg_completion(c, cursor);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert_eq!(labels, vec!["bpftrace", "perf", "raw"]);
    }

    #[test]
    fn test_struct() {
        let src = r#"
        struct Foo {
            int32 x;
            uint64 y;
            invalid_type invalid;
        }

        BEGIN {
            $f = (struct Foo *)arg0; $f->
        }
        "#;
        let program = ast::parse(src).unwrap();

        let cursor = src.find("$f->").unwrap() + "$f->".len();
        let field = field_access_at(&program, cursor).expect("field access at cursor");
        let structs = semantic_analyzer::collect_structs(&program);
        let var_types = semantic_analyzer::collect_var_types(&program);
        let labels: Vec<_> = field_completion(field, &structs, &var_types)
            .into_iter()
            .map(|i| i.label)
            .collect();

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "x");
        assert_eq!(labels[1], "y");
    }

    #[test]
    fn test_resolve_expr_type() {
        let src = r#"
        struct Bar {
            uint64 sector;
        }

        struct Foo {
            struct Bar *bar;
        }

        BEGIN {
            $f = (struct Foo *)arg0;
            $f->bar->
        }
        "#;
        let program = ast::parse(src).unwrap();
        let structs = semantic_analyzer::collect_structs(&program);
        let var_types = semantic_analyzer::collect_var_types(&program);
        let cursor = src.find("$f->bar->").unwrap() + "$f->bar->".len();
        let field = field_access_at(&program, cursor).expect("field access at cursor");

        let base_type = semantic_analyzer::resolve_expr_type(&field.base, &structs, &var_types)
            .expect("base type");

        assert_eq!(
            base_type,
            TypeInfo::StructLike {
                kind: crate::parser::TypeKind::Struct,
                name: "Bar".to_string(),
                pointers: 1,
            }
        );
    }

    #[test]
    fn test_assignment_type_propagation() {
        let src = r#"
        struct Inner {
            int64 x;
        }

        struct Outer {
            struct Inner *field1;
        }

        BEGIN {
            $test = (struct Outer *)arg0;
            $x = $test->field1;
            $x->
        }
        "#;
        let program = ast::parse(src).unwrap();
        let cursor = src.find("$x->").unwrap() + "$x->".len();
        let field = field_access_at(&program, cursor).expect("field access at cursor");
        let structs = semantic_analyzer::collect_structs(&program);
        let var_types = semantic_analyzer::collect_var_types(&program);
        let labels: Vec<_> = field_completion(field, &structs, &var_types)
            .into_iter()
            .map(|i| i.label)
            .collect();

        assert_eq!(labels, vec!["x"]);
    }
}
