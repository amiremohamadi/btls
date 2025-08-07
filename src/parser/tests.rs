#![cfg(test)]

use super::ast::parse;
use super::*;

fn parse_no_errors(input: &str) {
    let prog = parse(input).unwrap();
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
    parse_no_errors("END, BEGIN { $x  = 1   ; }");
    parse_no_errors("END, BEGIN / 1 / {}");
    parse_no_errors("BEGIN { $x = 1 + 2 - 3 * 4; }");
    parse_no_errors("BEGIN { if ($x == 1) {} }");

    // should fail
    // variable outside probe
    let prog = parse("$x = 1").unwrap();
    assert!(
        prog.errors().collect::<Vec<_>>().len() > 0,
        "parsed without any errors!"
    );
}

#[test]
fn test_probe() {
    let prog = parse("tracepoint:sched:* { }").unwrap();
    assert_eq!(prog.preambles.len(), 1);

    let Preamble::Probe(probe) = &prog.preambles[0] else {
        panic!("not a probe!");
    };
    assert_eq!(probe.attach_points[0], "tracepoint:sched:*");
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
    )
    .unwrap();
    let Preamble::Probe(probe) = &prog.preambles[0] else {
        panic!("not a probe!");
    };
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
    )
    .unwrap();
    let Preamble::Probe(probe) = &prog.preambles[0] else {
        panic!("not a probe!");
    };
    assert_eq!(probe.block.statements.len(), 6);
    assert!(matches!(probe.block.statements[1], Statement::Call(_)));
}
