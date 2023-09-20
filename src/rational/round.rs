use rug::Integer;

use crate::rational::Rational;
use crate::round::RoundingDirection;
use crate::util::*;
use crate::{Number, RoundingContext, RoundingMode};

/// Result type of [`Context::round_prepare`].
pub(crate) struct RoundPrepareResult {
    pub sign: bool,
    pub exp: isize,
    pub c: Integer,
    pub halfway_bit: bool,
    pub quarter_bit: bool,
    pub sticky_bit: bool,
}

/// Rounding contexts for rational numbers.
///
/// Rounding a digital number to a fixed-width rational number takes three
/// parameters: a maximum precision (see [`Number::p`]) and the minimum least
/// absolute digit (see [`Number::n`]), and a rounding mode [`RoundingMode`].
/// Rounding will theoretically work for all real values. The requested
/// precision may be one or zero bits, but there is no way to place an
/// upper bound on the resulting exponent; infinity and NaN will not be
/// rounded.
///
/// There are three possible rounding behaviors: only `min_n` is specified,
/// only `max_p` is specified, or both are specified. In the first case,
/// rounding will behave as with fixed-point numbers with unbounded precision
/// but the exponent `exp` must be more than `min_n`. For example, if
/// `min_n == 1`, then the rounded result will be an integer. In the second
/// case, the rounding will behave as with floating-point numbers, adjusting
/// `c` so that it has at most `max_p` bits. In the third case, `min_n` takes
/// precedence, so the result may have less than `max_p` precision even if
/// the input has at least `max_p` precision. This behavior may be used to
/// emulate IEEE 754 subnormalization. At least one parameter must be given
/// or rounding will panic.
///
/// The rounding mode affects how "lost" binary digits are handled. The
/// possible rounding modes that can be specified are defined by
/// [`RoundingMode`].
///
#[derive(Clone, Debug)]
pub struct Context {
    max_p: Option<usize>,
    min_n: Option<isize>,
    rm: RoundingMode,
}

impl Context {
    /// Constructs a rounding arguments with default arguments.
    /// Neither `max_p` nor `min_n` are specified so rounding
    /// will panic. The default rounding mode is
    /// [`RoundingMode::NearestTiesToEven`].
    pub fn new() -> Self {
        Self {
            max_p: None,
            min_n: None,
            rm: RoundingMode::NearestTiesToEven,
        }
    }

    /// Sets the maximum allowable precision.
    pub fn with_max_precision(mut self, max_p: usize) -> Self {
        self.max_p = Some(max_p);
        self
    }

    /// Sets the minimum least absolute digit.
    pub fn with_min_n(mut self, min_n: isize) -> Self {
        self.min_n = Some(min_n);
        self
    }

    /// Sets the rounding mode.
    pub fn with_rounding_mode(mut self, rm: RoundingMode) -> Self {
        self.rm = rm;
        self
    }

    /// Clears the maximum allowable precision.
    pub fn without_max_precision(mut self) -> Self {
        self.max_p = None;
        self
    }

    /// Clears the minimum least absolute digit.
    pub fn without_min_n(mut self) -> Self {
        self.min_n = None;
        self
    }

    /// Rounding utility function: splits a [`Number`] at binary digit `n`,
    /// returning two rational numbers: the first capturing digits above
    /// the digit at position `n`, and the second capturing digits at or
    /// below the digit at position `n`.
    pub(crate) fn split_at<T: Number>(num: &T, n: isize) -> (Rational, Rational) {
        // number components
        let s = num.sign();
        let e = num.e().unwrap();
        let exp = num.exp().unwrap();
        let c = num.c().unwrap();

        if n >= e {
            // split point is above the significant digits
            let high = Rational::Real(s, n + 1, Integer::from(0));
            let low = Rational::Real(s, exp, c);
            (high, low)
        } else if n < exp {
            // split point is below the significant digits
            let high = Rational::Real(s, exp, c);
            let low = Rational::Real(s, n, Integer::from(0));
            (high, low)
        } else {
            // split point is within the significant digits
            let offset = n - (exp - 1);
            let mask = bitmask(offset as usize);
            let c_high = c.clone() >> offset;
            let c_low = c & mask;

            let high = Rational::Real(s, n + 1, c_high);
            let low = Rational::Real(s, exp, c_low);
            (high, low)
        }
    }

    /// Rounding utility function: returns the rounding parameters
    /// necessary to perform rounding under this context for a
    /// given [`Number`].
    pub(crate) fn round_params<T: Number>(&self, num: &T) -> (Option<usize>, isize) {
        match (self.max_p, self.min_n) {
            (None, None) => {
                // unreachable
                panic!(
                    "at least one rounding parameter must be specified: max_p={:?}, min_n={:?}",
                    self.max_p, self.min_n
                );
            }
            (None, Some(min_n)) => {
                // fixed-point rounding:
                // limited by n, precision is unbounded
                (None, min_n)
            }
            (Some(max_p), None) => {
                // floating-point rounding:
                // limited by precision, exponent is unbounded
                (Some(max_p), num.e().unwrap() - (max_p as isize))
            }
            (Some(max_p), Some(min_n)) => {
                // floating-point rounding with subnormalization:
                // limited by precision or exponent
                let unbounded_n = num.e().unwrap() - (max_p as isize);
                let n = std::cmp::max(min_n, unbounded_n);
                (Some(max_p), n)
            }
        }
    }

    /// Rounding utility function: splits a [`Number`] at binary digit `n`,
    /// returning the digits above that position as a [`Rational`] number,
    /// two subsequent digits at the `n`th and `n-1`th position, and an
    /// inexact bit if there are any lower order digits.
    pub(crate) fn round_prepare<T: Number>(num: &T, n: isize) -> RoundPrepareResult {
        // split number at the `n`th digit
        let (high, low) = Self::split_at(num, n);

        // split the lower part at the `n-2`th digit
        let (mid, low) = Self::split_at(&low, n - 2);

        // compute the rounding bits
        let halfway_bit = mid.get_bit(n);
        let quarter_bit = mid.get_bit(n - 1);
        let sticky_bit = !low.is_zero();

        // compose result
        let result = RoundPrepareResult {
            sign: num.sign(),
            exp: high.exp().unwrap(),
            c: high.c().unwrap(),
            halfway_bit,
            quarter_bit,
            sticky_bit,
        };

        assert_eq!(result.exp, n + 1, "exponent not in the right place!");

        result
    }

    /// Rounding utility function: given the truncated result and rounding
    /// bits, should the truncated result be incremented to produce
    /// the final rounded result?
    fn round_increment(
        sign: bool,
        c: &Integer,
        half_bit: bool,
        sticky_bit: bool,
        rm: RoundingMode,
    ) -> bool {
        let (is_nearest, rd) = rm.to_direction(sign);
        match (is_nearest, half_bit, sticky_bit, rd) {
            (_, false, false, _) => {
                // exact => truncate
                false
            }
            (true, false, _, _) => {
                // nearest, below the halfway point => truncate
                false
            }
            (true, true, true, _) => {
                // nearest, above the halfway point => increment
                true
            }
            (true, true, false, RoundingDirection::ToZero) => {
                // nearest, exactly halfway, ToZero => truncate
                false
            }
            (true, true, false, RoundingDirection::AwayZero) => {
                // nearest, exactly halfway, AwayZero => increment
                true
            }
            (true, true, false, RoundingDirection::ToEven) => {
                // nearest, exactly halfway, ToEven => increment if odd
                c.is_odd()
            }
            (true, true, false, RoundingDirection::ToOdd) => {
                // nearest, exactly halfway, ToOdd => increment if even
                c.is_even()
            }
            (false, _, _, RoundingDirection::ToZero) => {
                // directed, toZero => always truncate
                false
            }
            (false, _, _, RoundingDirection::AwayZero) => {
                // directed, alwaysZero => increment
                true
            }
            (false, _, _, RoundingDirection::ToEven) => {
                // directed, toEven => increment if odd
                c.is_odd()
            }
            (false, _, _, RoundingDirection::ToOdd) => {
                // directed, toOdd => increment if even
                c.is_even()
            }
        }
    }

    /// Rounding utility function: finishes the rounding procedure
    /// by possibly incrementing the mantissa; the decision is
    /// based on rounding mode and rounding bits.
    pub(crate) fn round_finalize(
        split: RoundPrepareResult,
        p: Option<usize>,
        rm: RoundingMode,
    ) -> Rational {
        // truncated result
        let sign = split.sign;
        let mut exp = split.exp;
        let mut c = split.c;

        // rounding bits
        let halfway_bit = split.halfway_bit;
        let sticky_bit = split.quarter_bit || split.sticky_bit;

        // correct if needed
        if Self::round_increment(sign, &c, halfway_bit, sticky_bit, rm) {
            c += 1;
            if p.is_some() && c.significant_bits() as usize > p.unwrap() {
                c >>= 1;
                exp += 1;
            }
        }

        Rational::Real(sign, exp, c)
    }

    /// Rounds a finite [`Number`].
    ///
    /// Called by the public [`Number::round`] function.
    fn round_finite<T: Number>(&self, num: &T) -> Rational {
        // step 1: compute the first digit we will split off
        let (p, n) = self.round_params(num);

        // step 2: split the significand at binary digit `n`
        let split = Self::round_prepare(num, n);

        // step 3: finalize the rounding
        let rounded = Self::round_finalize(split, p, self.rm);

        // return the rounded number
        rounded.canonicalize()
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl RoundingContext for Context {
    type Rounded = Rational;

    fn round(&self, val: &Self::Rounded) -> Self::Rounded {
        self.mpmf_round(val)
    }

    fn mpmf_round<T: Number>(&self, num: &T) -> Self::Rounded {
        assert!(
            self.max_p.is_some() || self.min_n.is_some(),
            "must specify either maximum precision or least absolute digit"
        );

        // case split by class
        if num.is_zero() {
            // zero
            Rational::zero()
        } else if num.is_infinite() {
            // infinite number
            let s = num.is_negative().unwrap();
            Rational::Infinite(s)
        } else if num.is_nar() {
            // other non-real
            Rational::Nan
        } else {
            // finite, non-zero value
            self.round_finite(num)
        }
    }
}
