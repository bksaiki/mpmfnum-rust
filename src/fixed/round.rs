use std::ops::Rem;

use rug::Integer;

use crate::fixed::{Exceptions, Fixed};
use crate::rfloat::{RFloat, RFloatContext};
use crate::{Real, RoundingContext, RoundingMode, Split};

/// Fixed-point overflow behavior.
///
/// Should an unrounded number exceed the maximum number in the format,
/// the rounded value must be the next best result. In a hardware
/// implementation of fixed-point numbers, the number typically wraps,
/// preserving only the least significant bits of the implementation.
/// Alternatively, the value could be clamped to the largest representable
/// value in the representation, preserving the sign.
#[derive(Clone, Debug)]
pub enum Overflow {
    /// Values that overflow the format should be wrapped, the least
    /// significant bits preserved.
    Wrap,
    /// Clamp the representation to the largest representable value
    /// in the representation, preserving the sign.
    Saturate,
}

/// Rounding contexts for fixed-point numbers.
///
/// The associated storage type is [`Fixed`].
///
/// Values rounded this context are fixed-point numbers:
/// `(-1)^s * c * 2^scale` where `c` is a fixed-precision
/// unsigned or signed integer and `scale` is a fixed integer.
///
/// A [`FixedContext`] is parameterized by
///
///  - signedness (unsigned vs. signed),
///  - scale factor (position of least-significant digit),
///  - total bitwidth of the encoding,
///  - rounding mode,
///  - overflow behavior.
///
/// By default, the rounding mode is [`RoundingMode::ToZero`], and
/// the overflow handling is [`Overflow::Wrap`].
/// See [`Overflow`] for supported overflow behavior.
///
#[derive(Clone, Debug)]
pub struct FixedContext {
    pub(crate) signed: bool,
    pub(crate) scale: isize,
    pub(crate) nbits: usize,
    pub(crate) rm: RoundingMode,
    pub(crate) overflow: Overflow,
}

impl FixedContext {
    /// Constructs new rounding context.
    /// The default rounding mode is truncation
    /// (see [`ToZero`][crate::RoundingMode]). The default overflow
    /// behavior is saturation (see [`Wrap`][Overflow]).
    pub fn new(signed: bool, scale: isize, nbits: usize) -> Self {
        assert!(
            (signed && nbits >= 2) || (!signed && nbits >= 1),
            "insufficient number of representation bits"
        );

        Self {
            signed,
            scale,
            nbits,
            rm: RoundingMode::ToZero,
            overflow: Overflow::Wrap,
        }
    }

    /// Sets the rounding mode of this [`FixedContext`].
    pub fn with_rounding_mode(mut self, rm: RoundingMode) -> Self {
        self.rm = rm;
        self
    }

    /// Sets the overflow behavior of this [`FixedContext`].
    pub fn with_overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// The maximum value in format specified by this [`FixedContext`].
    /// If the format is unsigned, this is just `2^scale * 2^nbits - 1`.
    /// If the format is signed, this is just `2^scale * 2^(nbits-1) - 1`.
    pub fn maxval(&self) -> Fixed {
        if self.signed {
            let c = (Integer::from(1) << (self.nbits - 1)) - 1;
            Fixed {
                num: RFloat::Real(false, self.scale, c),
                flags: Default::default(),
                ctx: self.clone(),
            }
        } else {
            let c = (Integer::from(1) << self.nbits) - 1;
            Fixed {
                num: RFloat::Real(false, self.scale, c),
                flags: Default::default(),
                ctx: self.clone(),
            }
        }
    }

    /// The minimum value in a format specified by this [`FixedContext`].
    /// If the format is unsigned, this is just `0`.
    /// If the format is signed, this is just `2^scale * -2^(nbits-1)`.
    pub fn minval(&self) -> Fixed {
        if self.signed {
            let c = Integer::from(1) << (self.nbits - 1);
            Fixed {
                num: RFloat::Real(true, self.scale, c),
                flags: Default::default(),
                ctx: self.clone(),
            }
        } else {
            Fixed {
                num: RFloat::zero(),
                flags: Default::default(),
                ctx: self.clone(),
            }
        }
    }

    /// Constructs zero in the format described by [`FixedContext`]
    pub fn zero(&self) -> Fixed {
        Fixed {
            num: RFloat::zero(),
            flags: Default::default(),
            ctx: self.clone(),
        }
    }

    /// Constructs the smallest representable difference in
    /// the format described by [`FixedContext`].
    pub fn quantum(&self) -> Fixed {
        Fixed {
            num: RFloat::Real(false, -self.scale, Integer::from(1)),
            flags: Default::default(),
            ctx: self.clone(),
        }
    }
}

impl FixedContext {
    fn round_wrap(&self, val: RFloat) -> RFloat {
        let offset = val.exp().unwrap() - self.scale;
        let div = Integer::from(1) << self.nbits;
        let c = val.c().unwrap() << offset;

        if self.signed {
            // signed wrapping is harder:
            // `((v + 2^(w - 1)) % 2^w) - 2^(w - 1))`
            let shift = Integer::from(1) << (self.nbits - 1);
            let m = if val.sign().unwrap() { -c } else { c };
            let wrapped = (m + shift.clone()).rem(div) - shift;
            RFloat::Real(wrapped.is_negative(), self.scale, wrapped.abs())
        } else {
            // unsigned wrapping is just `(v % 2^w)`
            let wrapped = c.rem(div);
            RFloat::Real(false, self.scale, wrapped)
        }
    }
}

impl RoundingContext for FixedContext {
    type Format = Fixed;

    fn round<T: Real>(&self, val: &T) -> Self::Format {
        // case split by class
        if val.is_zero() {
            // zero is always representable
            Fixed {
                num: RFloat::zero(),
                flags: Default::default(),
                ctx: self.clone(),
            }
        } else if val.is_infinite() {
            // +Inf goes to MAX
            // -Inf goes to MIN
            match val.sign() {
                Some(true) => self.maxval(),
                _ => self.minval(),
            }
        } else if val.is_nar() {
            // +/- NaN goes to 0
            Fixed {
                num: RFloat::zero(),
                flags: Exceptions {
                    invalid: true,
                    ..Default::default()
                },
                ctx: self.clone(),
            }
        } else {
            // step 1: rounding as a unbounded fixed-point number
            // so we need to compute the context parameters; we only set
            // `min_n` when rounding with a RFloatContext, the first
            // digit we want to chop off.
            let (p, n) = RFloatContext::new()
                .with_min_n(self.scale - 1)
                .round_params(val);

            // step 2: split the significand at binary digit `n`
            let split = Split::new(val, p, n);

            // step 3: extract split parameters and compute inexact flag
            let inexact = !split.is_exact();

            // step 3: finalize (fixed point)
            let rounded = RFloatContext::round_finalize(split, self.rm);

            // early exit if zero
            if rounded.is_zero() {
                return Fixed {
                    num: RFloat::zero(),
                    flags: Exceptions {
                        inexact,
                        ..Default::default()
                    },
                    ctx: self.clone(),
                };
            }

            // double check rounding invariants
            let exp = rounded.exp().unwrap();
            assert!(
                exp >= self.scale,
                "unexpected exponent, scale: {}, num: {:?}",
                self.scale,
                rounded
            );

            // step 4: may need to round or saturate
            let maxval = self.maxval();
            let minval = self.minval();
            if rounded > maxval.num {
                // larger than the maxval
                Fixed {
                    num: match self.overflow {
                        Overflow::Wrap => self.round_wrap(rounded),
                        Overflow::Saturate => maxval.num,
                    },
                    flags: Exceptions {
                        inexact,
                        overflow: true,
                        ..Default::default()
                    },
                    ctx: self.clone(),
                }
            } else if rounded < minval.num {
                // smaller than the minval
                Fixed {
                    num: match self.overflow {
                        Overflow::Wrap => self.round_wrap(rounded),
                        Overflow::Saturate => minval.num,
                    },
                    flags: Exceptions {
                        inexact,
                        underflow: false,
                        ..Default::default()
                    },
                    ctx: self.clone(),
                }
            } else {
                Fixed {
                    num: rounded,
                    flags: Exceptions {
                        inexact,
                        ..Default::default()
                    },
                    ctx: self.clone(),
                }
            }
        }
    }
}
