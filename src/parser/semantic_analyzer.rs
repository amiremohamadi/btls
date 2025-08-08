use super::{Expr, Lvalue, Preamble, Program, Statement, UndefinedIdent};
use crate::builtins::BUILTINS;
use anyhow::Result;

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

    fn _analyze_block(&self, block: &mut super::Block, variables: &mut Vec<String>) {
        let mut errors = vec![];
        for s in block.statements.iter_mut() {
            match s {
                Statement::Assignment(a) => match &a.lvalue {
                    Lvalue::Identifier(ident) => variables.push(format!("${}", ident.name)),
                },
                Statement::Expr(e) => {
                    if let Expr::Identifier(ident) = e.as_ref() {
                        let mut keywords = variables
                            .iter()
                            .filter_map(|x| x.strip_prefix("$"))
                            .map(|x| x.to_string())
                            .chain(BUILTINS.keywords.iter().map(|x| x.name.to_string()));

                        (!keywords.any(|x| x == ident.name)).then(|| {
                            errors.push(UndefinedIdent::new(ident.name, ident.span));
                        });
                    }
                }
                Statement::IfCond(c) => {
                    self._analyze_block(&mut c.block, variables);
                }
                _ => {}
            }
        }
        block.statements.extend(errors);
    }

    pub fn analyze(&mut self, path: &str) -> Result<AnalyzedFile> {
        self.content = std::fs::read_to_string(path).unwrap();
        let mut ast = super::ast::parse(&self.content)?;
        let mut variables = vec![];

        for p in ast.preambles.iter_mut() {
            match p {
                Preamble::Probe(p) => self._analyze_block(&mut p.block, &mut variables),
                _ => {}
            }
        }

        Ok(AnalyzedFile { ast, variables })
    }
}
