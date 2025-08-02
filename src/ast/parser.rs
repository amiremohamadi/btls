use itertools::Itertools;
use pest::{Parser, iterators::Pair};

use super::{Program, Probe, Block, ErrorStatement, Statement, UnknownStatement};

#[derive(pest_derive::Parser)]
#[grammar = "ast/bpftrace.pest"]
struct BPFTraceParser;

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
            // Rule::statement => Some(convert_statement()),
            Rule::comment => None,
            _ => None,
        })
        .collect();
    Block { statements , span }
}

fn convert_probe(pair: Pair<Rule>) -> Probe {
    assert!(matches!(pair.as_rule(), Rule::probe));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let attach_point = pairs.next().unwrap().as_str();
    let block = convert_block(pairs.next().unwrap());

    Probe {
        span,
        attach_point,
        block,
    }
}

fn convert_prog(pair: Pair<Rule>) -> Program {
    assert!(matches!(pair.as_rule(), Rule::program));
    let span = pair.as_span();
    let probes = pair
        .into_inner()
        .filter_map(|pair| match pair.as_rule() {
            Rule::probe => Some(convert_probe(pair)),
            Rule::comment => None,
            _ => None,
        })
        .collect();
    Program { probes , span }
}

pub fn parse(input: &str) -> Program {
    let pair = BPFTraceParser::parse(Rule::program, input)
        .unwrap()
        .exactly_one()
        .unwrap();
    convert_prog(pair)
}
