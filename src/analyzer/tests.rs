#![cfg(test)]

use super::*;
use crate::parser::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_sanity() {
    let prog = r#"
        BEGIN {
            $var = 1;
            $undefined;
            print($undefined);
            $var2 = count();
            $var3 = undefinedfunc();
        }"#;
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{}", prog).unwrap();

    let mut analyzer = semantic_analyzer::SemanticAnalyzer::new();
    let analyzed = analyzer.analyze(file.path().to_str().unwrap()).unwrap();
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
