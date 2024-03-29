use num_traits::Zero;
use rug::Integer;
use std::fmt::Debug;

use crate::rfloat::RFloat;
use crate::util::bitmask;

/// Universal trait for extended real numbers.
///
/// Computer number systems share certain characterstics.
/// Many can be represented by a finite-precision number in
/// scientific notation: `(-1)^s * c * b^exp` where `s` is the sign,
/// `c` is the integer significand, `b` is the radix, and `exp` is
/// the exponent. Specifically, `s` is either `0` or `1`, `c` is
/// non-negative, and `b` is positive. Number systems can usually be
/// split into two broad groups: floating-point or fixed-point,
/// where the "point" refers to the position of the least-significant digit
/// in `c` when viewing the significand as an infinite sequence of digits
/// in either direction. Number systems may encode non-real numbers,
/// notably infinity or NaN.
///
/// See [`RoundingContext`][crate::RoundingContext] for details on rounding.
///
pub trait Real: Debug {
    /// Radix of a number.
    /// It must be strictly positive.
    fn radix() -> usize;

    /// The sign bit.
    /// This is not always well-defined, so the result is an [`Option`].
    /// This is distinct from `is_negative` (e.g. `-0.0` is not negative).
    fn sign(&self) -> Option<bool>;

    /// The exponent of this number when viewed as `(-1)^s * c * b^exp`
    /// where `c` is an integer integer. Only well-defined for finite,
    /// non-zero numbers.
    fn exp(&self) -> Option<isize>;

    /// The exponent of this number when viewed as `(-1)^s * f * b^e`
    /// where `f` is a fraction between 1 and 2. This is the preferred
    /// IEEE 754 interpretation of an exponent. Only well-defined for
    /// finite, non-zero numbers.
    fn e(&self) -> Option<isize>;

    /// The "absolute digit", the place below the least significant
    /// digit of the mantissa. Always equal to `self.exp() - 1`.
    /// For integer formats, this is just -1. Only well-defined for
    /// finite, non-zero numbers.
    fn n(&self) -> Option<isize>;

    /// The _unsigned" integer significand of this number when viewed as
    /// `(-1)^s * c * b^exp`. Only well-defined for finite, non-zero
    /// numbers. Only well-defined for finite, non-zero numbers.
    fn c(&self) -> Option<Integer>;

    /// The _signed_ integer significand of this number when viewed as
    /// `(-1)^s * c * b^exp`. Only well-defined for finite, non-zero
    /// numbers. Only well-defined for finite, non-zero numbers.
    fn m(&self) -> Option<Integer>;

    /// Precision of the significand.
    /// This is just `floor(logb(c))` where `b` is the radix and `c` is
    /// the integer significand. Only well-defined for finite,
    /// non-zero numbers.
    fn prec(&self) -> Option<usize>;

    /// Returns `true` if this number is not a real number.
    /// Example: NaN or +/-Inf from the IEEE 754 standard.
    fn is_nar(&self) -> bool;

    /// Returns `true` if this number is finite.
    /// For values that do not encode numbers, intervals, or even limiting
    /// behavior, the result is false.
    fn is_finite(&self) -> bool;

    /// Returns `true` if this number if infinite.
    /// For values that do not encode numbers, intervals, or even limiting
    /// behavior, the result is false.
    fn is_infinite(&self) -> bool;

    /// Returns `true` if this number is zero.
    fn is_zero(&self) -> bool;

    /// Returns `true` if this number is negative.
    /// This is not always well-defined, so the result is an [`Option`].
    /// This is not necessarily the same as the sign bit (the IEEE 754
    /// standard differentiates between -0.0 and +0.0).
    fn is_negative(&self) -> Option<bool>;

    /// Returns `true` if this number represents a numerical value:
    /// either a finite number, interval, or some limiting value.
    fn is_numerical(&self) -> bool;

    /// Splits this value at the `n`th binary digit,
    /// returning two [`RFloat`] values.
    ///
    /// The two values consist of:
    ///
    ///  - all significant digits above position `n`
    ///  - all significant digits at or below position `n`
    ///
    /// The exact sum of the resulting values will be exactly `num`,
    /// so it "splits" `num`.
    fn split(&self, n: isize) -> (RFloat, RFloat) {
        let s = self.sign().unwrap();
        if self.is_zero() {
            let high = RFloat::Real(s, 0, Integer::zero());
            let low = RFloat::Real(s, 0, Integer::zero());
            (high, low)
        } else {
            // number components
            let e = self.e().unwrap();
            let exp = self.exp().unwrap();
            let c = self.c().unwrap();

            // case split by split point offset
            if n >= e {
                // split point is above the significant digits
                let high = RFloat::Real(s, 0, Integer::zero());
                let low = RFloat::Real(s, exp, c);
                (high, low)
            } else if n < exp {
                // split point is below the significant digits
                let high = RFloat::Real(s, exp, c);
                let low = RFloat::Real(s, 0, Integer::zero());
                (high, low)
            } else {
                // split point is within the significant digits
                let offset = n - (exp - 1);
                let mask = bitmask(offset as usize);
                let high = RFloat::Real(s, n + 1, c.to_owned() >> offset);
                let low = RFloat::Real(s, exp, c & mask);
                (high, low)
            }
        }
    }
}
