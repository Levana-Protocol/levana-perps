use perpswap::{prelude::*, token::Token};

pub trait TokenExt {
    fn convert_u128<T: UnsignedDecimal>(&self, value: u128) -> NonZero<T>;
    fn mul_f32<T: UnsignedDecimal>(&self, value: NonZero<T>, factor: f32) -> NonZero<T>;
    fn assert_eq<T: UnsignedDecimal>(&self, a: NonZero<T>, b: NonZero<T>);
    fn assert_eq_signed<T: UnsignedDecimal>(&self, a: Signed<T>, b: Signed<T>);
}

impl TokenExt for Token {
    fn convert_u128<T: UnsignedDecimal>(&self, value: u128) -> NonZero<T> {
        NonZero::new(T::from_decimal256(self.from_u128(value).unwrap())).unwrap()
    }

    fn mul_f32<T: UnsignedDecimal>(&self, value: NonZero<T>, factor: f32) -> NonZero<T> {
        let factor: Decimal256 = factor.to_string().parse().unwrap();
        let value = value.into_decimal256() * factor;

        NonZero::new(T::from_decimal256(
            self.from_u128(self.into_u128(value).unwrap().unwrap())
                .unwrap(),
        ))
        .unwrap()
    }

    fn assert_eq<T: UnsignedDecimal>(&self, a: NonZero<T>, b: NonZero<T>) {
        // truncate both values and then compare
        let a = self.mul_f32(a, 1.0);
        let b = self.mul_f32(b, 1.0);

        assert_eq!(a, b);
    }

    fn assert_eq_signed<T: UnsignedDecimal>(&self, a: Signed<T>, b: Signed<T>) {
        // truncate both values and then compare
        let a_abs = self.mul_f32(a.abs().try_into_non_zero().unwrap(), 1.0);
        let b_abs = self.mul_f32(b.abs().try_into_non_zero().unwrap(), 1.0);
        assert_eq!(a_abs, b_abs);
        assert_eq!(a.is_negative(), b.is_negative());
    }
}
