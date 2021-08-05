use std::collections::HashMap;

use num_bigint::{BigInt, ToBigInt};
use num_traits::ToPrimitive;

use crate::ast::{AstNode, BuiltInFunction, SimplificationError};
use crate::numbers::{Number, NumberValue};
use crate::units::NumberUnits;


pub const GOLDEN_RATIO: f64 = 1.6180339887498948482045868344;


pub(crate) fn get_canonical_constants() -> HashMap<String, AstNode> {
    let mut prepared: HashMap<&str, AstNode> = HashMap::new();

    prepared.insert("pi", AstNode::from(std::f64::consts::PI));
    prepared.insert("e", AstNode::from(std::f64::consts::E));
    prepared.insert("goldenRatio", AstNode::from(GOLDEN_RATIO));
    prepared.insert("theAnswerToLifeTheUniverseAndEverything", AstNode::from(BigInt::from(42)));
    prepared.insert("numberOfHornsOnAUnicorn", AstNode::from(BigInt::from(1)));

    prepared.drain()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

pub(crate) fn get_canonical_functions() -> HashMap<String, BuiltInFunction> {
    let mut prepared: HashMap<&str, BuiltInFunction> = HashMap::new();

    prepared.insert("sqrt", f64_f64("sqrt", |f| f.sqrt()));

    prepared.insert("sin", f64_f64("sin", |f| f.sin()));
    prepared.insert("cos", f64_f64("cos", |f| f.cos()));
    prepared.insert("tan", f64_f64("tan", |f| f.tan()));
    prepared.insert("exp", f64_f64("exp", |f| f.tan()));
    prepared.insert("asin", f64_f64("asin", |f| f.asin()));
    prepared.insert("acos", f64_f64("acos", |f| f.acos()));
    prepared.insert("atan", f64_f64("atan", |f| f.atan()));
    prepared.insert("atan2", f64_f64_f64("atan2", |f, g| f.atan2(g)));
    prepared.insert("sinh", f64_f64("sinh", |f| f.sinh()));
    prepared.insert("cosh", f64_f64("cosh", |f| f.cosh()));
    prepared.insert("tanh", f64_f64("tanh", |f| f.tanh()));
    prepared.insert("ln", f64_f64("ln", |f| f.ln()));
    prepared.insert("log10", f64_f64("log10", |f| f.log10()));
    prepared.insert("log", f64_f64_f64("log", |f, g| f.log(g)));

    prepared.insert("ceil", f64_f64asint("ceil", |f| f.ceil()));
    prepared.insert("floor", f64_f64asint("floor", |f| f.floor()));
    prepared.insert("round", f64_f64asint("round", |f| f.round()));
    prepared.insert("trunc", f64_f64asint("trunc", |f| f.trunc()));

    prepared.drain()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}


fn f64_f64<F>(name: &'static str, inner: F) -> BuiltInFunction
    where F: Fn(f64) -> f64 + 'static
{
    Box::new(move |_state, operands| {
        if operands.len() != 1 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 1, operands.len()));
        }

        let (operand, units): (f64, NumberUnits) = match &operands[0].node {
            AstNode::Number(n) => {
                match &n.value {
                    NumberValue::Int(i) => (i.to_f64().expect("conversion failed"), n.units.clone()),
                    NumberValue::Float(f) => (*f, n.units.clone()),
                }
            },
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };

        Ok(AstNode::Number(Number::new(
            NumberValue::Float(inner(operand)),
            units,
        )))
    })
}


fn f64_f64asint<F>(name: &'static str, inner: F) -> BuiltInFunction
    where F: Fn(f64) -> f64 + 'static
{
    Box::new(move |_state, operands| {
        if operands.len() != 1 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 1, operands.len()));
        }

        let (operand, units): (f64, NumberUnits) = match &operands[0].node {
            AstNode::Number(n) => {
                match &n.value {
                    NumberValue::Int(i) => (i.to_f64().expect("conversion failed"), n.units.clone()),
                    NumberValue::Float(f) => (*f, n.units.clone()),
                }
            },
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };
        let result = inner(operand);
        let result_bint = match result.to_bigint() {
            Some(bi) => bi,
            None => return Err(SimplificationError::NonIntegralValue(result)),
        };

        Ok(AstNode::Number(Number::new(
            NumberValue::Int(result_bint),
            units,
        )))
    })
}


fn f64_f64_f64<F>(name: &'static str, inner: F) -> BuiltInFunction
    where F: Fn(f64, f64) -> f64 + 'static
{
    Box::new(move |_state, operands| {
        if operands.len() != 2 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 2, operands.len()));
        }

        let (left, left_units): (f64, NumberUnits) = match &operands[0].node {
            AstNode::Number(n) => {
                match &n.value {
                    NumberValue::Int(i) => (i.to_f64().expect("conversion failed"), n.units.clone()),
                    NumberValue::Float(f) => (*f, n.units.clone()),
                }
            },
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };
        let (right, right_units): (f64, NumberUnits) = match &operands[1].node {
            AstNode::Number(n) => {
                match &n.value {
                    NumberValue::Int(i) => (i.to_f64().expect("conversion failed"), n.units.clone()),
                    NumberValue::Float(f) => (*f, n.units.clone()),
                }
            },
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };

        if right_units.len() > 0 {
            return Err(SimplificationError::RightOperandHasUnits);
        }

        Ok(AstNode::Number(Number::new(
            NumberValue::Float(inner(left, right)),
            left_units,
        )))
    })
}
