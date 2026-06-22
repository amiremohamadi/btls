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
