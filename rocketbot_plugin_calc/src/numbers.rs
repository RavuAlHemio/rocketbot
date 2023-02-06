use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign};

use num_bigint::{BigInt, ToBigInt};
use num_traits::ToPrimitive;

use crate::units::{coerce_to_common_unit, NumberUnits, UnitDatabase};


trait WholeDiv {
    type Output;
    fn whole_div(self, rhs: Self) -> Self::Output;
}


#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NumberValue {
    Int(BigInt),
    Float(f64),
}
impl NumberValue {
    fn bin_op<I, F, O>(
        &self,
        other: &Self,
        mut int_op: I,
        mut float_op: F,
    ) -> Option<O>
        where
            I: FnMut(&BigInt, &BigInt) -> Option<O>,
            F: FnMut(f64, f64) -> Option<O>,
    {
        match (self, other) {
            (Self::Int(s), Self::Int(o)) => {
                int_op(s, o)
            },
            (Self::Int(s), Self::Float(o)) => {
                let s_f64: f64 = match s.to_f64() {
                    Some(sf) => sf,
                    None => return None,
                };
                float_op(s_f64, *o)
            },
            (Self::Float(s), Self::Int(o)) => {
                let o_f64: f64 = match o.to_f64() {
                    Some(of) => of,
                    None => return None,
                };
                float_op(*s, o_f64)
            },
            (Self::Float(s), Self::Float(o)) => {
                float_op(*s, *o)
            },
        }
    }

    pub fn checked_add(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| s.checked_add(r).map(|x| NumberValue::Int(x)),
            |s, r| Some(NumberValue::Float(s + r)),
        )
    }

    pub fn checked_bit_and(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| Some(NumberValue::Int(s & r)),
            |_s, _r| None,
        )
    }

    pub fn checked_bit_or(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| Some(NumberValue::Int(s | r)),
            |_s, _r| None,
        )
    }

    pub fn checked_bit_xor(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| Some(NumberValue::Int(s ^ r)),
            |_s, _r| None,
        )
    }

    pub fn checked_sub(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| s.checked_sub(r).map(|x| NumberValue::Int(x)),
            |s, r| Some(NumberValue::Float(s - r)),
        )
    }

    pub fn checked_mul(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, r| s.checked_mul(r).map(|x| NumberValue::Int(x)),
            |s, r| Some(NumberValue::Float(s * r)),
        )
    }

    pub fn checked_div(&self, rhs: Self) -> Option<Self> {
        // coerce to f64
        let s_f64: f64 = match self {
            Self::Int(s) => s.to_f64()?,
            Self::Float(s) => *s,
        };
        let r_f64: f64 = match rhs {
            Self::Int(r) => r.to_f64()?,
            Self::Float(r) => r,
        };
        Some(NumberValue::Float(s_f64 / r_f64))
    }

    pub fn checked_whole_div(&self, rhs: Self) -> Option<Self> {
        // coerce to BigInt
        let s_bi: BigInt = match self {
            Self::Int(s) => s.clone(),
            Self::Float(s) => s.to_bigint()?,
        };
        let r_bi: BigInt = match rhs {
            Self::Int(r) => r,
            Self::Float(r) => r.to_bigint()?,
        };
        Some(NumberValue::Int(s_bi / r_bi))
    }

    pub fn checked_rem(&self, rhs: Self) -> Option<Self> {
        self.bin_op(
            &rhs,
            |s, o| Some(NumberValue::Int(s % o)),
            |s, o| Some(NumberValue::Float(s % o)),
        )
    }
}
impl PartialOrd for NumberValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.bin_op(
            other,
            |s, o| s.partial_cmp(o),
            |s, o| s.partial_cmp(&o),
        )
    }
}
impl Add for NumberValue {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(rhs).expect("addition failed")
    }
}
impl AddAssign for NumberValue {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.checked_add(rhs).expect("addition failed");
    }
}
impl Div for NumberValue {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        self.checked_div(rhs).expect("division failed")
    }
}
impl DivAssign for NumberValue {
    fn div_assign(&mut self, rhs: Self) {
        *self = self.checked_div(rhs).expect("division failed");
    }
}
impl Mul for NumberValue {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        self.checked_mul(rhs).expect("multiplication failed")
    }
}
impl MulAssign for NumberValue {
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.checked_mul(rhs).expect("multiplication failed");
    }
}
impl Neg for NumberValue {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Self::Int(s) => Self::Int(-s),
            Self::Float(s) => Self::Float(-s),
        }
    }
}
impl Rem for NumberValue {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        self.checked_rem(rhs).expect("remainder calculation failed")
    }
}
impl RemAssign for NumberValue {
    fn rem_assign(&mut self, rhs: Self) {
        *self = self.checked_rem(rhs).expect("remainder calculation failed");
    }
}
impl Sub for NumberValue {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(rhs).expect("subtraction failed")
    }
}
impl SubAssign for NumberValue {
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.checked_sub(rhs).expect("subtraction failed");
    }
}
impl WholeDiv for NumberValue {
    type Output = Self;

    fn whole_div(self, rhs: Self) -> Self::Output {
        self.checked_div(rhs).expect("whole-number division failed")
    }
}
impl fmt::Display for NumberValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => v.fmt(f),
            Self::Float(v) => v.fmt(f),
        }
    }
}
impl From<f64> for NumberValue {
    fn from(f: f64) -> Self {
        NumberValue::Float(f)
    }
}
impl From<BigInt> for NumberValue {
    fn from(i: BigInt) -> Self {
        NumberValue::Int(i)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NumberOperationError {
    OperationFailed,
    UnitReconciliation,
}
impl fmt::Display for NumberOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OperationFailed => write!(f, "operation failed"),
            Self::UnitReconciliation => write!(f, "failed to reconcile operand units"),
        }
    }
}
impl std::error::Error for NumberOperationError {
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Number {
    pub value: NumberValue,
    pub units: NumberUnits,
}
impl Number {
    pub fn new(
        value: NumberValue,
        units: NumberUnits,
    ) -> Self {
        assert!(units.values().all(|power| power != &BigInt::from(0)));

        Self {
            value,
            units,
        }
    }

    pub fn checked_add(&self, rhs: Self, database: &UnitDatabase) -> Result<Self, NumberOperationError> {
        // coerce to same unit
        let (self_co, rhs_co) = coerce_to_common_unit(&self, &rhs, database)
            .ok_or(NumberOperationError::UnitReconciliation)?;
        debug_assert_eq!(self_co.units, rhs_co.units);

        let new_value = self_co.value.checked_add(rhs_co.value)
            .ok_or(NumberOperationError::OperationFailed)?;
        Ok(Number::new(new_value, self_co.units))
    }

    pub fn checked_bit_and(&self, rhs: Self, database: &UnitDatabase) -> Result<Self, NumberOperationError> {
        // coerce to same unit
        let (self_co, rhs_co) = coerce_to_common_unit(&self, &rhs, database)
            .ok_or(NumberOperationError::UnitReconciliation)?;
        debug_assert_eq!(self_co.units, rhs_co.units);

        let new_value = self_co.value.checked_bit_and(rhs_co.value)
            .ok_or(NumberOperationError::OperationFailed)?;
        Ok(Number::new(new_value, self_co.units))
    }

    pub fn checked_bit_or(&self, rhs: Self, database: &UnitDatabase) -> Result<Self, NumberOperationError> {
        // coerce to same unit
        let (self_co, rhs_co) = coerce_to_common_unit(&self, &rhs, database)
            .ok_or(NumberOperationError::UnitReconciliation)?;
        debug_assert_eq!(self_co.units, rhs_co.units);

        let new_value = self_co.value.checked_bit_or(rhs_co.value)
            .ok_or(NumberOperationError::OperationFailed)?;
        Ok(Number::new(new_value, self_co.units))
    }

    pub fn checked_bit_xor(&self, rhs: Self, database: &UnitDatabase) -> Result<Self, NumberOperationError> {
        // coerce to same unit
        let (self_co, rhs_co) = coerce_to_common_unit(&self, &rhs, database)
            .ok_or(NumberOperationError::UnitReconciliation)?;
        debug_assert_eq!(self_co.units, rhs_co.units);

        let new_value = self_co.value.checked_bit_xor(rhs_co.value)
            .ok_or(NumberOperationError::OperationFailed)?;
        Ok(Number::new(new_value, self_co.units))
    }

    pub fn checked_sub(&self, rhs: Self, database: &UnitDatabase) -> Result<Self, NumberOperationError> {
        // coerce to same unit
        let (self_co, rhs_co) = coerce_to_common_unit(&self, &rhs, database)
            .ok_or(NumberOperationError::UnitReconciliation)?;
        debug_assert_eq!(self_co.units, rhs_co.units);

        let new_value = self_co.value.checked_sub(rhs_co.value)
            .ok_or(NumberOperationError::OperationFailed)?;
        Ok(Number::new(new_value, self_co.units))
    }

    fn addsub_units<F: FnMut(BigInt) -> BigInt>(lhs_units: &NumberUnits, rhs_units: NumberUnits, mut transform_rhs: F) -> NumberUnits {
        let mut new_units = NumberUnits::new();
        for (self_unit, self_pow) in lhs_units {
            if let Some(rhs_pow) = rhs_units.get(self_unit) {
                let new_pow = self_pow + transform_rhs(rhs_pow.clone());
                new_units.insert(self_unit.clone(), new_pow);
            } else {
                new_units.insert(self_unit.clone(), self_pow.clone());
            }
        }
        for (rhs_unit, rhs_pow) in rhs_units {
            if new_units.contains_key(&rhs_unit) {
                // already taken care of
                continue;
            }
            new_units.insert(rhs_unit, transform_rhs(rhs_pow));
        }
        new_units.retain(|_unit, power| power != &BigInt::from(0));
        new_units
    }

    pub fn checked_mul(&self, rhs: Self) -> Result<Self, NumberOperationError> {
        let new_value = self.value.checked_mul(rhs.value)
            .ok_or(NumberOperationError::OperationFailed)?;

        // add unit powers
        let new_units = Self::addsub_units(&self.units, rhs.units, |r| r);

        Ok(Number::new(new_value, new_units))
    }

    pub fn checked_div(&self, rhs: Self) -> Result<Self, NumberOperationError> {
        let new_value = self.value.checked_div(rhs.value)
            .ok_or(NumberOperationError::OperationFailed)?;

        // subtract unit powers
        let new_units = Self::addsub_units(&self.units, rhs.units, |r| -r);

        Ok(Number::new(new_value, new_units))
    }

    pub fn checked_whole_div(&self, rhs: Self) -> Result<Self, NumberOperationError> {
        let new_value = self.value.checked_whole_div(rhs.value)
            .ok_or(NumberOperationError::OperationFailed)?;

        // subtract unit powers
        let new_units = Self::addsub_units(&self.units, rhs.units, |r| -r);

        Ok(Number::new(new_value, new_units))
    }

    pub fn checked_rem(&self, rhs: Self) -> Result<Self, NumberOperationError> {
        let new_value = self.value.checked_rem(rhs.value)
            .ok_or(NumberOperationError::OperationFailed)?;

        // subtract unit powers
        let new_units = Self::addsub_units(&self.units, rhs.units, |r| -r);

        Ok(Number::new(new_value, new_units))
    }

    pub fn negated(&self) -> Self {
        Self::new(
            -self.value.clone(),
            self.units.clone(),
        )
    }
}
impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let one = BigInt::from(1);
        write!(f, "{}", self.value)?;
        for (unit, power) in &self.units {
            if power == &one {
                write!(f, "#{}", unit)?;
            } else {
                write!(f, "#{}{}", unit, power)?;
            }
        }
        Ok(())
    }
}
impl From<NumberValue> for Number {
    fn from(v: NumberValue) -> Self {
        Number::new(
            v,
            NumberUnits::new(),
        )
    }
}
impl Neg for Number {
    type Output = Number;

    fn neg(self) -> Self::Output {
        Number::new(
            -self.value,
            self.units,
        )
    }
}
