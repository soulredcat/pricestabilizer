//! Field-level rules shared by every section check.

use crate::app::error::AppError;

pub fn check(holds: bool, field: &'static str, rule: &'static str) -> Result<(), AppError> {
    if holds {
        Ok(())
    } else {
        Err(AppError::ConfigInvalid { field, rule })
    }
}

pub fn positive(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value.is_finite() && value > 0.0,
        field,
        "must be a positive finite number",
    )
}

pub fn non_negative(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value.is_finite() && value >= 0.0,
        field,
        "must be a finite number >= 0",
    )
}

/// Strictly between 0 and 1.
pub fn fraction(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value > 0.0 && value < 1.0,
        field,
        "must be strictly between 0 and 1",
    )
}

/// From 0 (inclusive) to 1 (exclusive): rates that may be zero.
pub fn rate(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        (0.0..1.0).contains(&value),
        field,
        "must be at least 0 and below 1",
    )
}

pub fn nonzero(field: &'static str, value: u64) -> Result<(), AppError> {
    check(value > 0, field, "must be greater than 0")
}
