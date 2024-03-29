use std::cmp::Ordering;
use std::ops::{BitAnd, BitOr};

use num_traits::Zero;
use rug::Integer;

use crate::ieee754::IEEE754Context;
use crate::rfloat::RFloat;
use crate::util::bitmask;
use crate::Real;

/// Exception flags to signal certain properties of the rounded result.
///
/// Besides returning a (possibly) numerical result, any computation with
/// floating-point numbers may also raise exceptions depending on certain conditions.
/// The IEEE 754 standard defines five such exceptions:
///
/// - _invalid operation_: no useful definable result;
/// - _division by zero_: an infinite result for finite arguments;
/// - _overflow_: result exceeded in magnitude what would have been the rounded result
///     had the exponent range been unbounded;
/// - _underflow_: non-zero result that either (a) would lie strictly between
///     `-b^emin` and `+b^emin` had the exponent range been unbounded,
///     or (b) would lie strictly between `-b^emin` and `+b^emin`
///     had the exponent range and precision been unbounded; this flag
///     may only be raised if the result is also inexact;
/// - _inexact_: result would be different had both the exponent range
///     and precision been unbounded.
///
/// These flags are contains in each Exceptions instance with similar names.
/// Note that both definitions of underflow are provided with (a) corresponding
/// to `underflow_post` and (b) corresponding to `underflow_pre`.
///
/// The [`Exceptions`] type defines additional exceptions:
///
/// - _carry_: the exponent of the rounded result when in the form
///     `(-1)^s * c * b^exp` is different than that of the truncated result.
///     In particular, it was incremented by 1 by the rounding operation.
///     This flag will not be raised if the final result is subnormal.
/// - _denorm_: at least one argument to a particular operation was
///     subnormal (see Section 4.9.1.2 of the Intel(R) 64 and IA-32
///     Architectures Developer's Manual: Vol. 1, June 2023).
/// - _tiny_pre_: similar to the `underflow_pre` flag except this flag
///     will be raised regardless of the state of the `inexact` flag,
///     i.e., `underflow_pre = tiny_pre && inexact`.
/// - _tiny_post_: similar to the `underflow_post` flag except this flag
///     will be raised regardless of the state of the `inexact` flag
///     i.e., `underflow_post = tiny_post && inexact`.
///
#[derive(Clone, Debug, Default)]
pub struct Exceptions {
    // defined in the IEEE 754 standard
    pub invalid: bool,
    pub divzero: bool,
    pub overflow: bool,
    pub underflow_pre: bool,
    pub underflow_post: bool,
    pub inexact: bool,
    // non-standard flags
    pub carry: bool,
    pub denorm: bool,
    pub tiny_pre: bool,
    pub tiny_post: bool,
}

impl Exceptions {
    /// Constructs a new set of exceptions.
    /// All flags are set to false.
    pub fn new() -> Self {
        Self {
            invalid: false,
            divzero: false,
            overflow: false,
            underflow_pre: false,
            underflow_post: false,
            inexact: false,
            carry: false,
            denorm: false,
            tiny_pre: false,
            tiny_post: false,
        }
    }
}

/// IEEE 754 floating-point bitwise encoding viewed as an enumeration.
/// Unlike [`IEEE754`], [`IEEE754Val`] contains only the numerical data
/// required to encode a binary floating-point number as described by
/// the IEEE 754 standard.
#[derive(Clone, Debug)]
pub enum IEEE754Val {
    /// Unsigned zero
    PosZero,
    /// Signed zero
    NegZero,
    /// Subnormal numbers: `Subnormal(s, c)` encodes `(-1)^s * c * 2^expmin`.
    /// If the float has parameters `es` and `nbits`, then `c` is an
    /// integer of bitwidth `nbits - es - 1`.
    Subnormal(bool, Integer),
    /// Normal numbers: `Normal(s, exp, c)` encodes `(-1)^s * c * 2^exp`
    /// where `exp` is between `expmin` and `expmax` and `c` is an
    /// integer of bitwidth `nbits - es`.
    Normal(bool, isize, Integer),
    /// Positive infinity: encodes `+Inf`
    PosInfinity,
    /// Negative infinity: encodes `-Inf`
    NegInfinity,
    /// Not-a-number: `Nan(s, quiet, payload)` where `s` specifies the
    /// sign bit, `quiet` the signaling bit, and `payload` the payload
    /// of the NaN value. Either `quiet` must be true or `payload` must
    /// be non-zero.
    Nan(bool, bool, Integer),
}

/// The IEEE 754 floating-point format.
///
/// The associated [`RoundingContext`][crate::RoundingContext]
/// implementation is [`IEEE754Context`].
/// See [`IEEE754Context`] for more details on numerical properties
/// of the [`IEEE754`] type.
///
/// A [`IEEE754`] value also has an [`Exceptions`] instance to indicate
/// exceptional events that occured during its construction.
#[derive(Clone, Debug)]
pub struct IEEE754 {
    pub(crate) num: IEEE754Val,
    pub(crate) flags: Exceptions,
    pub(crate) ctx: IEEE754Context,
}

impl IEEE754 {
    /// Return the flags set when this number was created.
    pub fn flags(&self) -> &Exceptions {
        &self.flags
    }

    /// Returns the rounding context under which this number was created.
    pub fn ctx(&self) -> &IEEE754Context {
        &self.ctx
    }

    /// Returns true if this [`IEEE754`] value is a subnormal number.
    pub fn is_subnormal(&self) -> bool {
        matches!(self.num, IEEE754Val::Subnormal(_, _))
    }

    /// Returns true if this [`IEEE754`] value is a normal number.
    pub fn is_normal(&self) -> bool {
        matches!(self.num, IEEE754Val::Normal(_, _, _))
    }

    /// Returns true if this [`IEEE754`] value is NaN.
    pub fn is_nan(&self) -> bool {
        matches!(self.num, IEEE754Val::Nan(_, _, _))
    }

    /// Returns the NaN signaling bit as an Option.
    /// The result is None if the number is not NaN.
    pub fn nan_quiet(&self) -> Option<bool> {
        match &self.num {
            IEEE754Val::Nan(_, q, _) => Some(*q),
            _ => None,
        }
    }

    /// Returns the NaN payload as an Option.
    /// The result is None if the number is not NaN.
    pub fn nan_payload(&self) -> Option<Integer> {
        match &self.num {
            IEEE754Val::Nan(_, _, payload) => Some(payload.clone()),
            _ => None,
        }
    }

    /// Converts this [`IEEE754`] to an [`Integer`] representing an IEEE 754 bitpattern.
    pub fn into_bits(self) -> Integer {
        let nbits = self.ctx.nbits();
        let (s, unsigned) = match &self.num {
            IEEE754Val::PosZero => (false, Integer::zero()),
            IEEE754Val::NegZero => (true, Integer::zero()),
            IEEE754Val::Subnormal(s, c) => (*s, c.clone()),
            IEEE754Val::Normal(s, exp, c) => {
                let m = self.ctx().max_m();
                let efield = Integer::from((exp + m as isize) + self.ctx().emax()) << m;
                let mfield = c.clone().bitand(bitmask(m));
                (*s, mfield.bitor(efield))
            }
            IEEE754Val::PosInfinity => {
                let m = self.ctx().max_m();
                let efield = bitmask(self.ctx.es()) << m;
                (false, efield)
            }
            IEEE754Val::NegInfinity => {
                let m = self.ctx().max_m();
                let efield = bitmask(self.ctx.es()) << m;
                (true, efield)
            }
            IEEE754Val::Nan(s, q, payload) => {
                let m = self.ctx().max_m() as isize;
                let efield = bitmask(self.ctx.es()) << m;
                let qfield = if *q {
                    Integer::from(1) << (m - 1)
                } else {
                    Integer::zero()
                };
                (*s, payload.clone().bitor(qfield).bitor(efield))
            }
        };

        if s {
            let sfield = Integer::from(1) << (nbits - 1);
            unsigned.bitor(sfield)
        } else {
            unsigned
        }
    }
}

impl Real for IEEE754 {
    fn radix() -> usize {
        2
    }

    fn sign(&self) -> Option<bool> {
        match &self.num {
            IEEE754Val::PosZero => Some(false),
            IEEE754Val::NegZero => Some(true),
            IEEE754Val::Subnormal(s, _) => Some(*s),
            IEEE754Val::Normal(s, _, _) => Some(*s),
            IEEE754Val::PosInfinity => Some(false),
            IEEE754Val::NegInfinity => Some(true),
            IEEE754Val::Nan(s, _, _) => Some(*s),
        }
    }

    fn exp(&self) -> Option<isize> {
        match &self.num {
            IEEE754Val::Subnormal(_, _) => Some(self.ctx().expmin()),
            IEEE754Val::Normal(_, exp, _) => Some(*exp),
            _ => None,
        }
    }

    fn e(&self) -> Option<isize> {
        match &self.num {
            IEEE754Val::Subnormal(_, c) => {
                let n = self.ctx().expmin() - 1;
                Some(n + (c.significant_bits() as isize))
            }
            IEEE754Val::Normal(_, exp, c) => Some((*exp - 1) + (c.significant_bits() as isize)),
            _ => None,
        }
    }

    fn n(&self) -> Option<isize> {
        match &self.num {
            IEEE754Val::Subnormal(_, _) => Some(self.ctx().expmin() - 1),
            IEEE754Val::Normal(_, exp, _) => Some(exp - 1),
            _ => None,
        }
    }

    fn c(&self) -> Option<Integer> {
        match &self.num {
            IEEE754Val::Subnormal(_, c) => Some(c.clone()),
            IEEE754Val::Normal(_, _, c) => Some(c.clone()),
            _ => None,
        }
    }

    fn m(&self) -> Option<Integer> {
        self.c().map(|c| if self.sign().unwrap() { -c } else { c })
    }

    fn prec(&self) -> Option<usize> {
        match &self.num {
            IEEE754Val::Subnormal(_, c) => Some(c.significant_bits() as usize),
            IEEE754Val::Normal(_, _, c) => Some(c.significant_bits() as usize),
            _ => None,
        }
    }

    fn is_nar(&self) -> bool {
        matches!(
            &self.num,
            IEEE754Val::PosInfinity | IEEE754Val::NegInfinity | IEEE754Val::Nan(_, _, _)
        )
    }

    fn is_finite(&self) -> bool {
        matches!(
            &self.num,
            IEEE754Val::PosZero
                | IEEE754Val::NegZero
                | IEEE754Val::Subnormal(_, _)
                | IEEE754Val::Normal(_, _, _)
        )
    }

    fn is_infinite(&self) -> bool {
        matches!(&self.num, IEEE754Val::PosInfinity | IEEE754Val::NegInfinity)
    }

    fn is_zero(&self) -> bool {
        matches!(&self.num, IEEE754Val::PosZero | IEEE754Val::NegZero)
    }

    fn is_negative(&self) -> Option<bool> {
        match &self.num {
            IEEE754Val::PosZero | IEEE754Val::NegZero => None,
            IEEE754Val::Subnormal(s, _) => Some(*s),
            IEEE754Val::Normal(s, _, _) => Some(*s),
            IEEE754Val::PosInfinity => Some(false),
            IEEE754Val::NegInfinity => Some(true),
            IEEE754Val::Nan(_, _, _) => None,
        }
    }

    fn is_numerical(&self) -> bool {
        !matches!(&self.num, IEEE754Val::Nan(_, _, _))
    }
}

impl From<IEEE754> for RFloat {
    fn from(val: IEEE754) -> Self {
        match val.num {
            IEEE754Val::PosZero | IEEE754Val::NegZero => RFloat::zero(),
            IEEE754Val::Subnormal(s, c) => RFloat::Real(s, val.ctx.expmin(), c),
            IEEE754Val::Normal(s, exp, c) => RFloat::Real(s, exp, c),
            IEEE754Val::PosInfinity => RFloat::PosInfinity,
            IEEE754Val::NegInfinity => RFloat::NegInfinity,
            IEEE754Val::Nan(_, _, _) => RFloat::Nan,
        }
    }
}

impl From<IEEE754> for rug::Float {
    fn from(val: IEEE754) -> Self {
        let s = val.sign().unwrap();
        let f = rug::Float::from(RFloat::from(val));
        if f.is_zero() && s {
            -f
        } else {
            f
        }
    }
}

impl PartialOrd for IEEE754 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        RFloat::from(self.clone()).partial_cmp(&RFloat::from(other.clone()))
    }
}

impl PartialEq for IEEE754 {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}
