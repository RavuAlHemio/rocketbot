use std::collections::HashMap;

use num_bigint::{BigInt, ToBigInt};
use num_traits::ToPrimitive;

use crate::ast::{AstNode, BuiltInFunction, SimplificationError};


pub const GOLDEN_RATIO: f64 = 1.6180339887498948482045868344;


pub(crate) fn get_canonical_constants() -> HashMap<String, AstNode> {
    let mut prepared: HashMap<&str, AstNode> = HashMap::new();

    prepared.insert("pi", AstNode::Float(std::f64::consts::PI));
    prepared.insert("e", AstNode::Float(std::f64::consts::E));
    prepared.insert("goldenRatio", AstNode::Float(GOLDEN_RATIO));
    prepared.insert("theAnswerToLifeTheUniverseAndEverything", AstNode::Int(BigInt::from(42)));
    prepared.insert("numberOfHornsOnAUnicorn", AstNode::Int(BigInt::from(1)));

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
    prepared.insert("deg2rad", f64_f64("deg2rad", |f| f * std::f64::consts::PI / 180.0));
    prepared.insert("deg2gon", f64_f64("deg2gon", |f| f * 100.0 / 90.0));
    prepared.insert("rad2deg", f64_f64("rad2deg", |f| f * 180.0 / std::f64::consts::PI));
    prepared.insert("rad2gon", f64_f64("rad2gon", |f| f * 200.0 / std::f64::consts::PI));
    prepared.insert("gon2deg", f64_f64("gon2deg", |f| f * 90.0 / 100.0));
    prepared.insert("gon2rad", f64_f64("gon2rad", |f| f * std::f64::consts::PI / 200.0));
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


fn f64_f64<F>(name: &'static str, mut inner: F) -> BuiltInFunction
    where F: FnMut(f64) -> f64 + 'static
{
    Box::new(move |operands| {
        if operands.len() != 1 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 1, operands.len()));
        }

        let operand: f64 = match &operands[0].node {
            AstNode::Int(i) => i.to_f64().expect("conversion failed"),
            AstNode::Float(f) => *f,
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };

        Ok(AstNode::Float(inner(operand)))
    })
}


fn f64_f64asint<F>(name: &'static str, mut inner: F) -> BuiltInFunction
    where F: FnMut(f64) -> f64 + 'static
{
    Box::new(move |operands| {
        if operands.len() != 1 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 1, operands.len()));
        }

        let operand: f64 = match &operands[0].node {
            AstNode::Int(i) => i.to_f64().expect("conversion failed"),
            AstNode::Float(f) => *f,
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };
        let result = inner(operand);
        let result_bint = match result.to_bigint() {
            Some(bi) => bi,
            None => return Err(SimplificationError::NonIntegralValue(result)),
        };

        Ok(AstNode::Int(result_bint))
    })
}


fn f64_f64_f64<F>(name: &'static str, mut inner: F) -> BuiltInFunction
    where F: FnMut(f64, f64) -> f64 + 'static
{
    Box::new(move |operands| {
        if operands.len() != 2 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 2, operands.len()));
        }

        let left: f64 = match &operands[0].node {
            AstNode::Int(i) => i.to_f64().expect("conversion failed"),
            AstNode::Float(f) => *f,
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };
        let right: f64 = match &operands[1].node {
            AstNode::Int(i) => i.to_f64().expect("conversion failed"),
            AstNode::Float(f) => *f,
            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
        };

        Ok(AstNode::Float(inner(left, right)))
    })
}
