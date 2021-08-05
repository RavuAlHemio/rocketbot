use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use num_bigint::BigInt;
use num_traits::cast::ToPrimitive;

use crate::numbers::{Number, NumberOperationError, NumberValue};
use crate::units::{NumberUnits, UnitDatabase};


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BinaryOperation {
    Power,
    Multiply,
    Divide,
    DivideIntegral,
    Remainder,
    Add,
    Subtract,
    BinaryAnd,
    BinaryOr,
    BinaryXor,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum UnaryOperation {
    Factorial,
    Negate,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AstNode {
    Number(Number),
    Constant(String),
    FunctionCall(String, Vec<AstNodeAtLocation>),
    BinaryOperation(BinaryOperation, Box<AstNodeAtLocation>, Box<AstNodeAtLocation>),
    UnaryOperation(UnaryOperation, Box<AstNodeAtLocation>),
}
impl From<f64> for AstNode {
    fn from(f: f64) -> Self {
        AstNode::Number(Number::from(NumberValue::from(f)))
    }
}
impl From<BigInt> for AstNode {
    fn from(i: BigInt) -> Self {
        AstNode::Number(Number::from(NumberValue::from(i)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AstNodeAtLocation {
    pub node: AstNode,
    pub start_end: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SimplificationError {
    ConstantNotFound(String),
    FunctionNotFound(String),
    IncorrectArgCount(String, usize, usize),
    UnexpectedOperandType(String),
    NonIntegralValue(f64),
    Timeout,
    OperationFailed,
    RightOperandHasUnits,
    LeftOperandUnitsRightOperandFloat,
    OperandHasUnits,
    UnitReconciliation,
}
impl SimplificationError {
    pub fn at_location(self, start_end: Option<(usize, usize)>) -> SimplificationErrorAtLocation {
        SimplificationErrorAtLocation {
            error: self,
            start_end,
        }
    }
    pub fn at_location_of(self, node: &AstNodeAtLocation) -> SimplificationErrorAtLocation {
        SimplificationErrorAtLocation {
            error: self,
            start_end: node.start_end,
        }
    }
}
impl fmt::Display for SimplificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimplificationError::ConstantNotFound(c)
                => write!(f, "constant {:?} not found", c),
            SimplificationError::FunctionNotFound(n)
                => write!(f, "function {:?} not found", n),
            SimplificationError::IncorrectArgCount(n, expected, got)
                => write!(f, "{} arguments given to function {:?} which expects {} arguments", expected, n, got),
            SimplificationError::UnexpectedOperandType(t)
                => write!(f, "operand type {} unexpected", t),
            SimplificationError::NonIntegralValue(fv)
                => write!(f, "value {} cannot be represented as an integer", fv),
            SimplificationError::Timeout
                => write!(f, "timed out"),
            SimplificationError::OperationFailed
                => write!(f, "operation failed"),
            SimplificationError::RightOperandHasUnits
                => write!(f, "right operand has units; it mustn't"),
            SimplificationError::LeftOperandUnitsRightOperandFloat
                => write!(f, "left operand has units but the right operand is floating-point"),
            SimplificationError::OperandHasUnits
                => write!(f, "operand has units; it mustn't"),
            SimplificationError::UnitReconciliation
                => write!(f, "failed to reconcile operand units"),
        }
    }
}
impl std::error::Error for SimplificationError {
}
impl From<NumberOperationError> for SimplificationError {
    fn from(noe: NumberOperationError) -> Self {
        match noe {
            NumberOperationError::OperationFailed => Self::OperationFailed,
            NumberOperationError::UnitReconciliation => Self::UnitReconciliation,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SimplificationErrorAtLocation {
    pub error: SimplificationError,
    pub start_end: Option<(usize, usize)>,
}
impl fmt::Display for SimplificationErrorAtLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((s, e)) = self.start_end {
            write!(f, "{} from {} to {}", self.error, s, e)
        } else {
            write!(f, "{}", self.error)
        }
    }
}
impl std::error::Error for SimplificationErrorAtLocation {
}

pub(crate) type SimplificationResult = Result<AstNodeAtLocation, SimplificationErrorAtLocation>;
pub(crate) type BuiltInFuncResult = Result<AstNode, SimplificationError>;
pub(crate) type BuiltInFunction = Box<dyn Fn(&SimplificationState, &[AstNodeAtLocation]) -> BuiltInFuncResult>;

pub(crate) struct SimplificationState {
    pub constants: HashMap<String, AstNode>,
    pub functions: HashMap<String, BuiltInFunction>,
    pub units: UnitDatabase,
    pub start_time: Instant,
    pub timeout: Duration,
}


fn perform_binop<O>(start_end: Option<(usize, usize)>, left: &AstNodeAtLocation, right: &AstNodeAtLocation, mut op: O) -> SimplificationResult
    where
        O: FnMut(&Number, &Number) -> Result<Number, NumberOperationError>,
{
    let calculated: AstNode = if let AstNode::Number(l) = &left.node {
        if let AstNode::Number(r) = &right.node {
            op(l, r)
                .map(|res| AstNode::Number(res))
                .map_err(|noe| SimplificationError::from(noe).at_location(start_end))?
        } else {
            return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", right.node)).at_location_of(right));
        }
    } else {
        return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", left.node)).at_location_of(left));
    };
    Ok(AstNodeAtLocation {
        node: calculated,
        start_end,
    })
}


fn perform_integral_only<B>(start_end: Option<(usize, usize)>, left: &AstNodeAtLocation, right: &AstNodeAtLocation, mut bigint_op: B) -> SimplificationResult
    where
        B: FnMut(&Number, &Number) -> Result<Number, NumberOperationError>,
{
    let calculated: AstNode = match &left.node {
        AstNode::Number(lnum) => {
            if let NumberValue::Int(_l) = &lnum.value {
                match &right.node {
                    AstNode::Number(rnum) => {
                        if let NumberValue::Int(_r) = &rnum.value {
                            let result = bigint_op(&lnum, &rnum)
                                .map_err(|e| SimplificationError::from(e).at_location(start_end))?;
                            AstNode::Number(result)
                        } else {
                            return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", right.node)).at_location_of(right));
                        }
                    },
                    _other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", right.node)).at_location_of(right)),
                }
            } else {
                return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", left.node)).at_location_of(left));
            }
        },
        _other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", left.node)).at_location_of(left)),
    };
    Ok(AstNodeAtLocation {
        node: calculated,
        start_end,
    })
}


fn pow(start_end: Option<(usize, usize)>, left: &AstNodeAtLocation, right: &AstNodeAtLocation, state: &mut SimplificationState) -> SimplificationResult {
    let calculated: AstNode = match &left.node {
        AstNode::Number(lnum) => {
            match &right.node {
                AstNode::Number(rnum) => {
                    if rnum.units.len() > 0 {
                        return Err(SimplificationError::RightOperandHasUnits.at_location(start_end));
                    }

                    match (&lnum.value, &rnum.value) {
                        (NumberValue::Int(l), NumberValue::Int(r)) => {
                            let one = BigInt::from(1);
                            let mut val = one.clone();
                            let mut counter = BigInt::from(0);
                            while counter < *r {
                                val *= l;
                                counter += &one;
                                check_timeout(state)?;
                            }
                            AstNode::Number(Number::new(
                                NumberValue::Int(val),
                                lnum.units.clone(),
                            ))
                        },
                        (NumberValue::Int(l), NumberValue::Float(r)) => {
                            if lnum.units.len() > 0 {
                                return Err(SimplificationError::LeftOperandUnitsRightOperandFloat.at_location(start_end));
                            }

                            let l_f64 = l.to_f64().expect("conversion failed");
                            AstNode::Number(Number::new(
                                NumberValue::Float(l_f64.powf(*r)),
                                NumberUnits::new(),
                            ))
                        },
                        (NumberValue::Float(l), NumberValue::Int(r)) => {
                            let r_f64 = r.to_f64().expect("conversion failed");

                            // multiply unit powers
                            let mut new_units = NumberUnits::new();
                            for (unit, power) in &lnum.units {
                                let new_unit_power = power * r;
                                new_units.insert(
                                    unit.clone(),
                                    new_unit_power,
                                );
                            }

                            AstNode::Number(Number::new(
                                NumberValue::Float(l.powf(r_f64)),
                                new_units,
                            ))
                        },
                        (NumberValue::Float(l), NumberValue::Float(r)) => {
                            if lnum.units.len() > 0 {
                                return Err(SimplificationError::LeftOperandUnitsRightOperandFloat.at_location(start_end));
                            }
                            AstNode::Number(Number::new(
                                NumberValue::Float(l.powf(*r)),
                                NumberUnits::new(),
                            ))
                        },
                    }
                },
                _other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", right.node)).at_location_of(right)),
            }
        },
        _other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", left.node)).at_location_of(left)),
    };
    Ok(AstNodeAtLocation {
        node: calculated,
        start_end,
    })
}


fn check_timeout(state: &SimplificationState) -> Result<(), SimplificationErrorAtLocation> {
    let runtime = Instant::now() - state.start_time;
    if runtime >= state.timeout {
        Err(SimplificationErrorAtLocation {
            error: SimplificationError::Timeout,
            start_end: None,
        })
    } else {
        Ok(())
    }
}


impl AstNodeAtLocation {
    pub fn simplify(&self, mut state: &mut SimplificationState) -> SimplificationResult {
        check_timeout(state)?;

        match &self.node {
            AstNode::Number(_) => Ok(self.clone()),
            AstNode::Constant(name) => {
                match state.constants.get(name) {
                    None => Err(SimplificationError::ConstantNotFound(name.clone()).at_location_of(self)),
                    Some(c) => Ok(AstNodeAtLocation {
                        node: c.clone(),
                        start_end: self.start_end,
                    }),
                }
            },
            AstNode::FunctionCall(name, args) => {
                if !state.functions.contains_key(name) {
                    return Err(SimplificationError::FunctionNotFound(name.clone()).at_location_of(self));
                }

                let mut simplified_args: Vec<AstNodeAtLocation> = Vec::with_capacity(args.len());
                for arg in args {
                    simplified_args.push(arg.simplify(state)?);
                }

                let func = state.functions.get(name).unwrap();
                match func(&state, &simplified_args) {
                    Ok(node) => Ok(AstNodeAtLocation {
                        node,
                        start_end: self.start_end,
                    }),
                    Err(error) => Err(SimplificationErrorAtLocation {
                        error,
                        start_end: self.start_end,
                    }),
                }
            },
            AstNode::BinaryOperation(binop, left, right) => {
                let left_simp = left.simplify(state)?;
                let right_simp = right.simplify(state)?;
                match binop {
                    BinaryOperation::Add => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_add(r.clone(), &state.units))
                    },
                    BinaryOperation::BinaryAnd => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| l.checked_bit_and(r.clone(), &state.units))
                    },
                    BinaryOperation::BinaryOr => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| l.checked_bit_or(r.clone(), &state.units))
                    },
                    BinaryOperation::BinaryXor => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| l.checked_bit_xor(r.clone(), &state.units))
                    },
                    BinaryOperation::Multiply => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_mul(r.clone()))
                    },
                    BinaryOperation::Power => {
                        pow(self.start_end, &left_simp, &right_simp, &mut state)
                    },
                    BinaryOperation::Remainder => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_rem(r.clone()))
                    },
                    BinaryOperation::Subtract => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_sub(r.clone(), &state.units))
                    },
                    BinaryOperation::Divide => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_div(r.clone()))
                    },
                    BinaryOperation::DivideIntegral => {
                        perform_binop(self.start_end, &left_simp, &right_simp, |l, r| l.checked_whole_div(r.clone()))
                    },
                }
            },
            AstNode::UnaryOperation(unop, operand) => {
                let inner_op = operand.simplify(state)?;
                match unop {
                    UnaryOperation::Negate => {
                        let node = match &inner_op.node {
                            AstNode::Number(o) => AstNode::Number(o.negated()),
                            other => return Err(SimplificationError::UnexpectedOperandType(format!("{:?}", other)).at_location_of(operand)),
                        };
                        Ok(AstNodeAtLocation {
                            node,
                            start_end: self.start_end,
                        })
                    },
                    UnaryOperation::Factorial => {
                        if let AstNode::Number(onum) = &inner_op.node {
                            if onum.units.len() > 0 {
                                return Err(SimplificationError::OperandHasUnits.at_location_of(operand));
                            }
                            if let NumberValue::Int(o) = &onum.value {
                                let mut i = BigInt::from(2);
                                let one = BigInt::from(1);
                                let mut val = one.clone();
                                while i < *o {
                                    val *= &i;
                                    i += &one;
                                    check_timeout(state)?;
                                }
                                Ok(AstNodeAtLocation {
                                    node: AstNode::Number(Number::new(
                                        NumberValue::Int(val),
                                        NumberUnits::new(),
                                    )),
                                    start_end: self.start_end,
                                })
                            } else {
                                Err(SimplificationError::UnexpectedOperandType(format!("{:?}", inner_op)).at_location_of(operand))
                            }
                        } else {
                            Err(SimplificationError::UnexpectedOperandType(format!("{:?}", inner_op)).at_location_of(operand))
                        }
                    },
                }
            },
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::grimoire::{get_canonical_constants, get_canonical_functions};
    use crate::parsing::parse_full_expression;

    fn run_test(expected: &str, parse_me: &str) {
        let parsed = parse_full_expression(parse_me).unwrap();
        let mut state = SimplificationState {
            constants: get_canonical_constants(),
            functions: get_canonical_functions(),
            units: UnitDatabase::new_empty(),
            start_time: Instant::now(),
            timeout: Duration::from_secs(10),
        };
        let result = parsed.simplify(&mut state).unwrap();
        let obtained = match result.node {
            AstNode::Number(i) => i.to_string(),
            other => panic!("unexpected AST node {:?}", other),
        };
        assert_eq!(expected, obtained);
    }

    #[test]
    fn test_precedence_mul_add() {
        run_test("10", "2 * 3 + 4");
        run_test("14", "2 + 3 * 4");
    }

    #[test]
    fn test_associativity_sub_sub() {
        run_test("2", "7 - 4 - 1");
    }

    #[test]
    fn test_associativity_mul_div() {
        run_test("2.25", "3/2*3/2");
    }

    #[test]
    fn test_associativity_pow_pow() {
        // right-associative (2**(3**3))
        run_test("134217728", "2**3**3");
    }
}
