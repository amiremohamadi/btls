#![cfg(test)]

use super::*;

fn parse_no_errors(input: &str) {
    let prog = parse(input);
    let errors: Vec<_> = prog.errors().collect();
    assert!(errors.is_empty(), "parse failed!");
}

#[test]
fn test_sanity() {
    parse_no_errors("");
    parse_no_errors("// this is a comment");
    parse_no_errors("BEGIN {}");
    parse_no_errors("END {}");
    parse_no_errors("tracepoint:sched:* {}");
    parse_no_errors("tracepoint:sched:* { $x = 1; }");
    parse_no_errors("tracepoint:sched:* { $x  = 1   ; }");
}

#[test]
fn test_probe() {
    let prog = parse("tracepoint:sched:* {}");
    assert_eq!(prog.probes.len(), 1);

    let probe = &prog.probes[0];
    assert_eq!(probe.attach_point, "tracepoint:sched:*");
    assert_eq!(probe.block.statements.len(), 0);
}

#[test]
fn test_statements() {
    let prog = parse(
        r#"BEGIN {
        $x = 2;
        $y  = 3;
    }"#,
    );
    let probe = &prog.probes[0];
    assert_eq!(probe.block.statements.len(), 2);
    assert!(matches!(
        probe.block.statements[0],
        Statement::Assignment(_)
    ));
}
