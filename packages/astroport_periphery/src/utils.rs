use cosmwasm_std::{Decimal256, Fraction, OverflowError, Uint128, Uint256, Uint512};
use std::convert::TryInto;

pub trait Decimal256CheckedOps {
    fn checked_mul_uint256(self, other: Uint256) -> Result<Uint128, OverflowError>;
}

impl Decimal256CheckedOps for Decimal256 {
    fn checked_mul_uint256(self, other: Uint256) -> Result<Uint128, OverflowError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Uint128::zero());
        }

        let multiply_ratio = other.full_mul(self.numerator()) / Uint512::from(self.denominator());
        if multiply_ratio > Uint512::from(Uint128::MAX) {
            Err(OverflowError::new(
                cosmwasm_std::OverflowOperation::Mul,
                self,
                other,
            ))
        } else {
            Ok(multiply_ratio.try_into().unwrap())
        }
    }
}
