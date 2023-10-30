use std::collections::HashMap;
use std::f64::consts::PI;

use num_bigint::{BigInt, ToBigInt};
use num_traits::ToPrimitive;
use once_cell::sync::Lazy;

use crate::ast::{
    AstNode, AstNodeAtLocation, BuiltInFunction, BuiltInFuncResult, SimplificationError,
    SimplificationState,
};
use crate::numbers::{Number, NumberValue};
use crate::units::{coerce_to_base_units, coerce_to_unit, NumberUnits};


pub const GOLDEN_RATIO: f64 = 1.6180339887498948482045868344;
pub const WGS84_EQUATOR_RADIUS_M: f64 = 6_378_137.0;
pub const WGS84_INVERSE_FLATTENING: f64 = 298.257_223_563;
pub static WGS84_MEAN_RADIUS: Lazy<f64> = Lazy::new(|| ellipsoid_mean_radius(WGS84_EQUATOR_RADIUS_M, WGS84_INVERSE_FLATTENING));


pub(crate) fn get_canonical_constants() -> HashMap<String, AstNode> {
    let mut prepared: HashMap<&str, AstNode> = HashMap::new();

    prepared.insert("pi", AstNode::from(std::f64::consts::PI));
    prepared.insert("e", AstNode::from(std::f64::consts::E));
    prepared.insert("goldenRatio", AstNode::from(GOLDEN_RATIO));
    prepared.insert("theAnswerToLifeTheUniverseAndEverything", AstNode::from(BigInt::from(42)));
    prepared.insert("numberOfHornsOnAUnicorn", AstNode::from(BigInt::from(1)));
    prepared.insert("earthR", AstNode::from(*WGS84_MEAN_RADIUS));

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
    // the default for angles is radians because mathematicians hate their fellow humans
    // (the feeling is mutual)
    // let's be the change we want to see in the world
    prepared.insert("havsinrad", f64_x5_f64("havsinrad", haversine));
    prepared.insert("havsin", f64_x5_f64("havsin", haversine_deg));

    prepared.insert("ceil", f64_f64asint("ceil", |f| f.ceil()));
    prepared.insert("floor", f64_f64asint("floor", |f| f.floor()));
    prepared.insert("round", f64_f64asint("round", |f| f.round()));
    prepared.insert("trunc", f64_f64asint("trunc", |f| f.trunc()));

    prepared.insert("coerce", Box::new(coerce));
    prepared.insert("setunit", Box::new(set_unit));
    prepared.insert("baseunits", Box::new(to_base_units));
    prepared.insert("c2f", f64_f64("c2f", |f| f * 9.0/5.0 + 32.0));
    prepared.insert("f2c", f64_f64("f2c", |f| (f - 32.0) * 5.0/9.0));

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

fn f64_x5_f64<F>(name: &'static str, inner: F) -> BuiltInFunction
    where F: Fn(f64, f64, f64, f64, f64) -> f64 + 'static
{
    Box::new(move |_state, operands| {
        if operands.len() != 5 {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), 2, operands.len()));
        }
        
        let mut f64_operands = [0.0; 5];
        for i in 0..5 { 
            let f64_op = match &operands[i].node {
                AstNode::Number(n) => {
                    match &n.value {
                        NumberValue::Int(i) => i.to_f64().expect("conversion failed"),
                        NumberValue::Float(f) => *f,
                    }
                },
                other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
            };
            f64_operands[i] = f64_op;
        }

        Ok(AstNode::Number(Number::new(
            NumberValue::Float(inner(f64_operands[0], f64_operands[1], f64_operands[2], f64_operands[3], f64_operands[4])),
            NumberUnits::new(),
        )))
    })
}

#[inline]
fn deg2rad(deg: f64) -> f64 {
    deg * PI / 180.0
}

fn haversine(radius: f64, lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let left = ((lat2-lat1)/2.0).sin().powi(2);
    let right = lat1.cos() * lat2.cos() * ((lon2-lon1)/2.0).sin().powi(2);
    2.0 * radius * (left + right).sqrt().asin()
}

fn haversine_deg(radius: f64, lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    haversine(
        radius,
        deg2rad(lat1),
        deg2rad(lon1),
        deg2rad(lat2),
        deg2rad(lon2),
    )
}

fn ellipsoid_pole_radius(equator_radius: f64, inv_flattening: f64) -> f64 {
    equator_radius - (inv_flattening.recip() * equator_radius)
}

fn ellipsoid_mean_radius(equator_radius: f64, inv_flattening: f64) -> f64 {
    let prad = ellipsoid_pole_radius(equator_radius, inv_flattening);
    (2.0*equator_radius + prad) / 3.0
}

/// Takes two operands and attempts to convert the first operand to the unit of the second. The
/// numeric value of the second operand is ignored; only the unit is taken into account.
fn coerce(state: &SimplificationState, operands: &[AstNodeAtLocation]) -> BuiltInFuncResult {
    if operands.len() != 2 {
        return Err(SimplificationError::IncorrectArgCount("coerce".to_owned(), 2, operands.len()));
    }

    let left_number = match &operands[0].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };
    let right_number = match &operands[1].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };

    match coerce_to_unit(left_number, &right_number.units, &state.units) {
        Some(n) => Ok(AstNode::Number(n)),
        None => Err(SimplificationError::UnitReconciliation),
    }
}

/// Takes two operands and returns the value of the first operand with the units of the second
/// operand. No conversion is performed; the units of the second operand are simply attached to the
/// number in the first operand.
///
/// Units can be stripped from a number by passing a unitless value as the second operand.
fn set_unit(_state: &SimplificationState, operands: &[AstNodeAtLocation]) -> BuiltInFuncResult {
    if operands.len() != 2 {
        return Err(SimplificationError::IncorrectArgCount("setunit".to_owned(), 2, operands.len()));
    }

    let left_number = match &operands[0].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };
    let right_number = match &operands[1].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };

    Ok(AstNode::Number(Number::new(
        left_number.value.clone(),
        right_number.units.clone(),
    )))
}

/// Takes a single operand and returns its value converted to base units.
fn to_base_units(state: &SimplificationState, operands: &[AstNodeAtLocation]) -> BuiltInFuncResult {
    if operands.len() != 1 {
        return Err(SimplificationError::IncorrectArgCount("baseunits".to_owned(), 1, operands.len()));
    }

    let number = match &operands[0].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };

    let result = coerce_to_base_units(number, &state.units);

    Ok(AstNode::Number(result))
}
