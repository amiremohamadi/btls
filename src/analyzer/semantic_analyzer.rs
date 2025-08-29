use crate::builtins::BUILTINS;
use crate::parser::{
    Expr, Lvalue, Node, Preamble, Program, Statement, UndefinedFunc, UndefinedIdent, Walk,
    ast::parse,
};
use crate::server::Context;
use anyhow::Result;
use std::path::Path;

pub struct SemanticAnalyzer {
    pub content: String,
    // pub variables: Vec<String>,
}

pub struct AnalyzedFile<'a> {
    pub variables: Vec<String>,
    pub ast: Program<'a>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            content: "".to_string(),
        }
    }

    pub async fn analyze(&mut self, context: &Context, path: &Path) -> Result<AnalyzedFile> {
        // self.content = std::fs::read_to_string(path).unwrap();
        self.content = context.storage.lock().await.read(path);
        let mut ast = parse(&self.content)?;
        let mut variables = vec![];
        let mut errors = vec![];

        let root = Walk::new(ast.as_node());
        root.into_iter().for_each(|n| {
            match n.as_expr() {
                Some(Expr::Identifier(ident)) => {
                    let mut keywords = variables
                        .iter()
                        .filter_map(|x: &String| x.strip_prefix("$"))
                        .map(|x| x.to_string())
                        .chain(BUILTINS.keywords.iter().map(|x| x.name.to_string()));

                    (!keywords.any(|x| x == ident.name)).then(|| {
                        errors.push(UndefinedIdent::new(ident.name, ident.span));
                    });
                }
                Some(Expr::Call(call)) => {
                    (!BUILTINS
                        .functions
                        .iter()
                        .map(|x| x.name)
                        .any(|x| x == call.func.name))
                    .then(|| {
                        errors.push(UndefinedFunc::new(call.func.name, call.span()));
                    });
                }
                _ => {}
            }
            if let Some(stmt) = n.as_statement() {
                match stmt {
                    Statement::Assignment(a) => match &a.lvalue {
                        Lvalue::Identifier(ident) => variables.push(format!("${}", ident.name)),
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

        Ok(AnalyzedFile { ast, variables })
    }
}
