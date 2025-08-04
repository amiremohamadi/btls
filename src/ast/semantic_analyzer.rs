use super::{Lvalue, Preamble, Statement};

pub struct SemanticAnalyzer {
    pub variables: Vec<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            variables: Vec::new(),
        }
    }

    pub fn insert(&mut self, name: String) {
        self.variables.push(name);
    }

    pub fn analyze(&mut self, path: &str) {
        let data = std::fs::read_to_string(path).unwrap();
        let ast = super::parser::parse(&data);
        ast.preambles
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
            .for_each(|x| match &x.lvalue {
                Lvalue::Identifier(ident) => {
                    self.insert(format!("${}", ident.name));
                }
            });
    }
}
