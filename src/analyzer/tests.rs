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

macro_rules! assert_diag_msg {
    ($diag:expr, $sub:literal) => {
        assert!(
            $diag.message.contains($sub),
            "expected diag containing {:?}, got: {:?}",
            $sub,
            $diag.message,
        );
    };
}

#[tokio::test]
async fn test_sanity() {
    let prog = r#"
        #define MAX 10
        #define FLAG
        config = { stack_mode=perf; max_map_keys=2 }
        BEGIN {
            $var = 1;
            $undefined;
            print($undefined);
            $var2 = count();
            $var3 = undefinedfunc();
            if ($var > MAX) {
                print(FLAG);
            }
            print(UNKNOWN);
        }"#;

    let path = Path::new("tmp_path");
    let context = init_context();
    {
        let mut storage = context.storage.lock().await;
        storage.load(path, prog, 0);
    }

    let analyzer = context.analyzer.lock().await;
    let analyzed = analyzer.analyze(&context, path).await.unwrap();
    assert_eq!(analyzed.variables.len(), 3);

    let errors = analyzed.diagnostics();
    assert_eq!(errors.len(), 4);
    assert_diag_msg!(errors[1], "Undefined Identifier");
    assert_diag_msg!(errors[2], "Undefined function");
    assert_diag_msg!(errors[3], "Undefined Identifier");
    assert_diag_msg!(errors[3], "UNKNOWN");
}
