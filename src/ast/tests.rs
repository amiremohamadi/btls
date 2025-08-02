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
        $y = 3;
        $y += 6;
        $x -= 0;
        $str = "string";
    }"#,
    );
    let probe = &prog.probes[0];
    assert_eq!(probe.block.statements.len(), 5);
    assert!(matches!(
        probe.block.statements[0],
        Statement::Assignment(_)
    ));
}

#[test]
fn test_calls() {
    let prog = parse(
        r#"BEGIN {
        $x = 1;
        func();
        func(1);
        func(1, 2);
        func( 1, 2, $x );
        $z = func(69);
    }"#,
    );
    let probe = &prog.probes[0];
    assert_eq!(probe.block.statements.len(), 6);
    assert!(matches!(probe.block.statements[1], Statement::Call(_)));
}
