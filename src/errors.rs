// =============================================================================
// errors.rs — User-friendly error handling for fin_functions DuckDB extension
// =============================================================================
//
// USAGE IN OTHER MODULES
// ----------------------
// use crate::errors::{FinError, validate_rate, validate_nper, validate_cost,
//                     validate_date_order, validate_basis, arg_error};
//
// Return type for all scalar functions should use:
//   Result<f64, FinError>   (or Result<i64, FinError> for integer outputs)
//
// Then convert to a DuckDB error string at the call site with:
//   .map_err(|e| e.to_duckdb_string())
// =============================================================================

use std::fmt;

// ---------------------------------------------------------------------------
// Core error type
// ---------------------------------------------------------------------------

/// All error conditions that can arise in the financial functions extension.
/// Every variant carries enough context to produce a self-contained, helpful
/// message — the function name, argument name, bad value, and valid range.
#[derive(Debug, Clone)]
pub enum FinError {
    // ── Type / parse errors ─────────────────────────────────────────────────
    /// A string argument could not be parsed as a number.
    ParseFloat {
        func: &'static str,
        arg: &'static str,
        value: String,
    },
    /// A string argument could not be parsed as an integer.
    ParseInt {
        func: &'static str,
        arg: &'static str,
        value: String,
    },
    /// A string date could not be parsed (expected YYYY-MM-DD).
    ParseDate {
        func: &'static str,
        arg: &'static str,
        value: String,
    },
    /// A comma-separated list of values could not be parsed.
    ParseList {
        func: &'static str,
        arg: &'static str,
        detail: String,
    },

    // ── Domain / range errors ────────────────────────────────────────────────
    /// A numeric argument is outside its valid range.
    OutOfRange {
        func: &'static str,
        arg: &'static str,
        value: f64,
        min: Option<f64>,   // None = no lower bound
        max: Option<f64>,   // None = no upper bound
        exclusive_min: bool,
        exclusive_max: bool,
        hint: Option<&'static str>,
    },
    /// An integer argument is outside its valid range.
    OutOfRangeInt {
        func: &'static str,
        arg: &'static str,
        value: i64,
        min: Option<i64>,
        max: Option<i64>,
        hint: Option<&'static str>,
    },
    /// Two arguments that must be ordered are not (e.g. settlement < maturity).
    DateOrder {
        func: &'static str,
        earlier_arg: &'static str,
        later_arg: &'static str,
        earlier_value: String,
        later_value: String,
    },
    /// Lists that must be the same length are not (e.g. XNPV cash flows vs dates).
    LengthMismatch {
        func: &'static str,
        arg_a: &'static str,
        arg_b: &'static str,
        len_a: usize,
        len_b: usize,
    },
    /// A list argument is empty but must have at least one element.
    EmptyList {
        func: &'static str,
        arg: &'static str,
    },
    /// A required minimum number of list elements was not met.
    TooFewElements {
        func: &'static str,
        arg: &'static str,
        found: usize,
        minimum: usize,
    },

    // ── Convergence / computation errors ────────────────────────────────────
    /// An iterative solver (IRR, RATE, XIRR …) did not converge.
    NoConvergence {
        func: &'static str,
        hint: &'static str,
    },
    /// A computation produced NaN or Infinity.
    InvalidResult {
        func: &'static str,
        detail: &'static str,
    },
    /// Division by zero (e.g. zero par value, zero cost basis).
    DivisionByZero {
        func: &'static str,
        arg: &'static str,
    },

    // ── Enum / code arguments ────────────────────────────────────────────────
    /// An integer argument representing an enumerated code is invalid.
    InvalidCode {
        func: &'static str,
        arg: &'static str,
        value: i64,
        valid_values: &'static str, // human-readable, e.g. "0, 1, 2, 3, 4"
        description: &'static str,  // e.g. "day-count basis"
    },
    /// The `type` / `when` argument (0 = end of period, 1 = beginning) is invalid.
    InvalidPaymentType {
        func: &'static str,
        value: i64,
    },

    // ── Generic fallback ─────────────────────────────────────────────────────
    /// A catch-all for unusual conditions not covered above.
    General {
        func: &'static str,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Display — this is what the user sees in DuckDB
// ---------------------------------------------------------------------------

impl fmt::Display for FinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // ── Parse errors ─────────────────────────────────────────────
            FinError::ParseFloat { func, arg, value } => write!(
                f,
                "[{func}] Argument '{arg}' must be a number, but received \"{value}\". \
                 Example: {func}(..., 0.05, ...) where 0.05 represents 5%."
            ),
            FinError::ParseInt { func, arg, value } => write!(
                f,
                "[{func}] Argument '{arg}' must be a whole number (integer), but received \"{value}\". \
                 Example: pass 12 for monthly periods."
            ),
            FinError::ParseDate { func, arg, value } => write!(
                f,
                "[{func}] Argument '{arg}' must be a date in YYYY-MM-DD format, but received \"{value}\". \
                 Example: '2024-01-15'."
            ),
            FinError::ParseList { func, arg, detail } => write!(
                f,
                "[{func}] Argument '{arg}' must be a comma-separated list of numbers (e.g. \
                 '-10000,3000,4200,6800'), but parsing failed: {detail}."
            ),

            // ── Range errors ─────────────────────────────────────────────
            FinError::OutOfRange { func, arg, value, min, max, exclusive_min, exclusive_max, hint } => {
                write!(f, "[{func}] Argument '{arg}' = {value} is out of the valid range ")?;
                match (min, max) {
                    (Some(lo), Some(hi)) => {
                        let lo_bracket = if *exclusive_min { "(" } else { "[" };
                        let hi_bracket = if *exclusive_max { ")" } else { "]" };
                        write!(f, "{lo_bracket}{lo}, {hi}{hi_bracket}")?;
                    }
                    (Some(lo), None) => {
                        let lo_bracket = if *exclusive_min { "(" } else { "[" };
                        write!(f, "{lo_bracket}{lo}, ∞)")?;
                    }
                    (None, Some(hi)) => {
                        let hi_bracket = if *exclusive_max { ")" } else { "]" };
                        write!(f, "(-∞, {hi}{hi_bracket}")?;
                    }
                    (None, None) => write!(f, "(unrestricted)")?,
                }
                if let Some(h) = hint {
                    write!(f, ". Hint: {h}")?;
                }
                Ok(())
            }
            FinError::OutOfRangeInt { func, arg, value, min, max, hint } => {
                write!(f, "[{func}] Argument '{arg}' = {value} is outside the allowed range")?;
                match (min, max) {
                    (Some(lo), Some(hi)) => write!(f, " [{lo}, {hi}]")?,
                    (Some(lo), None)     => write!(f, " [{lo}, ∞)")?,
                    (None, Some(hi))     => write!(f, " (-∞, {hi}]")?,
                    (None, None)         => {},
                }
                if let Some(h) = hint {
                    write!(f, ". Hint: {h}")?;
                }
                Ok(())
            }
            FinError::DateOrder { func, earlier_arg, later_arg, earlier_value, later_value } => write!(
                f,
                "[{func}] '{earlier_arg}' ({earlier_value}) must be earlier than \
                 '{later_arg}' ({later_value}). Check that settlement and maturity \
                 dates are not swapped."
            ),
            FinError::LengthMismatch { func, arg_a, arg_b, len_a, len_b } => write!(
                f,
                "[{func}] Arguments '{arg_a}' and '{arg_b}' must have the same number \
                 of elements, but '{arg_a}' has {len_a} and '{arg_b}' has {len_b}. \
                 Each cash flow must have exactly one corresponding date."
            ),
            FinError::EmptyList { func, arg } => write!(
                f,
                "[{func}] Argument '{arg}' is empty. Provide at least one value \
                 as a comma-separated string, e.g. '-10000,3000,4200'."
            ),
            FinError::TooFewElements { func, arg, found, minimum } => write!(
                f,
                "[{func}] Argument '{arg}' has only {found} element(s) but requires \
                 at least {minimum}."
            ),

            // ── Computation errors ────────────────────────────────────────
            FinError::NoConvergence { func, hint } => write!(
                f,
                "[{func}] The solver did not converge to a solution. {hint}"
            ),
            FinError::InvalidResult { func, detail } => write!(
                f,
                "[{func}] The computation produced an invalid result (NaN or Infinity). {detail}"
            ),
            FinError::DivisionByZero { func, arg } => write!(
                f,
                "[{func}] Argument '{arg}' must not be zero — it appears in the denominator."
            ),

            // ── Code/enum errors ──────────────────────────────────────────
            FinError::InvalidCode { func, arg, value, valid_values, description } => write!(
                f,
                "[{func}] Argument '{arg}' = {value} is not a valid {description} code. \
                 Valid values are: {valid_values}."
            ),
            FinError::InvalidPaymentType { func, value } => write!(
                f,
                "[{func}] Argument 'type' = {value} is invalid. \
                 Use 0 for payments at the END of each period (ordinary annuity) \
                 or 1 for payments at the BEGINNING of each period (annuity-due)."
            ),

            // ── General ───────────────────────────────────────────────────
            FinError::General { func, message } => write!(
                f,
                "[{func}] {message}"
            ),
        }
    }
}

impl std::error::Error for FinError {}
impl FinError {
    /// Convert to the string format DuckDB expects when returning an error
    /// from a scalar function.  Use this at the outermost call site:
    ///
    /// ```rust
    /// .map_err(|e| e.to_duckdb_string())
    /// ```
    pub fn to_duckdb_string(&self) -> String {
        self.to_string()
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors — keeps call sites concise
// ---------------------------------------------------------------------------

/// Shorthand: create a `General` error.
pub fn arg_error(func: &'static str, message: impl Into<String>) -> FinError {
    FinError::General { func, message: message.into() }
}

// ---------------------------------------------------------------------------
// Validation helpers — call these at the top of every function implementation
// ---------------------------------------------------------------------------

/// Validate a rate / interest-rate argument.
/// Rate must be > -1.0 (i.e. > -100%).  Most functions also forbid rate == 0;
/// set `allow_zero` accordingly.
pub fn validate_rate(func: &'static str, rate: f64, allow_zero: bool) -> Result<(), FinError> {
    if rate <= -1.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "rate",
            value: rate,
            min: Some(-1.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("Express as a decimal: 5% → 0.05, not 5."),
        });
    }
    if !allow_zero && rate == 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "rate",
            value: rate,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("A rate of exactly zero is not supported by this function."),
        });
    }
    Ok(())
}

/// Validate a number-of-periods argument (must be a positive integer or float).
pub fn validate_nper(func: &'static str, nper: f64) -> Result<(), FinError> {
    if nper <= 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "nper",
            value: nper,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("nper is the total number of payment periods and must be positive."),
        });
    }
    Ok(())
}

/// Validate a cost / initial-investment argument (must be positive).
pub fn validate_cost(func: &'static str, arg: &'static str, value: f64) -> Result<(), FinError> {
    if value <= 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg,
            value,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("Asset cost/investment must be a positive number."),
        });
    }
    Ok(())
}

/// Validate a salvage value (must be ≥ 0 and < cost).
pub fn validate_salvage(func: &'static str, salvage: f64, cost: f64) -> Result<(), FinError> {
    if salvage < 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "salvage",
            value: salvage,
            min: Some(0.0),
            max: None,
            exclusive_min: false,
            exclusive_max: false,
            hint: Some("Salvage value is the residual value at end of life and cannot be negative."),
        });
    }
    if salvage >= cost {
        return Err(FinError::General {
            func,
            message: format!(
                "Argument 'salvage' ({salvage}) must be less than 'cost' ({cost}). \
                 An asset cannot be worth more at the end of its life than at purchase."
            ),
        });
    }
    Ok(())
}

/// Validate a `life` / useful-life-in-periods argument (positive integer).
pub fn validate_life(func: &'static str, life: f64) -> Result<(), FinError> {
    if life <= 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "life",
            value: life,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("'life' is the number of periods over which the asset is depreciated."),
        });
    }
    Ok(())
}

/// Validate a depreciation `period` argument (1 ≤ period ≤ life).
pub fn validate_period(func: &'static str, period: f64, life: f64) -> Result<(), FinError> {
    if period < 1.0 || period > life {
        return Err(FinError::OutOfRange {
            func,
            arg: "period",
            value: period,
            min: Some(1.0),
            max: Some(life),
            exclusive_min: false,
            exclusive_max: false,
            hint: Some("'period' is the specific year/period for which depreciation is calculated."),
        });
    }
    Ok(())
}

/// Validate a `per` argument for IPMT / PPMT (1 ≤ per ≤ nper).
pub fn validate_per(func: &'static str, per: f64, nper: f64) -> Result<(), FinError> {
    if per < 1.0 || per > nper {
        return Err(FinError::OutOfRange {
            func,
            arg: "per",
            value: per,
            min: Some(1.0),
            max: Some(nper),
            exclusive_min: false,
            exclusive_max: false,
            hint: Some("'per' is the payment period number; it must be between 1 and nper."),
        });
    }
    Ok(())
}

/// Validate a day-count basis code (0–4 for most bond functions).
///
/// | Code | Convention            |
/// |------|-----------------------|
/// | 0    | US 30/360             |
/// | 1    | Actual/Actual         |
/// | 2    | Actual/360            |
/// | 3    | Actual/365            |
/// | 4    | European 30/360       |
pub fn validate_basis(func: &'static str, basis: i64) -> Result<(), FinError> {
    if !(0..=4).contains(&basis) {
        return Err(FinError::InvalidCode {
            func,
            arg: "basis",
            value: basis,
            valid_values: "0 (US 30/360), 1 (Actual/Actual), 2 (Actual/360), \
                           3 (Actual/365), 4 (European 30/360)",
            description: "day-count basis",
        });
    }
    Ok(())
}

/// Validate a `type` / `when` payment-timing argument (0 or 1).
pub fn validate_payment_type(func: &'static str, payment_type: i64) -> Result<(), FinError> {
    if payment_type != 0 && payment_type != 1 {
        return Err(FinError::InvalidPaymentType { func, value: payment_type });
    }
    Ok(())
}

/// Validate that settlement < maturity (or settlement <= maturity when equal is allowed).
pub fn validate_date_order(
    func: &'static str,
    settlement: &str,
    maturity: &str,
    settlement_arg: &'static str,
    maturity_arg: &'static str,
    allow_equal: bool,
) -> Result<(), FinError> {
    // Simple lexicographic comparison is valid for ISO 8601 (YYYY-MM-DD) dates.
    let bad = if allow_equal {
        settlement > maturity
    } else {
        settlement >= maturity
    };
    if bad {
        return Err(FinError::DateOrder {
            func,
            earlier_arg: settlement_arg,
            later_arg: maturity_arg,
            earlier_value: settlement.to_owned(),
            later_value: maturity.to_owned(),
        });
    }
    Ok(())
}

/// Validate that two CSV lists have equal length.
pub fn validate_list_lengths(
    func: &'static str,
    arg_a: &'static str,
    arg_b: &'static str,
    len_a: usize,
    len_b: usize,
) -> Result<(), FinError> {
    if len_a != len_b {
        return Err(FinError::LengthMismatch { func, arg_a, arg_b, len_a, len_b });
    }
    Ok(())
}

/// Validate that a list is non-empty.
pub fn validate_non_empty(
    func: &'static str,
    arg: &'static str,
    len: usize,
) -> Result<(), FinError> {
    if len == 0 {
        return Err(FinError::EmptyList { func, arg });
    }
    Ok(())
}

/// Validate that a list has at least `minimum` elements.
pub fn validate_min_length(
    func: &'static str,
    arg: &'static str,
    len: usize,
    minimum: usize,
) -> Result<(), FinError> {
    if len < minimum {
        return Err(FinError::TooFewElements { func, arg, found: len, minimum });
    }
    Ok(())
}

/// Validate that a value is strictly positive (> 0).
pub fn validate_positive(func: &'static str, arg: &'static str, value: f64) -> Result<(), FinError> {
    if value <= 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg,
            value,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: None,
        });
    }
    Ok(())
}

/// Validate that a value is non-negative (≥ 0).
pub fn validate_non_negative(func: &'static str, arg: &'static str, value: f64) -> Result<(), FinError> {
    if value < 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg,
            value,
            min: Some(0.0),
            max: None,
            exclusive_min: false,
            exclusive_max: false,
            hint: None,
        });
    }
    Ok(())
}

/// Validate a par / face value argument (must be positive).
pub fn validate_par(func: &'static str, par: f64) -> Result<(), FinError> {
    if par <= 0.0 {
        return Err(FinError::DivisionByZero { func, arg: "par" });
    }
    Ok(())
}

/// Validate a redemption value (must be positive).
pub fn validate_redemption(func: &'static str, redemption: f64) -> Result<(), FinError> {
    if redemption <= 0.0 {
        return Err(FinError::OutOfRange {
            func,
            arg: "redemption",
            value: redemption,
            min: Some(0.0),
            max: None,
            exclusive_min: true,
            exclusive_max: false,
            hint: Some("'redemption' is the value received at maturity per $100 face value."),
        });
    }
    Ok(())
}

/// Validate a coupon frequency argument (1 = annual, 2 = semi-annual, 4 = quarterly).
pub fn validate_frequency(func: &'static str, frequency: i64) -> Result<(), FinError> {
    if frequency != 1 && frequency != 2 && frequency != 4 {
        return Err(FinError::InvalidCode {
            func,
            arg: "frequency",
            value: frequency,
            valid_values: "1 (annual), 2 (semi-annual), 4 (quarterly)",
            description: "coupon payment frequency",
        });
    }
    Ok(())
}

/// Check a computed result for NaN/Infinity and return a meaningful error.
pub fn check_result(func: &'static str, result: f64) -> Result<f64, FinError> {
    if result.is_nan() {
        return Err(FinError::InvalidResult {
            func,
            detail: "Result is NaN. Check that all inputs are financially meaningful \
                     (e.g. cash flows are not all the same sign for IRR).",
        });
    }
    if result.is_infinite() {
        return Err(FinError::InvalidResult {
            func,
            detail: "Result is infinite. A denominator in the formula evaluated to zero; \
                     check rates, periods, and par values.",
        });
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Parse helpers — wraps std::str::parse with function/arg context
// ---------------------------------------------------------------------------

/// Parse a &str to f64, yielding a FinError::ParseFloat on failure.
pub fn parse_f64(func: &'static str, arg: &'static str, s: &str) -> Result<f64, FinError> {
    s.trim().parse::<f64>().map_err(|_| FinError::ParseFloat {
        func,
        arg,
        value: s.to_owned(),
    })
}

/// Parse a &str to i64, yielding a FinError::ParseInt on failure.
pub fn parse_i64(func: &'static str, arg: &'static str, s: &str) -> Result<i64, FinError> {
    s.trim().parse::<i64>().map_err(|_| FinError::ParseInt {
        func,
        arg,
        value: s.to_owned(),
    })
}

/// Parse a comma-separated string into a Vec<f64>.
pub fn parse_f64_list(func: &'static str, arg: &'static str, s: &str) -> Result<Vec<f64>, FinError> {
    if s.trim().is_empty() {
        return Err(FinError::EmptyList { func, arg });
    }
    s.split(',')
        .map(|tok| {
            tok.trim().parse::<f64>().map_err(|e| FinError::ParseList {
                func,
                arg,
                detail: format!("could not parse '{}' as a number: {}", tok.trim(), e),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_rate_negative() {
        let err = validate_rate("pv", -1.5, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("[pv]"), "missing function name: {msg}");
        assert!(msg.contains("rate"), "missing arg name: {msg}");
        assert!(msg.contains("-1"), "missing bad value: {msg}");
        assert!(msg.contains("5% → 0.05"), "missing hint: {msg}");
    }

    #[test]
    fn test_validate_rate_zero_forbidden() {
        let err = validate_rate("fv", 0.0, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("zero is not supported"), "{msg}");
    }

    #[test]
    fn test_validate_rate_zero_allowed() {
        assert!(validate_rate("fv", 0.0, true).is_ok());
    }

    #[test]
    fn test_validate_basis_invalid() {
        let err = validate_basis("price", 9).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("9"), "{msg}");
        assert!(msg.contains("day-count basis"), "{msg}");
    }

    #[test]
    fn test_validate_payment_type() {
        assert!(validate_payment_type("pmt", 0).is_ok());
        assert!(validate_payment_type("pmt", 1).is_ok());
        let err = validate_payment_type("pmt", 2).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("END") && msg.contains("BEGINNING"), "{msg}");
    }

    #[test]
    fn test_date_order() {
        // Good order
        assert!(validate_date_order(
            "price", "2024-01-01", "2030-06-15", "settlement", "maturity", false
        ).is_ok());

        // Bad order (settlement > maturity)
        let err = validate_date_order(
            "price", "2031-01-01", "2030-06-15", "settlement", "maturity", false
        ).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("settlement"), "{msg}");
        assert!(msg.contains("maturity"), "{msg}");
        assert!(msg.contains("swapped"), "{msg}");
    }

    #[test]
    fn test_length_mismatch() {
        let err = validate_list_lengths("xnpv", "values", "dates", 3, 4).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("3") && msg.contains("4"), "{msg}");
    }

    #[test]
    fn test_parse_f64_list_bad_value() {
        let err = parse_f64_list("npv", "values", "1000,abc,500").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("abc"), "{msg}");
    }

    #[test]
    fn test_parse_f64_list_empty() {
        let err = parse_f64_list("irr", "values", "   ").unwrap_err();
        assert!(matches!(err, FinError::EmptyList { .. }));
    }

    #[test]
    fn test_check_result_nan() {
        let err = check_result("irr", f64::NAN).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("NaN"), "{msg}");
    }

    #[test]
    fn test_validate_frequency() {
        assert!(validate_frequency("price", 1).is_ok());
        assert!(validate_frequency("price", 2).is_ok());
        assert!(validate_frequency("price", 4).is_ok());
        let err = validate_frequency("price", 3).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("annual") && msg.contains("semi-annual"), "{msg}");
    }

    #[test]
    fn test_salvage_gte_cost() {
        let err = validate_salvage("sln", 10_000.0, 8_000.0).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("salvage"), "{msg}");
        assert!(msg.contains("cost"), "{msg}");
    }
}
