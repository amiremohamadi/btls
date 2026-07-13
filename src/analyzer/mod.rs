pub mod analyzed;
pub mod semantic_analyzer;
mod tests;

use std::sync::Arc;

use crate::parser::Program;
use self_cell::self_cell;

self_cell!(
    struct OwnedAstCell {
        owner: Arc<String>,
        #[covariant]
        dependent: Program,
    }
    impl {Debug}
);

#[derive(Clone, Debug)]
pub struct OwnedAst(Arc<OwnedAstCell>);

impl OwnedAst {
    pub fn new(source: Arc<String>) -> anyhow::Result<Self> {
        let _ = crate::parser::ast::parse(source.as_str())?;
        Ok(Self(Arc::new(OwnedAstCell::new(source, |data| {
            crate::parser::ast::parse(data.as_str()).unwrap()
        }))))
    }

    pub fn program(&self) -> &Program<'_> {
        self.0.borrow_dependent()
    }
}

use analyzed::AnalyzedProgram;

self_cell!(
    struct AnalyzedProgramCell {
        owner: OwnedAst,
        #[covariant]
        dependent: AnalyzedProgram,
    }
);

#[derive(Clone)]
pub struct OwnedAnalyzedProgram(Arc<AnalyzedProgramCell>);

impl OwnedAnalyzedProgram {
    pub fn new(ast: OwnedAst) -> Self {
        Self(Arc::new(AnalyzedProgramCell::new(ast, |ast| {
            analyzed::analyze_program(ast.program())
        })))
    }

    pub fn get(&self) -> &AnalyzedProgram<'_> {
        self.0.borrow_dependent()
    }
}
