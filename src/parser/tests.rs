#![cfg(test)]

use super::ast::parse;
use super::*;

fn has_errors(prog: &Program<'_>) -> bool {
    prog.preambles
        .iter()
        .any(|p| matches!(p, Preamble::Error(_)))
        || Walk::new(prog.as_node())
            .any(|node| matches!(node.as_statement(), Some(Statement::Error(_))))
}

fn parse_no_errors(input: &str) {
    let prog = parse(input).unwrap();
    assert!(!has_errors(&prog), "parse failed: {input:?}");
}

fn check_config(input: &str, expected: &[(&str, &str)]) {
    let prog = parse(input).unwrap();
    assert!(!has_errors(&prog), "parse failed: {input:?}");
    assert_eq!(prog.preambles.len(), 1);
    let Preamble::Config(config) = &prog.preambles[0] else {
        panic!("not a config block!");
    };
    assert_eq!(config.assignments.len(), expected.len());
    for (i, (key, value)) in expected.iter().enumerate() {
        assert_eq!(config.assignments[i].key, *key);
        assert_eq!(config.assignments[i].value, Some(*value));
    }
}

fn parse_has_errors(input: &str) {
    let prog = parse(input).unwrap();
    assert!(
        has_errors(&prog),
        "expected a parse error but got none for {input:?}"
    );
}

#[test]
fn test_sanity() {
    parse_no_errors("");
    parse_no_errors("// this is a comment");
    parse_no_errors("/* multi\nline\ncomment */");
    parse_no_errors("BEGIN {}");
    parse_no_errors("END {}");
    parse_no_errors("tracepoint:sched:* {}");
    parse_no_errors("tracepoint:sched:* { $x = 1; }");
    parse_no_errors("tracepoint:sched:* { $x  = 1   ; }");
    parse_no_errors("END, BEGIN { $x  = 1   ; }");
    parse_no_errors("END, BEGIN / 1 / {}");
    parse_no_errors("BEGIN { $x = 1 + 2 - 3 * 4; }");
    parse_no_errors("BEGIN { $x = 1 & 2 | 3 ^ 4 << 5 >> 6; }");
    parse_no_errors("BEGIN { $x = 1 + 2 - 3 * func($y, $z); }");
    parse_no_errors("BEGIN { if ($x == 1) {} }");
    parse_no_errors("BEGIN { if ($x == 1) { return; } }");
    parse_no_errors("BEGIN { if ($x == 1) {} else {} }");
    parse_no_errors("BEGIN { if ($x == 1) {} else if ($x == 2) {} }");
    parse_no_errors("BEGIN { while ($x < $y) { return; } }");
    parse_no_errors("BEGIN { for ($x : $y) { $var += 1; } }");
    parse_no_errors("BEGIN { unroll(3) { $var += 1; } }");
    parse_no_errors("BEGIN { $x = (uint64)1; }");
    parse_no_errors("BEGIN { $x = (uint64)arg3; }");
    parse_no_errors("BEGIN { $x = (uint16) -1; }");
    parse_no_errors("BEGIN { $x = (int64 *)$y * 2; }");
    parse_no_errors("BEGIN { @map = 1 + 2; $var = -1; $var = +2; $var2 = @map + -1; }");
    parse_no_errors("BEGIN { $var++; --$var; }");
    parse_no_errors("BEGIN { $x++; ++$x; $x--; --$x; @map++; ++@map; }");
    parse_no_errors("BEGIN { @map[1] = 2; }");
    parse_no_errors("BEGIN { @map[1] = @m2[$x]; }");
    parse_no_errors("BEGIN { @map[1, 2] = 3; }");
    parse_no_errors("BEGIN { @[$x] = 1; }");
    parse_no_errors("BEGIN { $x = @map[1]; }");
    parse_no_errors("BEGIN { @map[1]++; }");
    parse_no_errors("BEGIN { ++@map[1]; }");
    parse_no_errors("BEGIN { @map[@n] = $x; }");
    parse_no_errors("BEGIN { $x = 0xFF_FF; $y = 1_000_000; $z = 2e3; }");

    parse_no_errors("macro one() { 1 }");
    parse_no_errors("macro add_one(x) { x + 1 }");
    parse_no_errors("macro add_one_to_each($a, @b) { $a += 1; @b += 1; }");
    parse_no_errors("macro wrong_parameter_type(@m[2]) { $x++ }"); // should be valid in ast level
    parse_no_errors("macro anonymous_map_param(@[x]) { 1 }");
    parse_no_errors("macro one() { 1 }\nBEGIN { print(one()); print(one); }");

    parse_no_errors("#include <linux/sched.h>");
    parse_no_errors("#define MAX 100");
    parse_no_errors("#define FLAG");
    parse_no_errors("#include <linux/sched.h>\n#define MAX 100\nBEGIN { $x = MAX; }");

    parse_has_errors("#define ADD(a, b) ((a) + (b))");
    parse_has_errors("BEGIN { @m[] = 2; }");
    parse_has_errors("chertopert");
    parse_has_errors("12313");
    parse_has_errors("s\n12313\nBEGIN { @c = 0; }");

    // variable outside probe
    let prog = parse("$x = 1").unwrap();
    assert!(
        matches!(&prog.preambles[0], Preamble::Error(e) if matches!(**e, ErrorPreamble::UnknownPreamble(_))),
        "unexpected error type"
    );

    // unmatched brace
    let prog = parse("BEGIN { } }").unwrap();
    assert!(
        prog.preambles.iter().any(
            |p| matches!(p, Preamble::Error(e) if matches!(**e, ErrorPreamble::UnmatchedBrace(_)))
        ),
        "unexpected error type"
    );

    // config blocks
    parse_no_errors("config = { }");
    parse_no_errors("config = { stack_mode=perf; }");
    parse_no_errors("config = { stack_mode=perf; max_map_keys=2 }");
    parse_no_errors("config = { stack_mode=perf; }\nBEGIN { }");
    parse_no_errors("#define MAX 100\nconfig = { max_map_keys=2 }\nBEGIN { }");

    check_config(
        "config = { stack_mode=perf; max_map_keys=2 }",
        &[("stack_mode", "perf"), ("max_map_keys", "2")],
    );

    // tuples
    parse_no_errors("BEGIN { $a = (1, 2); }");

    // field access
    parse_no_errors("BEGIN { $x = $f->pid; }");
    parse_no_errors("BEGIN { $x = $f.pid; }");
    parse_no_errors("BEGIN { $x = $a->b->c; }");
    parse_no_errors("BEGIN { $x = ((struct Foo *)arg0)->pid; }");
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
        $last_stmt_without_semicolon = 2
    }"#,
    )
    .unwrap();
    let Preamble::Probe(probe) = &prog.preambles[0] else {
        panic!("not a probe!");
    };
    assert_eq!(probe.block.statements.len(), 6);
    assert!(matches!(
        probe.block.statements[0],
        Statement::Assignment(_, _)
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
    let Statement::Expr(call, _) = &probe.block.statements[1] else {
        panic!("not an expression!");
    };
    assert!(matches!(call.as_ref(), Expr::Call(_)));
}

#[test]
fn test_macros() {
    let prog = parse(
        r#"
        // first line
        // second line
        macro add_one_to_each($a, @b, x) {
            $a += 1;
            @b += 1;
            x + 1
        }

        BEGIN {
            add_one_to_each($x, @y, 1 + 2);
        }"#,
    )
    .unwrap();

    assert_eq!(prog.preambles.len(), 2);

    let Preamble::Macro(r#macro) = &prog.preambles[0] else {
        panic!("not a macro!");
    };
    assert_eq!(r#macro.name.name, "add_one_to_each");
    assert_eq!(r#macro.comments, vec!["first line", "second line"]);
    assert_eq!(r#macro.params.len(), 3);
    assert_eq!(r#macro.params[0].name().kind, IdentKind::Scratch);
    assert_eq!(r#macro.params[1].name().kind, IdentKind::Map);
    assert_eq!(r#macro.params[2].name().kind, IdentKind::Bare);

    let Preamble::Probe(probe) = &prog.preambles[1] else {
        panic!("not a probe!");
    };
    let Statement::Expr(call, _) = &probe.block.statements[0] else {
        panic!("not an expression!");
    };
    let Expr::Call(call) = call.as_ref() else {
        panic!("not a call!");
    };
    assert_eq!(call.func.name, "add_one_to_each");
    assert_eq!(call.args.len(), 3);
}

#[test]
fn test_loops() {
    let prog = parse(
        r#"BEGIN {
            while ($x < 69) {
                $x += 1;
            }

            while ($y != $x) {}
        }"#,
    )
    .unwrap();

    let walk = Walk::new(prog.as_node());
    let loops = walk
        .into_iter()
        .filter(|n| matches!(n.as_statement(), Some(Statement::Loop(..))))
        .collect::<Vec<_>>()
        .len();

    assert_eq!(loops, 2);
}

#[test]
fn test_structs() {
    parse_no_errors("struct Foo { int32 x; uint64 name }");
    parse_no_errors("struct Foo { int32 x; };");
    parse_no_errors("union U { int32 a; uint64 b; }");
    parse_no_errors("struct Nested { struct Foo *next; uint64 flags; }");
    parse_no_errors("struct Foo { int32 x; }\nBEGIN { $f = (struct Foo *)curtask; }");

    let prog = parse("struct Foo { int32 x; uint64 name; }").unwrap();
    let Preamble::CDef(cdef) = &prog.preambles[0] else {
        panic!("not a cdef!");
    };
    let CDef::Struct(def) = cdef.as_ref() else {
        panic!("not a struct def!");
    };
    assert_eq!(def.name.name, "Foo");
    assert_eq!(def.fields.len(), 2);
    assert_eq!(def.fields[0].0.name.name, "x");
    assert_eq!(def.fields[0].0.type_name.text(), "int32");
    assert!(matches!(def.fields[0].0.type_name.kind, TypeKind::Builtin));
    assert_eq!(def.fields[1].0.name.name, "name");
    assert_eq!(def.fields[1].0.type_name.text(), "uint64");
    assert!(matches!(def.fields[1].0.type_name.kind, TypeKind::Builtin));
}

#[test]
fn test_map() {
    let prog = parse("BEGIN { @map[1] = 2; }").unwrap();

    let Preamble::Probe(probe) = &prog.preambles[0] else {
        panic!("not a probe!");
    };

    assert_eq!(probe.block.statements.len(), 1);
    let Statement::Assignment(assign, _) = &probe.block.statements[0] else {
        panic!("not an assignment!");
    };

    assert!(matches!(&assign.lvalue, Lvalue::MapAccess(_)));
    if let Lvalue::MapAccess(access) = &assign.lvalue {
        assert_eq!(access.map.name, "map");
        assert_eq!(access.map.kind, IdentKind::Map);
        assert_eq!(access.keys.len(), 1);
        assert!(matches!(access.keys[0], Expr::Integer(_)));
    }
}
