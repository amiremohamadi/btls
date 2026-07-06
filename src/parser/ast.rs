use anyhow::Result;
use itertools::Itertools;
use pest::{
    Parser, Span,
    iterators::Pair,
    pratt_parser::{Assoc, Op, PrattParser},
};

use super::{
    ArgNExpr, AssignOp, Assignment, BinaryExpr, Block, CDef, Call, CastExpr, Config,
    ConfigAssignment, Define, Else, ErrorPreamble, ErrorStatement, Expr, FieldAccess, FieldDecl,
    For, IdentKind, Identifier, If, Include, IntegerLiteral, Loop, Lvalue, MacroDefinition,
    MacroParam, MacroParamKind, MapAccess, Node, Preamble, Probe, Program, Statement,
    StringLiteral, StructDef, TypeKind, TypeName, UnaryExpr, UnaryOp, UnknownPreamble,
    UnknownStatement, UnmatchedBrace, Unroll, While,
};

#[derive(pest_derive::Parser)]
#[grammar = "parser/bpftrace.pest"]
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
        kind: IdentKind::Bare,
    }
}

fn convert_macro_param(pair: Pair<Rule>) -> MacroParam {
    assert!(matches!(pair.as_rule(), Rule::macro_param));
    let span = pair.as_span();
    let inner = pair.into_inner().exactly_one().unwrap();

    match inner.as_rule() {
        Rule::identifier => MacroParam::Expr {
            name: convert_ident(inner),
            span,
        },
        Rule::scratch_var => {
            let name = convert_ident(inner.into_inner().exactly_one().unwrap());
            MacroParam::Scratch {
                name: Identifier {
                    kind: IdentKind::Scratch,
                    ..name
                },
                span,
            }
        }
        Rule::map_var => {
            let mut parts = inner.into_inner();
            let first = parts.next();
            let (name, keyed) = match first {
                Some(first) if matches!(first.as_rule(), Rule::identifier) => {
                    let has_keys = parts.next().is_some();
                    (convert_ident(first), has_keys)
                }
                Some(first) if matches!(first.as_rule(), Rule::map_keys) => (
                    Identifier {
                        name: "",
                        span,
                        kind: IdentKind::Map,
                    },
                    true,
                ),
                None => (
                    Identifier {
                        name: "",
                        span,
                        kind: IdentKind::Map,
                    },
                    false,
                ),
                _ => unreachable!(),
            };
            MacroParam::Map {
                name: Identifier {
                    kind: IdentKind::Map,
                    ..name
                },
                keyed,
                span,
            }
        }
        _ => unreachable!(),
    }
}

fn convert_macro_def<'a>(pair: Pair<'a, Rule>, comments: Vec<&'a str>) -> MacroDefinition<'a> {
    assert!(matches!(pair.as_rule(), Rule::macro_def));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let name = convert_ident(pairs.next().unwrap());
    let next = pairs.next().unwrap();
    let (params, body) = match next.as_rule() {
        Rule::macro_params => (
            next.into_inner()
                .filter(|p| matches!(p.as_rule(), Rule::macro_param))
                .map(convert_macro_param)
                .collect(),
            pairs.next().unwrap(),
        ),
        _ => (Vec::new(), next),
    };

    MacroDefinition {
        name,
        params,
        body: convert_block(body),
        comments,
        span,
    }
}

fn convert_comment(pair: Pair<Rule>) -> &str {
    assert!(matches!(pair.as_rule(), Rule::comment));
    pair.into_inner()
        .next()
        .and_then(|comment| comment.into_inner().next())
        .map(|content| content.as_str().trim())
        .unwrap_or("")
}

fn convert_var_to_expr(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::variable));
    let span = pair.as_span();
    let inner = pair.into_inner().exactly_one().unwrap();
    match inner.as_rule() {
        Rule::scratch_var => {
            let ident = convert_ident(inner.into_inner().exactly_one().unwrap());
            Expr::Identifier(Box::new(Identifier {
                kind: IdentKind::Scratch,
                ..ident
            }))
        }
        Rule::map_var => convert_map_var_to_expr(inner, span),
        _ => unreachable!(),
    }
}

fn convert_map_var<'a>(pair: Pair<'a, Rule>) -> Identifier<'a> {
    assert!(matches!(pair.as_rule(), Rule::map_var));
    let mut ident = convert_ident(pair.into_inner().exactly_one().unwrap());
    ident.kind = IdentKind::Map;
    ident
}

fn convert_map_var_to_expr<'a>(pair: Pair<'a, Rule>, span: Span<'a>) -> Expr<'a> {
    assert!(matches!(pair.as_rule(), Rule::map_var));
    let mut inner = pair.into_inner();
    let Some(first) = inner.next() else {
        return Expr::Identifier(Box::new(Identifier {
            name: "",
            span,
            kind: IdentKind::Map,
        }));
    };
    match first.as_rule() {
        Rule::identifier => {
            let mut ident = convert_ident(first);
            ident.kind = IdentKind::Map;
            if let Some(keys_pair) = inner.next() {
                let keys = convert_expr_list(keys_pair.into_inner().exactly_one().unwrap());
                Expr::MapAccess(Box::new(MapAccess {
                    map: ident,
                    keys,
                    span,
                }))
            } else {
                Expr::Identifier(Box::new(ident))
            }
        }
        Rule::map_keys => {
            let ident = Identifier {
                name: "",
                span,
                kind: IdentKind::Map,
            };
            let keys = convert_expr_list(first.into_inner().exactly_one().unwrap());
            Expr::MapAccess(Box::new(MapAccess {
                map: ident,
                keys,
                span,
            }))
        }
        _ => unreachable!(),
    }
}

fn convert_var_to_lvalue(pair: Pair<Rule>) -> Lvalue {
    assert!(matches!(pair.as_rule(), Rule::variable));
    let span = pair.as_span();
    let inner = pair.into_inner().exactly_one().unwrap();
    match inner.as_rule() {
        Rule::scratch_var => {
            let ident = convert_ident(inner.into_inner().exactly_one().unwrap());
            Lvalue::Identifier(Box::new(Identifier {
                kind: IdentKind::Scratch,
                ..ident
            }))
        }
        Rule::map_var => convert_map_var_to_lvalue(inner, span),
        _ => unreachable!(),
    }
}

fn convert_map_var_to_lvalue<'a>(pair: Pair<'a, Rule>, span: Span<'a>) -> Lvalue<'a> {
    assert!(matches!(pair.as_rule(), Rule::map_var));
    let mut inner = pair.into_inner();
    let Some(first) = inner.next() else {
        return Lvalue::Identifier(Box::new(Identifier {
            name: "",
            span,
            kind: IdentKind::Map,
        }));
    };
    match first.as_rule() {
        Rule::identifier => {
            let mut ident = convert_ident(first);
            ident.kind = IdentKind::Map;
            if let Some(keys_pair) = inner.next() {
                let keys = convert_expr_list(keys_pair.into_inner().exactly_one().unwrap());
                Lvalue::MapAccess(Box::new(MapAccess {
                    map: ident,
                    keys,
                    span,
                }))
            } else {
                Lvalue::Identifier(Box::new(ident))
            }
        }
        Rule::map_keys => {
            let ident = Identifier {
                name: "",
                span,
                kind: IdentKind::Map,
            };
            let keys = convert_expr_list(first.into_inner().exactly_one().unwrap());
            Lvalue::MapAccess(Box::new(MapAccess {
                map: ident,
                keys,
                span,
            }))
        }
        _ => unreachable!(),
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
    pair.into_inner()
        .filter(|p| matches!(p.as_rule(), Rule::expr))
        .map(convert_expr)
        .collect()
}

fn convert_primary_expr(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::primary));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::identifier => Expr::Identifier(Box::new(convert_ident(pair))),
        Rule::number => Expr::Integer(Box::new(convert_int(pair))),
        Rule::string => Expr::String(Box::new(convert_str(pair))),
        Rule::call => Expr::Call(Box::new(convert_call(pair))),
        Rule::cast => convert_cast(pair),
        Rule::paren_expr => {
            let inner = pair.into_inner().exactly_one().unwrap();
            convert_expr(inner)
        }
        Rule::arg_n => Expr::ArgN(Box::new(convert_arg_n(pair))),
        Rule::variable => convert_var_to_expr(pair),
        _ => unreachable!(),
    }
}

fn convert_cast(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::cast));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();
    let type_name = convert_type_name(pairs.next().unwrap());
    let expr = convert_expr(pairs.next().unwrap());
    Expr::Cast(Box::new(CastExpr {
        type_name,
        expr: Box::new(expr),
        span,
    }))
}

fn convert_type_name(pair: Pair<Rule>) -> TypeName {
    assert!(matches!(pair.as_rule(), Rule::type_name));
    let span = pair.as_span();
    let mut pairs = pair.into_inner().peekable();

    let kind = match pairs.peek().map(|p| p.as_rule()) {
        Some(Rule::struct_kw) => match pairs.next().unwrap().as_str() {
            "struct" => TypeKind::Struct,
            "union" => TypeKind::Union,
            _ => unreachable!(),
        },
        Some(Rule::EOI) | None => TypeKind::Invalid,
        _ => TypeKind::Builtin,
    };

    let name = pairs.next().unwrap();
    let pointers = pairs.filter(|p| p.as_rule() == Rule::ptr).count();

    TypeName {
        kind,
        name: name.as_str(),
        pointers,
        span,
    }
}

fn convert_arg_n(pair: Pair<Rule>) -> ArgNExpr {
    let span = pair.as_span();
    let index = pair.as_str().trim_start_matches("arg").parse().unwrap();
    ArgNExpr { index, span }
}

fn convert_unary_op(op: &Pair<Rule>) -> UnaryOp {
    match op.as_rule() {
        Rule::not => UnaryOp::Not,
        Rule::neg => UnaryOp::Minus,
        Rule::pos => UnaryOp::Plus,
        Rule::inc_prefix | Rule::inc_postfix => UnaryOp::Inc,
        Rule::dec_prefix | Rule::dec_postfix => UnaryOp::Dec,
        Rule::deref => UnaryOp::Deref,
        _ => unreachable!(),
    }
}

fn convert_expr(pair: Pair<Rule>) -> Expr {
    assert!(matches!(pair.as_rule(), Rule::expr));
    let pairs = pair.into_inner();

    let parser = PrattParser::new()
        .op(Op::infix(Rule::and, Assoc::Left) | Op::infix(Rule::or, Assoc::Left))
        .op(Op::infix(Rule::ge, Assoc::Left)
            | Op::infix(Rule::gt, Assoc::Left)
            | Op::infix(Rule::le, Assoc::Left)
            | Op::infix(Rule::lt, Assoc::Left)
            | Op::infix(Rule::eq, Assoc::Left)
            | Op::infix(Rule::ne, Assoc::Left))
        .op(Op::infix(Rule::add, Assoc::Left)
            | Op::infix(Rule::sub, Assoc::Left)
            | Op::infix(Rule::mul, Assoc::Left)
            | Op::infix(Rule::div, Assoc::Left)
            | Op::infix(Rule::r#mod, Assoc::Left))
        .op(Op::prefix(Rule::not)
            | Op::prefix(Rule::neg)
            | Op::prefix(Rule::pos)
            | Op::prefix(Rule::inc_prefix)
            | Op::prefix(Rule::dec_prefix)
            | Op::prefix(Rule::deref))
        .op(Op::postfix(Rule::inc_postfix)
            | Op::postfix(Rule::dec_postfix)
            | Op::postfix(Rule::field_access));

    parser
        .map_primary(|p| convert_primary_expr(p))
        .map_prefix(|op, rhs| {
            let span = Span::new(
                op.as_span().get_input(),
                op.as_span().start(),
                rhs.span().end(),
            )
            .unwrap();
            Expr::UnaryExpr(Box::new(UnaryExpr {
                op: convert_unary_op(&op),
                expr: Box::new(rhs),
                span,
            }))
        })
        .map_postfix(|lhs, op| {
            let span = Span::new(
                lhs.span().get_input(),
                lhs.span().start(),
                op.as_span().end(),
            )
            .unwrap();
            match op.as_rule() {
                Rule::field_access => {
                    let field = op.into_inner().next().map(convert_ident);
                    Expr::Field(Box::new(FieldAccess {
                        base: Box::new(lhs),
                        field,
                        span,
                    }))
                }
                _ => Expr::UnaryExpr(Box::new(UnaryExpr {
                    op: convert_unary_op(&op),
                    expr: Box::new(lhs),
                    span,
                })),
            }
        })
        .map_infix(|lhs, _op, rhs| {
            let span =
                Span::new(lhs.span().get_input(), lhs.span().start(), rhs.span().end()).unwrap();
            Expr::BinaryExpr(Box::new(BinaryExpr {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            }))
        })
        .parse(pairs)
}

fn convert_assignment(pair: Pair<Rule>) -> Assignment {
    assert!(matches!(pair.as_rule(), Rule::assignment));
    let span = pair.as_span();
    let (lvalue, op, rvalue) = pair.into_inner().collect_tuple().unwrap();
    let lvalue = convert_var_to_lvalue(lvalue);
    let _op = convert_assign_op(op);
    let rvalue = convert_expr(rvalue);
    Assignment {
        lvalue,
        rvalue: Box::new(rvalue),
        span,
    }
}

fn convert_call(pair: Pair<Rule>) -> Call {
    assert!(matches!(pair.as_rule(), Rule::call));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();
    let func = convert_ident(pairs.next().unwrap());
    let args = pairs.next().map(convert_expr_list).unwrap_or_default();
    Call { func, args, span }
}

fn convert_if(pair: Pair<Rule>) -> If {
    assert!(matches!(pair.as_rule(), Rule::r#if));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let expr = convert_expr(pairs.next().unwrap());
    let block = convert_block(pairs.next().unwrap());
    let else_branch = pairs.next().map(convert_else).map(Box::new);

    If {
        condition: Box::new(expr),
        block,
        else_branch,
        span,
    }
}

fn convert_else(pair: Pair<Rule>) -> Else {
    assert!(matches!(pair.as_rule(), Rule::r#else));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::block => Else::Block(convert_block(pair)),
        Rule::r#if => Else::IfCond(Box::new(convert_if(pair))),
        _ => unreachable!(),
    }
}

fn convert_while(pair: Pair<Rule>) -> Loop {
    assert!(matches!(pair.as_rule(), Rule::r#while));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let expr = convert_expr(pairs.next().unwrap());
    let block = convert_block(pairs.next().unwrap());

    Loop::While(Box::new(While {
        condition: Box::new(expr),
        block,
        span,
    }))
}

fn convert_for(pair: Pair<Rule>) -> Loop {
    assert!(matches!(pair.as_rule(), Rule::r#for));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let lhs = convert_expr(pairs.next().unwrap());
    let rhs = convert_expr(pairs.next().unwrap());
    let block = convert_block(pairs.next().unwrap());

    Loop::For(Box::new(For {
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        block,
        span,
    }))
}

fn convert_unroll(pair: Pair<Rule>) -> Loop {
    assert!(matches!(pair.as_rule(), Rule::unroll));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    let count = convert_int(pairs.next().unwrap());
    let block = convert_block(pairs.next().unwrap());

    Loop::Unroll(Box::new(Unroll {
        count: Box::new(count),
        block,
        span,
    }))
}

fn convert_statement(pair: Pair<Rule>) -> Statement {
    assert!(matches!(pair.as_rule(), Rule::statement));
    let statement_span = pair.as_span();
    let inner = pair.into_inner().exactly_one().unwrap();
    // TODO: currently, semicolon matching happens during semantic analysis
    // to fix code completion issues when semicolons are missing.
    // check if this approach is common in other parsers as well.
    let has_semi = statement_span.end() > inner.as_span().end();
    match inner.as_rule() {
        Rule::assignment => Statement::Assignment(Box::new(convert_assignment(inner)), has_semi),
        Rule::r#if => Statement::IfCond(Box::new(convert_if(inner))),
        Rule::r#while => Statement::Loop(Box::new(convert_while(inner))),
        Rule::r#for => Statement::Loop(Box::new(convert_for(inner))),
        Rule::unroll => Statement::Loop(Box::new(convert_unroll(inner))),
        Rule::expr => Statement::Expr(Box::new(convert_expr(inner)), has_semi),
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

fn convert_include(pair: Pair<Rule>) -> Include {
    assert!(matches!(pair.as_rule(), Rule::include));
    let span = pair.as_span();
    let path = pair.into_inner().exactly_one().unwrap();
    Include {
        path: path.as_str(),
        span,
    }
}

fn convert_define(pair: Pair<Rule>) -> Define {
    assert!(matches!(pair.as_rule(), Rule::define));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();
    let name_pair = pairs.next().unwrap();
    assert!(matches!(name_pair.as_rule(), Rule::identifier));
    let name = Identifier {
        name: name_pair.as_str(),
        span: name_pair.as_span(),
        kind: IdentKind::Bare,
    };
    let body = pairs.next().map(|p| p.as_str());
    Define { name, body, span }
}

fn convert_cdef(pair: Pair<Rule>) -> CDef {
    assert!(matches!(pair.as_rule(), Rule::cdef));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::include => CDef::Include(Box::new(convert_include(pair))),
        Rule::define => CDef::Define(Box::new(convert_define(pair))),
        Rule::struct_def => CDef::Struct(Box::new(convert_struct_def(pair))),
        _ => unreachable!(),
    }
}

fn convert_struct_def(pair: Pair<Rule>) -> StructDef {
    assert!(matches!(pair.as_rule(), Rule::struct_def));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();

    // skip the leading struct keyword
    let name = pairs
        .find(|p| matches!(p.as_rule(), Rule::identifier))
        .map(convert_ident)
        .unwrap();
    let fields = pairs
        .filter(|p| matches!(p.as_rule(), Rule::field_decl))
        .map(convert_field_decl)
        .collect();

    StructDef { name, fields, span }
}

fn convert_field_decl(pair: Pair<Rule>) -> FieldDecl {
    assert!(matches!(pair.as_rule(), Rule::field_decl));
    let span = pair.as_span();
    let mut pairs = pair.into_inner();
    let type_pair = pairs.next().unwrap();

    assert!(matches!(type_pair.as_rule(), Rule::type_name));
    let type_name = convert_type_name(type_pair);
    let name = pairs
        .find(|p| matches!(p.as_rule(), Rule::identifier))
        .map(convert_ident)
        .unwrap();

    FieldDecl {
        name,
        type_name,
        span,
    }
}

fn convert_config(pair: Pair<Rule>) -> Config {
    assert!(matches!(pair.as_rule(), Rule::cfg_block));
    let span = pair.as_span();
    let assignments = pair
        .into_inner()
        .filter_map(|pair| match pair.as_rule() {
            Rule::cfg_assign => {
                let span = pair.as_span();
                let mut inner = pair.into_inner();
                let key = inner.next().unwrap().as_str();
                let value = inner.next().map(|p| p.as_str());
                Some(ConfigAssignment { key, value, span })
            }
            _ => None,
        })
        .collect();
    Config { assignments, span }
}

fn convert_preamble<'a>(pair: Pair<'a, Rule>, comments: Vec<&'a str>) -> Preamble<'a> {
    assert!(matches!(pair.as_rule(), Rule::preamble));
    let pair = pair.into_inner().exactly_one().unwrap();
    match pair.as_rule() {
        Rule::cfg_block => Preamble::Config(Box::new(convert_config(pair))),
        Rule::probe => Preamble::Probe(convert_probe(pair)),
        Rule::cdef => Preamble::CDef(Box::new(convert_cdef(pair))),
        Rule::macro_def => Preamble::Macro(Box::new(convert_macro_def(pair, comments))),
        _ => unreachable!(),
    }
}

fn convert_prog(pair: Pair<Rule>) -> Program {
    assert!(matches!(pair.as_rule(), Rule::program));
    let span = pair.as_span();
    let mut comments = Vec::new();
    let preambles = pair
        .into_inner()
        .filter_map(|pair| match pair.as_rule() {
            Rule::comment => {
                comments.push(convert_comment(pair));
                None
            }
            Rule::preamble => Some(convert_preamble(pair, std::mem::take(&mut comments))),
            Rule::error => Some(Preamble::Error(Box::new(ErrorPreamble::UnknownPreamble(
                Box::new(UnknownPreamble {
                    text: pair.as_str(),
                    span: pair.as_span(),
                }),
            )))),
            Rule::unmatched_brace => Some(Preamble::Error(Box::new(
                ErrorPreamble::UnmatchedBrace(Box::new(UnmatchedBrace {
                    span: pair.as_span(),
                })),
            ))),
            _ => None,
        })
        .collect();
    Program { preambles, span }
}

pub fn parse(input: &str) -> Result<Program> {
    let pair = BPFTraceParser::parse(Rule::program, input)?
        .exactly_one()
        .map_err(|_| anyhow::anyhow!("failed to consume"))?;
    Ok(convert_prog(pair))
}
