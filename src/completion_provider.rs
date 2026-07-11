use super::analyzer::semantic_analyzer;
use super::analyzer::semantic_analyzer::{StructInfo, StructScope, TypeInfo};
use super::btf::{self, BtfScope};
use super::builtins::{BUILTINS, DATA_TYPES, SYNTAX_KEYWORDS};
use super::parser::{Config, Expr, FieldAccess, Node, Preamble, Program, Walk};
use super::server::Context;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
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
        let args = probe_args_at(analyzed.ast(), offset, &analyzed.btf_scopes);
        let structs = StructScope::new(&analyzed.struct_defs).with(&args);
        let items = field_completion(field, &structs, &analyzed.var_types);
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

    let macros = macro_completion_items(analyzed.ast(), offset);

    Ok(Some(CompletionResponse::Array(
        variables
            .into_iter()
            .chain(macros)
            .chain(builtin_keywords)
            .chain(syntax_keywords)
            .chain(data_types)
            .chain(builtin_funcs)
            .collect(),
    )))
}

fn macro_completion_items(program: &Program, offset: usize) -> Vec<CompletionItem> {
    semantic_analyzer::macros_at(program, offset)
        .into_iter()
        .map(|r#macro| CompletionItem {
            label: r#macro.name.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(r#macro.to_string()),
            documentation: (!r#macro.comments.is_empty()).then(|| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: r#macro.comments.join("\n"),
                })
            }),
            ..Default::default()
        })
        .collect()
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

fn probe_args_at(
    program: &Program,
    offset: usize,
    btf_scopes: &HashMap<String, Arc<BtfScope>>,
) -> HashMap<String, StructInfo> {
    let mut layer = HashMap::new();
    for preamble in &program.preambles {
        if let Preamble::Probe(probe) = preamble {
            if probe.span.start() <= offset && offset < probe.span.end() {
                if let Some(scope) =
                    btf::probe_func(&probe.attach_points).and_then(|func| btf_scopes.get(func))
                {
                    layer.insert("args".to_string(), scope.args.clone());
                }
                break;
            }
        }
    }
    layer
}

fn field_completion(
    field: &FieldAccess,
    structs: &StructScope,
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
    fn test_macro_completion() {
        let src = r#"// increments its argument
        // in place
        macro inc($x) { $x += 1 }
        BEGIN {  }"#;
        let program = ast::parse(src).unwrap();

        let items = macro_completion_items(&program, src.len());
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.label, "inc");
        assert_eq!(item.kind, Some(CompletionItemKind::FUNCTION));
        assert_eq!(item.detail.as_deref(), Some("macro inc($x)"));

        let Some(Documentation::MarkupContent(doc)) = &item.documentation else {
            panic!("expected docs!");
        };
        assert_eq!(doc.value, "increments its argument\nin place");
    }

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
        let var_types =
            semantic_analyzer::collect_var_types_with_btf(&program, &structs, &HashMap::new());
        let scope = StructScope::new(&structs);
        let labels: Vec<_> = field_completion(field, &scope, &var_types)
            .into_iter()
            .map(|i| i.label)
            .collect();

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "x");
        assert_eq!(labels[1], "y");
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
        let var_types =
            semantic_analyzer::collect_var_types_with_btf(&program, &structs, &HashMap::new());
        let scope = StructScope::new(&structs);
        let labels: Vec<_> = field_completion(field, &scope, &var_types)
            .into_iter()
            .map(|i| i.label)
            .collect();

        assert_eq!(labels, vec!["x"]);
    }

    #[test]
    fn test_btf_args_completion() {
        use crate::btf::Btf;

        let src = "fentry:tcp_v4_do_rcv { $x = args. }";
        let program = ast::parse(src).unwrap();

        let btf = Btf::new();
        let mut btf_scopes = HashMap::new();
        if let Some(scope) = btf.probe_scope("tcp_v4_do_rcv") {
            btf_scopes.insert("tcp_v4_do_rcv".to_string(), scope);
        }
        if btf_scopes.is_empty() {
            eprintln!("skipping: kernel BTF unavailable?");
            return;
        }

        let cursor = src.find("args.").unwrap() + "args.".len();
        let field = field_access_at(&program, cursor).expect("field access at cursor");
        let structs = semantic_analyzer::collect_structs(&program);
        let var_types =
            semantic_analyzer::collect_var_types_with_btf(&program, &structs, &btf_scopes);

        let mut args_layer = HashMap::new();
        if let Some(args_scope) = btf_scopes.get("tcp_v4_do_rcv") {
            args_layer.insert("args".to_string(), args_scope.args.clone());
        }
        let scope = StructScope::new(&structs).with(&args_layer);

        let labels: Vec<_> = field_completion(field, &scope, &var_types)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.iter().any(|l| l == "sk"));
        assert!(labels.iter().any(|l| l == "skb"));
    }
}
