use itertools::Itertools;
use pest::{Parser, iterators::Pair};

use super::{
    AssignOp, Assignment, Block, Call, ErrorPreamble, ErrorStatement, Expr, Identifier,
    IntegerLiteral, Lvalue, Preamble, Probe, Program, Statement, StringLiteral, UnknownPreamble,
    UnknownStatement,
};

#[derive(pest_derive::Parser)]
#[grammar = "ast/bpftrace.pest"]
struct BPFTraceParser;

fn convert_int(pair: Pair<Rule>) -> IntegerLiteral {
    assert!(matches!(pair.as_rule(), Rule::number));
    IntegerLiteral {
        value: pair.as_str().parse().unwrap(),
        span: pair.as_span(),
    }
}

fn convert_str(pair: Pair<Rule>) -> StringLiteral {
    assert!(matches!(pair.as_rule(), Rule::string));
    let span = pair.as_span();
    StringLiteral {
        value: pair.as_str(),
        span,
    }
}

fn convert_ident(pair: Pair<Rule>) -> Identifier {
    assert!(matches!(pair.as_rule(), Rule::identifier));
    Identifier {
        name: pair.as_str(),
        span: pair.as_span(),
    }
}

fn convert_assign_op(pair: Pair<Rule>) -> AssignOp {
    assert!(matches!(pair.as_rule(), Rule::assign_op));
    match pair.as_str() {
        "=" => AssignOp::Assign,
        "+=" => AssignOp::AddAssign,
        "-=" => AssignOp::SubAssign,
        _ => unreachable!(),
    }
}

fn convert_expr_list(pair: Pair<Rule>) -> Vec<Expr> {
    assert!(matches!(pair.as_rule(), Rule::expr_list));
    let mut pairs = pair.into_inner();
    let Some(first) = pairs.next() else {
        return Vec::new();
    };

    let mut exprs = vec![convert_expr(first)];
    pairs.tuples().for_each(|(_, expr)| {
        exprs.push(convert_expr(expr));
    });

    exprs
}

fn convert_primary_expr(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::primary));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::identifier => Expr::Identifier(Box::new(convert_ident(pair))),
        Rule::number => Expr::Integer(Box::new(convert_int(pair))),
        Rule::string => Expr::String(Box::new(convert_str(pair))),
        _ => unreachable!(),
    }
}

fn convert_expr(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::expr));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::primary => convert_primary_expr(pair),
        _ => unreachable!(),
    }
}

fn convert_lvalue(pair: Pair<Rule>) -> Lvalue {
    assert!(matches!(pair.as_rule(), Rule::identifier));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::identifier => Lvalue::Identifier(Box::new(convert_ident(pair))),
        _ => unreachable!(),
    }
}

fn convert_assignment(pair: Pair<Rule>) -> Assignment {
    assert!(matches!(pair.as_rule(), Rule::assignment));
    let span = pair.as_span();
    let (lvalue, op, rvalue) = pair.into_inner().collect_tuple().unwrap();
    // let lvalue = convert_lvalue(lvalue);
    let lvalue = convert_ident(lvalue);
    let op = convert_assign_op(op);
    let rvalue = convert_expr(rvalue);
    Assignment {
        lvalue: Lvalue::Identifier(Box::new(lvalue)),
        rvalue: Box::new(rvalue),
        span,
    }
}

fn convert_call(pair: Pair<Rule>) -> Call {
    assert!(matches!(pair.as_rule(), Rule::call));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();
    let func = convert_ident(pairs.next().unwrap());
    let args = convert_expr_list(pairs.next().unwrap());
    Call { func, args, span }
}

fn convert_statement(pair: Pair<Rule>) -> Statement {
    assert!(matches!(pair.as_rule(), Rule::statement));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::assignment => Statement::Assignment(Box::new(convert_assignment(pair))),
        Rule::call => Statement::Call(Box::new(convert_call(pair))),
        _ => unreachable!(),
    }
}

fn convert_block(pair: Pair<Rule>) -> Block {
    assert!(matches!(pair.as_rule(), Rule::block));
    let span = pair.as_span();
    let statements = pair
        .into_inner()
        .filter_map(|pair| match pair.as_rule() {
            Rule::error => Some(Statement::Error(Box::new(
                ErrorStatement::UnknownStatement(Box::new(UnknownStatement {
                    text: pair.as_str(),
                    span: pair.as_span(),
                })),
            ))),
            Rule::statement => Some(convert_statement(pair)),
            Rule::comment => None,
            _ => None,
        })
        .collect();
    Block { statements, span }
}

fn convert_attach_points(pair: Pair<Rule>) -> Vec<&str> {
    assert!(matches!(pair.as_rule(), Rule::attach_point_list));
    let mut pairs = pair.into_inner();
    let Some(first) = pairs.next() else {
        return Vec::new();
    };

    let mut attach_points = vec![first.as_str()];
    pairs.tuples().for_each(|(_, ap)| {
        attach_points.push(ap.as_str());
    });

    attach_points
}

fn convert_probe(pair: Pair<Rule>) -> Probe {
    assert!(matches!(pair.as_rule(), Rule::probe));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let attach_points = convert_attach_points(pairs.next().unwrap());

    let next = pairs.next().unwrap();
    let (condition, next) = match next {
        p if matches!(p.as_rule(), Rule::probe_condition) => {
            let expr = p.into_inner().exactly_one().unwrap();
            let next = pairs.next().unwrap();
            (Some(convert_expr(expr)), next)
        }
        _ => (None, next),
    };

    let block = convert_block(next);

    Probe {
        span,
        attach_points,
        condition,
        block,
    }
}

fn convert_preamble(pair: Pair<Rule>) -> Preamble {
    assert!(matches!(pair.as_rule(), Rule::preamble));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::probe => Preamble::Probe(convert_probe(pair)),
        _ => unreachable!(),
    }
}

fn convert_prog(pair: Pair<Rule>) -> Program {
    assert!(matches!(pair.as_rule(), Rule::program));
    let span = pair.as_span();
    let preambles = pair
        .into_inner()
        .filter_map(|pair| match pair.as_rule() {
            Rule::preamble => Some(convert_preamble(pair)),
            Rule::error => Some(Preamble::Error(Box::new(ErrorPreamble::UnknownPreamble(
                Box::new(UnknownPreamble {
                    text: pair.as_str(),
                    span: pair.as_span(),
                }),
            )))),
            _ => None,
        })
        .collect();
    Program { preambles, span }
}

pub fn parse(input: &str) -> Program {
    let pair = BPFTraceParser::parse(Rule::program, input)
        .unwrap()
        .exactly_one()
        .unwrap();
    convert_prog(pair)
}
