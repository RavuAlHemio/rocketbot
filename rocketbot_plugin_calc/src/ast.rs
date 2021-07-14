use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use num_bigint::{BigInt, ToBigInt};
use num_traits::cast::ToPrimitive;


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
    Int(BigInt),
    Float(f64),
    Constant(String),
    FunctionCall(String, Vec<AstNodeAtLocation>),
    BinaryOperation(BinaryOperation, Box<AstNodeAtLocation>, Box<AstNodeAtLocation>),
    UnaryOperation(UnaryOperation, Box<AstNodeAtLocation>),
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
            &SimplificationError::Timeout
                => write!(f, "timed out"),
        }
    }
}
impl std::error::Error for SimplificationError {
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
pub(crate) type BuiltInFunction = Box<dyn FnMut(&[AstNodeAtLocation]) -> BuiltInFuncResult>;

pub(crate) struct SimplificationState {
    pub constants: HashMap<String, AstNode>,
    pub functions: HashMap<String, BuiltInFunction>,
    pub start_time: Instant,
    pub timeout: Duration,
}


fn perform_binop_coerce<B, F>(start_end: Option<(usize, usize)>, left: &AstNodeAtLocation, right: &AstNodeAtLocation, mut bigint_op: B, mut float_op: F) -> SimplificationResult
    where
        B: FnMut(&BigInt, &BigInt) -> Result<BigInt, SimplificationErrorAtLocation>,
        F: FnMut(f64, f64) -> Result<f64, SimplificationErrorAtLocation>,
{
    let calculated: AstNode = match &left.node {
        AstNode::Int(l) => {
            match &right.node {
                AstNode::Int(r) => {
                    AstNode::Int(bigint_op(&l, &r)?)
                },
                AstNode::Float(r) => {
                    AstNode::Float(float_op(l.to_f64().expect("conversion failed"), *r)?)
                },
                other => return Err(right.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
            }
        },
        AstNode::Float(l) => {
            match &right.node {
                AstNode::Int(r) => {
                    AstNode::Float(float_op(*l, r.to_f64().expect("conversion failed"))?)
                },
                AstNode::Float(r) => {
                    AstNode::Float(float_op(*l, *r)?)
                },
                other => return Err(right.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
            }
        },
        other => return Err(left.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
    };
    Ok(AstNodeAtLocation {
        node: calculated,
        start_end,
    })
}


fn perform_integral_only<B>(start_end: Option<(usize, usize)>, left: &AstNodeAtLocation, right: &AstNodeAtLocation, mut bigint_op: B) -> SimplificationResult
    where
        B: FnMut(&BigInt, &BigInt) -> Result<BigInt, SimplificationErrorAtLocation>,
{
    let calculated: AstNode = match &left.node {
        AstNode::Int(l) => {
            match &right.node {
                AstNode::Int(r) => {
                    AstNode::Int(bigint_op(&l, &r)?)
                },
                other => return Err(right.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
            }
        },
        other => return Err(left.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
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
    fn make_error(&self, error_kind: SimplificationError) -> SimplificationErrorAtLocation {
        SimplificationErrorAtLocation {
            error: error_kind,
            start_end: self.start_end,
        }
    }

    pub fn simplify(&self, state: &mut SimplificationState) -> SimplificationResult {
        check_timeout(state)?;

        match &self.node {
            AstNode::Int(_) => Ok(self.clone()),
            AstNode::Float(_) => Ok(self.clone()),
            AstNode::Constant(name) => {
                match state.constants.get(name) {
                    None => Err(self.make_error(SimplificationError::ConstantNotFound(name.clone()))),
                    Some(c) => Ok(AstNodeAtLocation {
                        node: c.clone(),
                        start_end: self.start_end,
                    }),
                }
            },
            AstNode::FunctionCall(name, args) => {
                if !state.functions.contains_key(name) {
                    return Err(self.make_error(SimplificationError::FunctionNotFound(name.clone())));
                }

                let mut simplified_args: Vec<AstNodeAtLocation> = Vec::with_capacity(args.len());
                for arg in args {
                    simplified_args.push(arg.simplify(state)?);
                }

                let func = state.functions.get_mut(name).unwrap();
                match func(&simplified_args) {
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
                        perform_binop_coerce(self.start_end, &left_simp, &right_simp, |l, r| Ok(l + r), |l, r| Ok(l + r))
                    },
                    BinaryOperation::BinaryAnd => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| Ok(l & r))
                    },
                    BinaryOperation::BinaryOr => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| Ok(l | r))
                    },
                    BinaryOperation::BinaryXor => {
                        perform_integral_only(self.start_end, &left_simp, &right_simp, |l, r| Ok(l ^ r))
                    },
                    BinaryOperation::Multiply => {
                        perform_binop_coerce(self.start_end, &left_simp, &right_simp, |l, r| Ok(l * r), |l, r| Ok(l * r))
                    },
                    BinaryOperation::Power => {
                        perform_binop_coerce(
                            self.start_end, &left_simp, &right_simp,
                            |l, r| {
                                let one = BigInt::from(1);
                                let mut val = one.clone();
                                let mut counter = BigInt::from(0);
                                while counter < *r {
                                    val *= l;
                                    counter += &one;
                                    check_timeout(state)?;
                                }
                                Ok(val)
                            },
                            |l, r| Ok(l.powf(r)),
                        )
                    },
                    BinaryOperation::Remainder => {
                        perform_binop_coerce(self.start_end, &left_simp, &right_simp, |l, r| Ok(l % r), |l, r| Ok(l % r))
                    },
                    BinaryOperation::Subtract => {
                        perform_binop_coerce(self.start_end, &left_simp, &right_simp, |l, r| Ok(l - r), |l, r| Ok(l - r))
                    },
                    BinaryOperation::Divide => {
                        let left_f64 = match &left_simp.node {
                            AstNode::Int(l) => l.to_f64().expect("conversion failed"),
                            AstNode::Float(l) => *l,
                            other => return Err(left_simp.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
                        };
                        let right_f64 = match &right_simp.node {
                            AstNode::Int(r) => r.to_f64().expect("conversion failed"),
                            AstNode::Float(r) => *r,
                            other => return Err(right_simp.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
                        };
                        Ok(AstNodeAtLocation {
                            node: AstNode::Float(left_f64 / right_f64),
                            start_end: self.start_end,
                        })
                    },
                    BinaryOperation::DivideIntegral => {
                        let left_bigint = match &left_simp.node {
                            AstNode::Int(l) => l.clone(),
                            AstNode::Float(l) => l.to_bigint()
                                .ok_or(left_simp.make_error(SimplificationError::NonIntegralValue(*l)))?,
                            other => return Err(left_simp.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
                        };
                        let right_bigint = match &right_simp.node {
                            AstNode::Int(r) => r.clone(),
                            AstNode::Float(r) => r.to_bigint()
                                .ok_or(right_simp.make_error(SimplificationError::NonIntegralValue(*r)))?,
                            other => return Err(right_simp.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
                        };
                        Ok(AstNodeAtLocation {
                            node: AstNode::Int(left_bigint / right_bigint),
                            start_end: self.start_end,
                        })
                    },
                }
            },
            AstNode::UnaryOperation(unop, operand) => {
                let inner_op = operand.simplify(state)?;
                match unop {
                    UnaryOperation::Negate => {
                        let node = match &inner_op.node {
                            AstNode::Int(o) => AstNode::Int(-o),
                            AstNode::Float(o) => AstNode::Float(-o),
                            other => return Err(operand.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", other)))),
                        };
                        Ok(AstNodeAtLocation {
                            node,
                            start_end: self.start_end,
                        })
                    },
                    UnaryOperation::Factorial => {
                        if let AstNode::Int(o) = inner_op.node {
                            let mut i = BigInt::from(2);
                            let one = BigInt::from(1);
                            let mut val = one.clone();
                            while i < o {
                                val *= &i;
                                i += &one;
                                check_timeout(state)?;
                            }
                            Ok(AstNodeAtLocation {
                                node: AstNode::Int(val),
                                start_end: self.start_end,
                            })
                        } else {
                            Err(operand.make_error(SimplificationError::UnexpectedOperandType(format!("{:?}", inner_op))))
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
            start_time: Instant::now(),
            timeout: Duration::from_secs(10),
        };
        let result = parsed.simplify(&mut state).unwrap();
        let obtained = match result.node {
            AstNode::Int(i) => i.to_string(),
            AstNode::Float(f) => f.to_string(),
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
