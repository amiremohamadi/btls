#![cfg(test)]

use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::*;
use crate::client::*;
use crate::parser::*;
use crate::server::*;
use crate::storage::*;

fn init_context() -> Context {
    let client = Client::new_test();
    let storage = Storage::new();
    let analyzer = semantic_analyzer::SemanticAnalyzer::new();

    Context {
        client,
        storage: Arc::new(Mutex::new(storage)),
        analyzer: Mutex::new(analyzer),
    }
}

#[tokio::test]
async fn test_sanity() {
    let prog = r#"
        BEGIN {
            $var = 1;
            $undefined;
            print($undefined);
            $var2 = count();
            $var3 = undefinedfunc();
        }"#;

    let path = Path::new("tmp_path");
    let context = init_context();
    {
        let mut storage = context.storage.lock().await;
        storage.load(path, prog);
    }

    let mut analyzer = context.analyzer.lock().await;
    let analyzed = analyzer.analyze(&context, path).await.unwrap();
    assert_eq!(analyzed.variables.len(), 3);

    let errors = analyzed.ast.errors().collect::<Vec<_>>();
    assert_eq!(errors.len(), 3);
    assert!(matches!(
        errors[1],
        ErrorRef::Statement(ErrorStatement::UndefinedIdent(..))
    ));
    assert!(matches!(
        errors[2],
        ErrorRef::Statement(ErrorStatement::UndefinedFunc(..))
    ));
}
