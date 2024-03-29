use rug::Integer;
use std::cmp::Ordering;

use crate::{rfloat::RFloat, Real};

use super::FloatContext;

/// Exception flags to signal properties of a rounded result.
///
/// Similar to IEEE 754 style exceptions, except we have
/// no constraints on exponent so we only have four exceptions:
///
/// - _invalid operation_: no useful definable result;
/// - _division by zero_: an infinite result for finite arguments;
/// - _inexact_: result would be different had both the exponent range
///     and precision been unbounded.
/// - _carry_: the exponent of the rounded result when in the form
///     `(-1)^s * c * b^exp` is different than that of the truncated result.
///     In particular, it was incremented by 1 by the rounding operation.
///
#[derive(Clone, Debug, Default)]
pub struct Exceptions {
    pub invalid: bool,
    pub divzero: bool,
    pub inexact: bool,
    pub carry: bool,
}

impl Exceptions {
    /// Constructs a new set of exceptions.
    /// All flags are set to false.
    pub fn new() -> Self {
        Self {
            invalid: false,
            divzero: false,
            inexact: false,
            carry: false,
        }
    }
}

/// A fixed-precision, floating-point number with unbounded exponent.
///
/// The associated [`RoundingContext`][crate::RoundingContext]
/// implementation is [`FloatContext`][crate::float::FloatContext].
/// See [`FloatContext`] for more details on numerical properties
/// of the [`Float`] type.
///
/// A [`Float`] also has an [`Exceptions`] instance to indicate
/// exceptional events that occured during its construction.
#[derive(Debug, Clone)]
pub struct Float {
    pub(crate) num: RFloat,
    pub(crate) flags: Exceptions,
    pub(crate) ctx: FloatContext,
}

impl Float {
    /// Return the flags set when this number was created.
    pub fn flags(&self) -> &Exceptions {
        &self.flags
    }

    /// Returns the rounding context under which this number was created.
    pub fn ctx(&self) -> &FloatContext {
        &self.ctx
    }
}

impl Real for Float {
    fn radix() -> usize {
        2
    }

    fn sign(&self) -> Option<bool> {
        self.num.sign()
    }

    fn exp(&self) -> Option<isize> {
        self.num.exp()
    }

    fn e(&self) -> Option<isize> {
        self.num.e()
    }

    fn n(&self) -> Option<isize> {
        self.num.n()
    }

    fn c(&self) -> Option<Integer> {
        self.num.c()
    }

    fn m(&self) -> Option<Integer> {
        self.num.m()
    }

    fn prec(&self) -> Option<usize> {
        self.num.prec()
    }

    fn is_nar(&self) -> bool {
        self.num.is_nar()
    }

    fn is_finite(&self) -> bool {
        self.num.is_finite()
    }

    fn is_infinite(&self) -> bool {
        self.num.is_finite()
    }

    fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    fn is_negative(&self) -> Option<bool> {
        self.num.is_negative()
    }

    fn is_numerical(&self) -> bool {
        self.num.is_numerical()
    }
}

impl From<Float> for RFloat {
    fn from(value: Float) -> Self {
        value.num
    }
}

impl From<Float> for rug::Float {
    fn from(value: Float) -> Self {
        rug::Float::from(value.num)
    }
}

impl PartialOrd for Float {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.num.partial_cmp(&other.num)
    }
}

impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}
