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
pub const SPEED_LIGHT_M_PER_S: i64 = 299_792_458;
pub const RACK_POST_GAP_19IN_IN: f64 = 17.75;


#[derive(Clone, Debug, PartialEq)]
pub struct Constant {
    pub value: AstNode,
    pub help_text: String,
}
impl Constant {
    pub fn new<V: Into<AstNode>, S: Into<String>>(value: V, help_text: S) -> Self {
        Self {
            value: value.into(),
            help_text: help_text.into(),
        }
    }
}


pub struct Function {
    pub function: BuiltInFunction,
    pub help_text: String,
}
impl Function {
    pub fn new<S: Into<String>>(function: BuiltInFunction, help_text: S) -> Self {
        Self {
            function,
            help_text: help_text.into(),
        }
    }
}


pub(crate) fn get_canonical_constants() -> HashMap<String, Constant> {
    let mut prepared: HashMap<&str, Constant> = HashMap::new();

    prepared.insert("pi", Constant::new(
        std::f64::consts::PI,
        "The ratio between a circle's circumference to its diameter.",
    ));
    prepared.insert("e", Constant::new(
        std::f64::consts::E,
        concat!(
            "The base of the natural logarithm and the limit of",
            " \\(\\left(1 + \\frac{1}{n}\\right)^{n}\\) with \\(n\\) approaching infinity.",
        ),
    ));
    prepared.insert("goldenRatio", Constant::new(
        GOLDEN_RATIO,
        concat!(
            "The ratio between \\(a\\) and \\(b\\) such that \\(\\frac{a}{b} = \\frac{a+b}{a}\\).",
            " Equal to \\(\\frac{1+\\sqrt{5}}{2}\\).",
        ),
    ));
    prepared.insert("theAnswerToLifeTheUniverseAndEverything", Constant::new(
        BigInt::from(42),
        "The answer to life, the universe and everything. (What was the question?)",
    ));
    prepared.insert("numberOfHornsOnAUnicorn", Constant::new(
        BigInt::from(1),
        concat!(
            "The number of horns on a unicorn. An important constant in Euler's identity:",
            " \\(e^{\\imath\\pi} + \\text{numberOfHornsOnAUnicorn} = 0\\)"
        ),
    ));
    prepared.insert("earthR", Constant::new(
        *WGS84_MEAN_RADIUS,
        concat!(
            "The mean radius of the Earth ellipsoid in meters, according to WGS84. You may want to",
            " use it in conjunction with the `havsin` function."
        ),
    ));
    prepared.insert("earthER", Constant::new(
        WGS84_EQUATOR_RADIUS_M,
        concat!(
            "The equatorial radius (semi-major axis \\(a\\)) of the Earth ellipsoid in meters,",
            " according to WGS84. You may want to use it with the `elldis` function.",
        ),
    ));
    prepared.insert("earthIF", Constant::new(
        WGS84_INVERSE_FLATTENING,
        concat!(
            "\\(\\frac{1}{f}\\), the inverse flattening of the Earth ellipsoid according to WGS84.",
            " You may want to use it with the `elldis` function.",
        ),
    ));

    let mut m_per_s = NumberUnits::new();
    m_per_s.insert("m".to_owned(), (1i8).into());
    m_per_s.insert("s".to_owned(), (-1i8).into());
    prepared.insert("c", Constant::new(
        Number::new(
            NumberValue::Int(BigInt::from(SPEED_LIGHT_M_PER_S)),
            m_per_s,
        ),
        "The speed of light in a vacuum, in meters per second.",
    ));

    let mut inches = NumberUnits::new();
    inches.insert("in".to_owned(), (1i8).into());
    prepared.insert("rackPostGap19in", Constant::new(
        Number::new(
            NumberValue::Float(RACK_POST_GAP_19IN_IN),
            inches,
        ),
        concat!(
            "The gap between the posts in a standard 19-inch rack. This is measured from the inner",
            " edge of one post to the inner edge of the other, not between the cage nut holes.",
            " Hardware meant for installation in a 19-inch rack generally has this width.",
        ),
    ));

    prepared.drain()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

pub(crate) fn get_canonical_functions() -> HashMap<String, Function> {
    let mut prepared: HashMap<&str, Function> = HashMap::new();

    prepared.insert("sqrt", f64_f64(
        "sqrt",
        |f| f.sqrt(),
        "`sqrt(x)` calculates the square root of a number, i.e. \\(y = \\sqrt{x}\\) such that \\(y^2 = x\\)",
    ));

    prepared.insert("sin", f64_f64(
        "sin",
        |f| f.sin(),
        concat!(
            "`sin(theta)` calculates the sine of an angle in radians, the ratio of the length of",
            " the leg opposite the angle in a triangle to its hypotenuse.",
        ),
    ));
    prepared.insert("cos", f64_f64(
        "cos",
        |f| f.cos(),
        concat!(
            "`cos(theta)` calculates the cosine of an angle in radians, the ratio of the length of",
            " the leg adjacent to the angle in a triangle to its hypotenuse.",
        ),
    ));
    prepared.insert("tan", f64_f64(
        "tan",
        |f| f.tan(),
        concat!(
            "`tan(theta)` calculates the tangent of an angle in radians, the ratio of the length of",
            " the leg opposite the angle in a triangle to the leg adjacent to the angle.",
        ),
    ));
    prepared.insert("exp", f64_f64(
        "exp",
        |f| f.exp(),
        concat!(
            "`exp(x)` calculates the exponential function, the function where",
            " \\(\\text{exp}(0) = 1\\) and",
            " \\(\\frac{\\text{d}}{\\text{d}x}\\text{exp}(x) = \\text{exp}(x)\\). `exp(x)` is",
            " equivalent to `e**x`.",
        ),
    ));
    prepared.insert("asin", f64_f64(
        "asin",
        |f| f.asin(),
        concat!(
            "`asin(x)` calculates the inverse sine, the angle in the triangle with the given ratio",
            " of the length of the leg opposite the angle to the hypotenuse. The result is",
            " returned in radians because mathematicians hate people.",
        ),
    ));
    prepared.insert("acos", f64_f64(
        "acos",
        |f| f.acos(),
        concat!(
            "`acos(x)` calculates the inverse cosine, the angle in the triangle with the given",
            " ratio of the length of the leg adjacent to the angle to the hypotenuse. The result",
            " is returned in radians because mathematicians hate people.",
        ),
    ));
    prepared.insert("atan", f64_f64(
        "atan",
        |f| f.atan(),
        concat!(
            "`atan(x)` calculates the inverse tangent, the angle in the triangle with the given",
            " ratio of the length of the leg opposite to the angle to the leg adjacend to the",
            " angle. The result is returned in radians because mathematicians hate people. Note",
            " that the angle will always be returned in the range of \\(-\\frac{\\pi}{2}\\) to",
            " \\(\\frac{\\pi}{2}\\); to calculate the inverse tangent in a way that respects the",
            " four quadrants, see `atan2(y, x)`.",
        ),
    ));
    prepared.insert("atan2", f64_f64_f64(
        "atan2",
        |f, g| f.atan2(g),
        concat!(
            "`atan2(y, x)` calculates the inverse tangent, the angle in the triangle with the",
            " given ratio of the length of the leg opposite to the angle (`y`) and the leg",
            " adjacent to it (`x`). In contrast to `atan(y/x)`, `atan2` returns the angle in the",
            " correct quadrant depending on the signs of `x` and `y`.",
        ),
    ));
    prepared.insert("sinh", f64_f64(
        "sinh",
        |f| f.sinh(),
        "`sinh(x)` calculates the hyperbolic sine of `x`.",
    ));
    prepared.insert("cosh", f64_f64(
        "cosh",
        |f| f.cosh(),
        "`cosh(x)` calculates the hyperbolic cosine of `x`.",
    ));
    prepared.insert("tanh", f64_f64(
        "tanh",
        |f| f.tanh(),
        "`tanh(x)` calculates the hyperbolic tangent of `x`.",
    ));
    prepared.insert("ln", f64_f64(
        "ln",
        |f| f.ln(),
        concat!(
            "`ln(x)` calculates the natural logarithm, i.e. the logarithm with base \\(e\\), of",
            " `x`.",
        ),
    ));
    prepared.insert("log10", f64_f64(
        "log10",
        |f| f.log10(),
        concat!(
            "`log10(x)` calculates the decimal logarithm, i.e. the logarithm with base 10, of `x`.",
        ),
    ));
    prepared.insert("log", f64_f64_f64(
        "log",
        |f, g| f.log(g),
        concat!(
            "`log(x, b)` calculates the logarithm of `x` with base `b`.",
        ),
    ));
    prepared.insert("dms", f64_multi_f64(
        "dms",
        from_dms_array,
        concat!(
            "`dms(d, m, s)` converts a value in degrees `d`, minutes `m` and seconds `s` to a value",
            " in decimal degrees. Can also be used for time (substitute _hours_ for _degrees_).",
        ),
    ));
    prepared.insert("dm", f64_multi_f64(
        "dm",
        from_dm_array,
        concat!(
            "`dm(d, m)` converts a value in degrees `d` and minutes `m` to a value in decimal",
            " degrees. Can also be used for time (substitute _hours_ for _degrees_).",
        ),
    ));
    // the default for angles is radians because mathematicians hate their fellow humans
    // (the feeling is mutual)
    // let's be the change we want to see in the world
    prepared.insert("havsinrad", f64_multi_f64(
        "havsinrad",
        haversine_array,
        concat!(
            "`havsinrad(r, lat1, lon1, lat2, lon2)` calculates the distance between points",
            " `(lat1, lon1)` and `(lat2, lon2)` on a sphere with radius `r` using the haversine",
            " formula. The latitudes and longitudes must be in radians; see `havsin` for a",
            " degrees-based version.",
        ),
    ));
    prepared.insert("havsin", f64_multi_f64(
        "havsin",
        haversine_deg_array,
        concat!(
            "`havsin(r, lat1, lon1, lat2, lon2)` calculates the distance between points",
            " `(lat1, lon1)` and `(lat2, lon2)` on a sphere with radius `r` using the haversine",
            " formula. The latitudes and longitudes must be in degrees; see `havsinrad` for a",
            " degrees-based version. You may also be interested in the constant `earthR`.",
        ),
    ));
    prepared.insert("elldisrad", f64_multi_f64(
        "elldisrad",
        ellipsoid_distance_array,
        concat!(
            "`elldisrad(er, if, lat1, lon1, lat2, lon2)` calculates the distance between points",
            " `(lat1, lon1)` and `(lat2, lon2)` on an ellipsoid with equatorial radius `er` and",
            " inverse flattening `if`. The latitudes and longitudes must be in radians; see",
            " `elldis` for a degrees-based version.",
        ),
    ));
    prepared.insert("elldis", f64_multi_f64(
        "elldis",
        ellipsoid_distance_deg_array,
        concat!(
            "`elldis(er, if, lat1, lon1, lat2, lon2)` calculates the distance between points",
            " `(lat1, lon1)` and `(lat2, lon2)` on an ellipsoid with equatorial radius `er` and",
            " inverse flattening `if`. The latitudes and longitudes must be in degrees; see",
            " `elldisrad` for a radians-based version. You may also be interested in the constants",
            " `earthER` and `earthIF`.",
        ),
    ));

    prepared.insert("ceil", f64_f64asint(
        "ceil",
        |f| f.ceil(),
        "`ceil(x)` returns `x` rounded up (towards \\(\\infty\\)).",
    ));
    prepared.insert("floor", f64_f64asint(
        "floor",
        |f| f.floor(),
        "`floor(x)` returns `x` rounded down (towards \\(-\\infty\\)).",
    ));
    prepared.insert("round", f64_f64asint(
        "round",
        |f| f.round(),
        concat!(
            "`round(x)` returns `x` rounded _half away from zero_ (the most commonly used",
            " tie-breaking rule).",
        ),
    ));
    prepared.insert("trunc", f64_f64asint(
        "trunc",
        |f| f.trunc(),
        concat!(
            "`trunc(x)` returns `x` rounded towards 0 (equivalent to stripping away all fractional",
            " digits).",
        ),
    ));

    prepared.insert("coerce", Function::new(
        Box::new(coerce),
        concat!(
            "`coerce(x, u)` attempts to convert the value `x` into the units of value `u`. The",
            " numeric value of `u` does not matter; only its units are taken into account. To",
            " attach units to a value, see `setunit`.",
        ),
    ));
    prepared.insert("setunit", Function::new(
        Box::new(set_unit),
        concat!(
            "`setunit(x, u)` returns the numeric value of `x` attached to the units of `u`. No",
            " calculation or unit conversion is performed; see `coerce` for that behavior.",
        ),
    ));
    prepared.insert("baseunits", Function::new(
        Box::new(to_base_units),
        concat!(
            "`baseunits(x)` converts the given value into base units (which are mostly SI base",
            " units).",
        ),
    ));
    prepared.insert("c2f", f64_f64(
        "c2f",
        |f| f * 9.0/5.0 + 32.0,
        "`c2f(t)` converts the temperature `t` from Celsius to Fahrenheit.",
    ));
    prepared.insert("f2c", f64_f64(
        "f2c",
        |f| (f - 32.0) * 5.0/9.0,
        "`f2c(t)` converts the temperature `t` from Fahrenheit to Celsius.",
    ));

    prepared.drain()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}


fn check_arg_count(name: &'static str, expected: usize, obtained: usize) -> Result<(), SimplificationError> {
    if expected != obtained {
        Err(SimplificationError::IncorrectArgCount {
            function_name: name.to_owned(),
            expected,
            obtained,
        })
    } else {
        Ok(())
    }
}


fn f64_f64<F>(name: &'static str, inner: F, help_text: &'static str) -> Function
    where F: Fn(f64) -> f64 + 'static
{
    Function::new(
        Box::new(move |_state, operands| {
            check_arg_count(name, 1, operands.len())?;

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
        }),
        help_text,
    )
}


fn f64_f64asint<F>(name: &'static str, inner: F, help_text: &'static str) -> Function
    where F: Fn(f64) -> f64 + 'static
{
    Function::new(
        Box::new(move |_state, operands| {
            check_arg_count(name, 1, operands.len())?;

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
        }),
        help_text,
    )
}


fn f64_f64_f64<F>(name: &'static str, inner: F, help_text: &'static str) -> Function
    where F: Fn(f64, f64) -> f64 + 'static
{
    Function::new(
        Box::new(move |_state, operands| {
            check_arg_count(name, 2, operands.len())?;

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
        }),
        help_text,
    )
}

fn f64_multi_f64<F, const ARG_COUNT: usize>(name: &'static str, inner: F, help_text: &'static str) -> Function
    where F: Fn([f64; ARG_COUNT]) -> f64 + 'static
{
    Function::new(
        Box::new(move |_state, operands| {
            check_arg_count(name, ARG_COUNT, operands.len())?;

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
        }),
        help_text,
    )
}

fn from_dms_array(dms: [f64; 3]) -> f64 {
    let d = dms[0];
    let min = dms[1];
    let s = dms[2];

    d + (min / 60.0) + (s / (60.0 * 60.0))
}

fn from_dm_array(dms: [f64; 2]) -> f64 {
    let d = dms[0];
    let min = dms[1];

    d + (min / 60.0)
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
    check_arg_count("coerce", 2, operands.len())?;

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
    check_arg_count("setunit", 2, operands.len())?;

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
    check_arg_count("baseunits", 1, operands.len())?;

    let number = match &operands[0].node {
        AstNode::Number(n) => n,
        other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other))),
    };

    let result = coerce_to_base_units(number, &state.units);

    Ok(AstNode::Number(result))
}
