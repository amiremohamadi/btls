use super::{Lvalue, Statement};

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
        ast.probes
            .iter()
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
