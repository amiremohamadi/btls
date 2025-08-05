use super::{Lvalue, Preamble, Program, Statement};

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

    pub fn analyze(&mut self, path: &str) -> AnalyzedFile {
        self.content = std::fs::read_to_string(path).unwrap();
        let ast = super::ast::parse(&self.content);
        let variables = ast
            .preambles
            .iter()
            .filter_map(|p| match p {
                Preamble::Probe(p) => Some(p),
                _ => None,
            })
            .flat_map(|p| &p.block.statements)
            .filter_map(|s| match s {
                Statement::Assignment(a) => Some(a),
                _ => None,
            })
            .map(|x| match &x.lvalue {
                Lvalue::Identifier(ident) => {
                    format!("${}", ident.name)
                }
            })
            .collect();
        AnalyzedFile { ast, variables }
    }
}
