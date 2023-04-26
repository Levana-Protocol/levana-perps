use super::{Number, Signed, UnsignedDecimal};
use anyhow::{anyhow, Result};
use std::cmp::Ordering;

// Intentionally keeping operations pegged to Number for now.
//
// The whole point of the newtype wrappers is to ensure the mathematical
// operations we perform are logical. Providing a general purpose `checked_mul`
// that multiplies two collaterals together would defeat the whole purpose of
// the exercise. Going forward, we'll add type-specific operations.
//
// Addition and subtraction, however, can be added. All our numeric newtypes can
// be added and subtracted sanely.

impl<T: UnsignedDecimal> Signed<T> {
    /// Addition that checks for integer overflow.
    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        Ok(match (self.is_negative(), rhs.is_negative()) {
            (false, false) => Self::new_positive(self.value().checked_add(rhs.value())?),
            (true, true) => Self::new_negative(self.value().checked_add(rhs.value())?),
            (false, true) => {
                if self.value() >= rhs.value() {
                    Self::new_positive(self.value().checked_sub(rhs.value())?)
                } else {
                    Self::new_negative(rhs.value().checked_sub(self.value())?)
                }
            }
            (true, false) => {
                if self.value() >= rhs.value() {
                    Self::new_negative(self.value().checked_sub(rhs.value())?)
                } else {
                    Self::new_positive(rhs.value().checked_sub(self.value())?)
                }
            }
        })
    }

    /// Subtraction that checks for underflow
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        self.checked_add(-rhs)
    }
}

impl Number {
    /// Multiplication that checks for integer overflow
    pub fn checked_mul(self, rhs: Self) -> Result<Self> {
        match self.value().checked_mul(rhs.value()).ok() {
            None => Err(anyhow!(
                "Overflow while multiplying {} and {}",
                self.value(),
                rhs.value()
            )),
            Some(value) => Ok(if self.is_negative() == rhs.is_negative() {
                Signed::new_positive(value)
            } else {
                Signed::new_negative(value)
            }),
        }
    }

    /// Division that checks for underflow and divide-by-zero.
    pub fn checked_div(self, rhs: Self) -> Result<Self> {
        if rhs.is_zero() {
            Err(anyhow!("Cannot divide with zero"))
        } else {
            match self.value().checked_div(rhs.value()).ok() {
                None => Err(anyhow!(
                    "Overflow while dividing {} by {}",
                    self.value(),
                    rhs.value()
                )),
                Some(value) => Ok(if self.is_negative() == rhs.is_negative() {
                    Signed::new_positive(value)
                } else {
                    Signed::new_negative(value)
                }),
            }
        }
    }

    /// equality check with allowance for precision diff
    pub fn approx_eq(self, other: Number) -> bool {
        (self - other).abs() < Self::EPS_E7
    }

    /// equality check with allowance for precision diff
    pub fn approx_eq_eps(self, other: Number, eps: Number) -> bool {
        (self - other).abs() < eps
    }

    /// less-than with allowance for precision diff
    pub fn approx_lt_relaxed(self, other: Number) -> bool {
        self < other + Self::EPS_E7
    }

    /// greater-than with allowance for precision diff
    pub fn approx_gt_relaxed(self, other: Number) -> bool {
        self > other - Self::EPS_E7
    }

    /// greater-than with restriction for precision diff
    pub fn approx_gt_strict(self, other: Number) -> bool {
        self > other + Self::EPS_E7
    }
}

impl std::ops::Mul for Number {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        self.checked_mul(rhs).unwrap()
    }
}

impl std::ops::Mul<u64> for Number {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self {
        self.checked_mul(rhs.into()).unwrap()
    }
}

impl std::ops::Div<u64> for Number {
    type Output = Self;

    fn div(self, rhs: u64) -> Self {
        self.checked_div(rhs.into()).unwrap()
    }
}

impl std::ops::MulAssign for Number {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl std::ops::Div for Number {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        self.checked_div(rhs).unwrap()
    }
}

impl std::ops::DivAssign for Number {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl<T: UnsignedDecimal> std::ops::Add for Signed<T> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).unwrap()
    }
}
impl<T: UnsignedDecimal> std::ops::AddAssign for Signed<T> {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<T: UnsignedDecimal> std::ops::Sub for Signed<T> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs).unwrap()
    }
}

impl<T: UnsignedDecimal> std::ops::SubAssign for Signed<T> {
    fn sub_assign(&mut self, rhs: Self) {
        *self += -rhs;
    }
}

impl<T: UnsignedDecimal> std::cmp::PartialOrd for Signed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: UnsignedDecimal> std::cmp::Ord for Signed<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_positive_or_zero(), other.is_positive_or_zero()) {
            (true, true) => self.value().cmp(&other.value()),
            (false, false) => other.value().cmp(&self.value()),
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
        }
    }
}
