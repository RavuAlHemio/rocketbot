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
    prepared.insert("earthER", AstNode::from(WGS84_EQUATOR_RADIUS_M));
    prepared.insert("earthIF", AstNode::from(WGS84_INVERSE_FLATTENING));

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
    prepared.insert("havsinrad", f64_multi_f64("havsinrad", haversine_array));
    prepared.insert("havsin", f64_multi_f64("havsin", haversine_deg_array));
    prepared.insert("elldisrad", f64_multi_f64("elldisrad", ellipsoid_distance_array));
    prepared.insert("elldis", f64_multi_f64("elldis", ellipsoid_distance_deg_array));

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

fn f64_multi_f64<F, const ARG_COUNT: usize>(name: &'static str, inner: F) -> BuiltInFunction
    where F: Fn([f64; ARG_COUNT]) -> f64 + 'static
{
    Box::new(move |_state, operands| {
        if operands.len() != ARG_COUNT {
            return Err(SimplificationError::IncorrectArgCount(name.to_owned(), ARG_COUNT, operands.len()));
        }
        
        let mut f64_operands = [0.0; ARG_COUNT];
        for i in 0..ARG_COUNT { 
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
            NumberValue::Float(inner(f64_operands)),
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
fn haversine_array(operands: [f64; 5]) -> f64 {
    haversine(operands[0], operands[1], operands[2], operands[3], operands[4])
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
fn haversine_deg_array(operands: [f64; 5]) -> f64 {
    haversine_deg(operands[0], operands[1], operands[2], operands[3], operands[4])
}

fn ellipsoid_pole_radius(equator_radius: f64, inv_flattening: f64) -> f64 {
    equator_radius - (inv_flattening.recip() * equator_radius)
}

fn ellipsoid_mean_radius(equator_radius: f64, inv_flattening: f64) -> f64 {
    let prad = ellipsoid_pole_radius(equator_radius, inv_flattening);
    (2.0*equator_radius + prad) / 3.0
}

#[allow(non_snake_case)]
fn ellipsoid_distance(equator_radius: f64, inv_flattening: f64, lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    // Vincenty's formulae
    let a = equator_radius;
    let f = 1.0/inv_flattening;
    let b = (1.0 - f) * a;

    let U1 = ((1.0 - f) * lat1.tan()).atan();
    let U2 = ((1.0 - f) * lat2.tan()).atan();
    let L = lon2 - lon1;

    let mut lambda = L;
    let mut cos2_alpha;
    let mut sin_sigma;
    let mut cos_sigma;
    let mut sigma;
    let mut cos_2sigmam;
    loop {
        let prev_lambda = lambda;
        sin_sigma = (
            (U2.cos() * lambda.sin()).powi(2)
            + (U1.cos() * U2.sin() - U1.sin() * U2.cos() * lambda.cos()).powi(2)
        ).sqrt();
        cos_sigma = U1.sin() * U2.sin() + U1.cos() * U2.cos() * lambda.cos();
        sigma = sin_sigma.atan2(cos_sigma);
        let sin_alpha = (U1.cos() * U2.cos() * lambda.sin()) / sigma.sin();
        cos2_alpha = 1.0 - sin_alpha.powi(2);
        cos_2sigmam = sigma.cos() - (2.0 * U1.sin() * U2.sin()) / cos2_alpha;
        let C = f / 16.0 * cos2_alpha * (4.0 + f * (4.0 - 3.0 * cos2_alpha));
        lambda = L + (1.0 - C) * f * sin_alpha * (
            sigma + C * sin_sigma * (
                cos_2sigmam + C * cos_sigma * (
                    -1.0 + 2.0 * cos_2sigmam.powi(2)
                )
            )
        );
        if (lambda - prev_lambda).abs() < 1e-6 {
            break;
        }
    }

    let u2 = cos2_alpha * (a.powi(2) - b.powi(2)) / b.powi(2);
    let A = 1.0 + u2 / 16384.0 * (4096.0 + u2 * (-768.0 + u2 * (320.0 - 175.0 * u2)));
    let B = u2 / 1024.0 * (256.0 + u2 * (128.0 + u2 * (74.0 - 47.0 * u2)));
    let delta_sigma = B * sin_sigma * (
        cos_2sigmam + 1.0/4.0 * B * (
            cos_sigma * (
                -1.0 + 2.0 * cos_2sigmam.powi(2)
            )
            - B/6.0 * cos_2sigmam * (-3.0 + 4.0 * sin_sigma.powi(2)) * (-3.0 + 4.0 * cos_2sigmam.powi(2))
        )
    );
    let s = b * A * (sigma - delta_sigma);

    s
}
fn ellipsoid_distance_array(operands: [f64; 6]) -> f64 {
    ellipsoid_distance(operands[0], operands[1], operands[2], operands[3], operands[4], operands[5])
}


fn ellipsoid_distance_deg(equator_radius: f64, inv_flattening: f64, lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    ellipsoid_distance(
        equator_radius,
        inv_flattening,
        deg2rad(lat1),
        deg2rad(lon1),
        deg2rad(lat2),
        deg2rad(lon2),
    )
}
fn ellipsoid_distance_deg_array(operands: [f64; 6]) -> f64 {
    ellipsoid_distance_deg(operands[0], operands[1], operands[2], operands[3], operands[4], operands[5])
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
