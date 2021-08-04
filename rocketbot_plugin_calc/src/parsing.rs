use std::collections::VecDeque;

use log::trace;
use num_bigint::BigInt;
use pest::Parser;
use pest::error::Error;
use pest::iterators::{Pair, Pairs};
use pest_derive::Parser;

use crate::ast::{AstNode, AstNodeAtLocation, BinaryOperation, UnaryOperation};
use crate::numbers::{Number, NumberValue};
use crate::units::NumberUnits;


#[derive(Parser)]
#[grammar = "calc_lang.pest"]
struct CalcParser;


pub(crate) fn parse_full_expression(text: &str) -> Result<AstNodeAtLocation, Error<Rule>> {
    let pairs: Vec<Pair<'_, Rule>> = match CalcParser::parse(Rule::full_expression, text) {
        Ok(p) => p,
        Err(e) => return Err(e),
    }.collect();

    assert_eq!(pairs.len(), 1);

    let mut inner = pairs[0].clone().into_inner();
    let expression = inner.next().expect("no expression");
    Ok(parse_expression(&expression))
}

fn parse_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_expression: {:?}", pair.as_rule());
    let mut inner = pair.clone().into_inner();
    let bor_expression = inner.next().expect("no bor_expression");
    parse_bor_expression(&bor_expression)
}

fn generic_parse_left_assoc_expr<C, O>(pair: &Pair<'_, Rule>, right_assoc: bool, mut child_func: C, mut op_match_func: O) -> AstNodeAtLocation
    where
        C: FnMut(&Pair<'_, Rule>) -> AstNodeAtLocation,
        O: FnMut(&str) -> BinaryOperation
{
    let mut inner = pair.clone().into_inner();
    let mut operands: VecDeque<AstNodeAtLocation> = VecDeque::new();
    let mut operations: VecDeque<BinaryOperation> = VecDeque::new();

    operands.push_back(child_func(&inner.next().expect("missing first operand")));

    while let Some(operator) = inner.next() {
        let operand = inner.next()
            .expect("missing additional operand");
        let operation = op_match_func(operator.as_str());
        operations.push_back(operation);
        operands.push_back(child_func(&operand));
    }

    let final_node = if right_assoc {
        let mut current_node = operands.pop_back()
            .expect("at least one element is available");
        while let Some(prev_node) = operands.pop_back() {
            let operation = operations.pop_back()
                .expect("ran out of operations before we rand out of operands");
            current_node = AstNodeAtLocation {
                node: AstNode::BinaryOperation(operation, Box::new(prev_node), Box::new(current_node)),
                start_end: Some((pair.as_span().start(), pair.as_span().end())),
            };
        }
        current_node
    } else {
        let mut current_node = operands.pop_front()
            .expect("at least one element is available");
        while let Some(next_node) = operands.pop_front() {
            let operation = operations.pop_front()
                .expect("ran out of operations before we rand out of operands");
            current_node = AstNodeAtLocation {
                node: AstNode::BinaryOperation(operation, Box::new(current_node), Box::new(next_node)),
                start_end: Some((pair.as_span().start(), pair.as_span().end())),
            };
        }
        current_node
    };

    final_node
}

fn parse_bor_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_bor_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, false, |c| parse_bxor_expression(c), |_| BinaryOperation::BinaryOr)
}
fn parse_bxor_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_bxor_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, false, |c| parse_band_expression(c), |_| BinaryOperation::BinaryXor)
}
fn parse_band_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_band_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, false, |c| parse_addsub_expression(c), |_| BinaryOperation::BinaryAnd)
}
fn parse_addsub_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_addsub_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, false, |c| parse_muldivrem_expression(c), |op| match op {
        "+" => BinaryOperation::Add,
        "-" => BinaryOperation::Subtract,
        other => panic!("unknown addsub operator {}", other),
    })
}
fn parse_muldivrem_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_muldivrem_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, false, |c| parse_pow_expression(c), |op| match op {
        "*" => BinaryOperation::Multiply,
        "/" => BinaryOperation::Divide,
        "//" => BinaryOperation::DivideIntegral,
        "%" => BinaryOperation::Remainder,
        other => panic!("unknown muldivrem operator {}", other),
    })
}
fn parse_pow_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_pow_expression: {:?}", pair.as_rule());
    generic_parse_left_assoc_expr(pair, true, |c| parse_neg_expression(c), |_| BinaryOperation::Power)
}

fn parse_neg_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_neg_expression: {:?}", pair.as_rule());
    let mut is_negated: bool = false;
    let mut inner = pair.clone().into_inner();
    while let Some(child) = inner.next() {
        if child.as_rule() == Rule::neg_op {
            is_negated = !is_negated;
        } else {
            // operand
            return if is_negated {
                AstNodeAtLocation {
                    node: AstNode::UnaryOperation(UnaryOperation::Negate, Box::new(parse_fac_expression(&child))),
                    start_end: Some((pair.as_span().start(), pair.as_span().end())),
                }
            } else {
                parse_fac_expression(&child)
            };
        }
    }
    panic!("missing operand");
}

fn parse_fac_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_fac_expression: {:?}", pair.as_rule());
    let mut inner = pair.clone().into_inner();
    let operand = inner.next().expect("missing operand");
    let mut node = parse_atom_expression(&operand);

    while let Some(_factorial_call) = inner.next() {
        node = AstNodeAtLocation {
            node: AstNode::UnaryOperation(UnaryOperation::Factorial, Box::new(node)),
            start_end: Some((pair.as_span().start(), pair.as_span().end())),
        };
    }

    node
}

fn parse_atom_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_atom_expression: {:?}", pair.as_rule());
    let mut inner = pair.clone().into_inner();
    let child = inner.next().expect("missing atom");
    match child.as_rule() {
        Rule::call_expression => parse_call_expression(&child),
        Rule::identifier => AstNodeAtLocation {
            node: AstNode::Constant(child.as_str().to_owned()),
            start_end: Some((pair.as_span().start(), pair.as_span().end())),
        },
        Rule::parens_expression => parse_expression(&child),
        Rule::integer_expression => {
            let mut innerer = child.into_inner();
            let integer: BigInt = innerer
                .next().expect("missing integer")
                .as_str().parse().expect("failed to parse integer");
            let units = parse_unit_suffixes(innerer);
            AstNodeAtLocation {
                node: AstNode::Number(Number::new(
                    NumberValue::Int(integer),
                    units,
                )),
                start_end: Some((pair.as_span().start(), pair.as_span().end())),
            }
        },
        Rule::decimal_expression => {
            let mut innerer = child.into_inner();
            let float: f64 = innerer
                .next().expect("missing float")
                .as_str().parse().expect("failed to parse decimal expression");
            let units = parse_unit_suffixes(innerer);
            AstNodeAtLocation {
                node: AstNode::Number(Number::new(
                    NumberValue::Float(float),
                    units,
                )),
                start_end: Some((pair.as_span().start(), pair.as_span().end())),
            }
        },
        other => panic!("unexpected rule {:?}", other),
    }
}

fn parse_unit_suffixes(mut pairs: Pairs<'_, Rule>) -> NumberUnits {
    trace!("parse_unit_suffixes: {:?}", pairs);
    let mut number_units = NumberUnits::new();
    while let Some(pair) = pairs.next() {
        let mut inner = pair.into_inner();

        let unit_abbrev = inner.next().expect("unit abbreviation missing")
            .as_str();
        let unit_pow: BigInt = match inner.next() {
            Some(up) => up.as_str()
                .parse().expect("failed to parse unit power"),
            None => BigInt::from(1),
        };
        number_units.insert(
            unit_abbrev.to_owned(),
            unit_pow,
        );
    }
    number_units
}

fn parse_call_expression(pair: &Pair<'_, Rule>) -> AstNodeAtLocation {
    trace!("parse_call_expression: {:?}", pair.as_rule());
    let mut inner = pair.clone().into_inner();
    let identifier = inner.next()
        .expect("missing identifier")
        .as_str()
        .to_owned();

    let arglist_pair = inner.next().expect("no arglist");
    let arguments = parse_arglist(&arglist_pair);

    AstNodeAtLocation {
        node: AstNode::FunctionCall(identifier, arguments),
        start_end: Some((pair.as_span().start(), pair.as_span().end())),
    }
}

fn parse_arglist(pair: &Pair<'_, Rule>) -> Vec<AstNodeAtLocation> {
    trace!("parse_arglist: {:?}", pair.as_rule());
    let mut inner = pair.clone().into_inner();

    let mut arguments: Vec<AstNodeAtLocation> = Vec::new();
    while let Some(arg_pair) = inner.next() {
        let arg = parse_expression(&arg_pair);
        arguments.push(arg);
    }

    arguments
}
