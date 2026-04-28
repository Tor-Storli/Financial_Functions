// ── Cash-flow functions ───────────────────────────────────────────────────────
// Excel: NPV, IRR, MIRR, XNPV, XIRR
//
// NOTE: The `financial` crate is NOT used here. Its IRR/MIRR/XIRR implementations
// panic with "cannot unwind" on certain inputs, which bypasses catch_unwind and
// crashes DuckDB. All five functions are implemented from scratch.

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::{read_varchar, parse_csv_dates, write_f64};
use crate::errors::{FinError, parse_f64_list, validate_list_lengths, validate_non_empty, validate_rate};

/// Wrap a FinError into Box<dyn Error> for use with `?`.
#[inline]
fn fe(e: FinError) -> Box<dyn Error> { Box::new(e) }

fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }
fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

// ── Pure-Rust implementations ─────────────────────────────────────────────────

fn npv_calc(rate: f64, values: &[f64]) -> f64 {
    let base = 1.0 + rate;
    if base.abs() < 1e-12 { return f64::INFINITY; }
    values.iter().enumerate()
        .map(|(t, &v)| v / base.powi((t + 1) as i32))
        .sum()
}

fn npv_at_zero(values: &[f64], rate: f64) -> f64 {
    let base = 1.0 + rate;
    if base.abs() < 1e-12 { return f64::INFINITY; }
    values.iter().enumerate()
        .map(|(t, &v)| v / base.powi(t as i32))
        .sum()
}

fn calc_irr(values: &[f64]) -> f64 {
    if values.len() < 2 { return f64::NAN; }
    if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
        return f64::NAN;
    }
    let points: &[f64] = &[-0.9999, -0.5, -0.2, -0.1, -0.01, 0.0,
                             0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 100.0];
    for w in points.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let flo = npv_at_zero(values, lo);
        let fhi = npv_at_zero(values, hi);
        if flo.is_finite() && fhi.is_finite() && flo * fhi <= 0.0 {
            return bisect(|r| npv_at_zero(values, r), lo, hi, 1e-10, 200);
        }
    }
    f64::NAN
}

fn calc_mirr(values: &[f64], finance_rate: f64, reinvest_rate: f64) -> f64 {
    let n = values.len();
    if n < 2 { return f64::NAN; }
    if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
        return f64::NAN;
    }
    let mut numer = 0.0f64;
    let mut denom = 0.0f64;
    for (t, &v) in values.iter().enumerate() {
        if v > 0.0 {
            numer += v / (1.0 + reinvest_rate).powi(t as i32);
        } else if v < 0.0 {
            denom += v / (1.0 + finance_rate).powi(t as i32);
        }
    }
    if denom.abs() < 1e-12 { return f64::NAN; }
    (numer / denom.abs()).powf(1.0 / (n - 1) as f64) * (1.0 + reinvest_rate) - 1.0
}

fn xnpv_at(values: &[f64], year_fracs: &[f64], rate: f64) -> f64 {
    let base = 1.0 + rate;
    if base.abs() < 1e-12 { return f64::INFINITY; }
    values.iter().zip(year_fracs.iter())
        .map(|(&v, &t)| v / base.powf(t))
        .sum()
}

fn calc_xirr(values: &[f64], dates: &[chrono::NaiveDate]) -> f64 {
    if values.len() < 2 || values.len() != dates.len() { return f64::NAN; }
    if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
        return f64::NAN;
    }
    let d0 = dates[0];
    let year_fracs: Vec<f64> = dates.iter()
        .map(|&d| (d - d0).num_days() as f64 / 365.0)
        .collect();
    let points: &[f64] = &[-0.9999, -0.5, -0.2, -0.1, -0.01, 0.0,
                             0.01, 0.1, 0.5, 1.0, 5.0, 10.0];
    for w in points.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let flo = xnpv_at(values, &year_fracs, lo);
        let fhi = xnpv_at(values, &year_fracs, hi);
        if flo.is_finite() && fhi.is_finite() && flo * fhi <= 0.0 {
            return bisect(|r| xnpv_at(values, &year_fracs, r), lo, hi, 1e-10, 200);
        }
    }
    f64::NAN
}

fn bisect<F: Fn(f64) -> f64>(f: F, mut lo: f64, mut hi: f64, tol: f64, max_iter: usize) -> f64 {
    for _ in 0..max_iter {
        let mid = (lo + hi) / 2.0;
        if (hi - lo).abs() < tol { return mid; }
        let fm = f(mid);
        if fm.abs() < 1e-12 { return mid; }
        if f(lo) * fm <= 0.0 { hi = mid; } else { lo = mid; }
    }
    (lo + hi) / 2.0
}

// ── NPV ──────────────────────────────────────────────────────────────────────
pub struct NpvFunction;
impl VScalar for NpvFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), varchar()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate = input.flat_vector(0).as_slice::<f64>()[i];
            let raw  = unsafe { read_varchar(input, 1, i) };

            validate_rate("npv", rate, /*allow_zero=*/true).map_err(fe)?;
            let values = parse_f64_list("npv", "values", &raw).map_err(fe)?;
            validate_non_empty("npv", "values", values.len()).map_err(fe)?;

            write_f64(&mut out, i, npv_calc(rate, &values));
        }
        Ok(())
    }
}

// ── IRR ──────────────────────────────────────────────────────────────────────
pub struct IrrFunction;
impl VScalar for IrrFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let raw = unsafe { read_varchar(input, 0, i) };
            let values = parse_f64_list("irr", "values", &raw).map_err(fe)?;

            if values.len() < 2 {
                return Err(fe(FinError::TooFewElements {
                    func: "irr", arg: "values", found: values.len(), minimum: 2,
                }));
            }
            let has_pos = values.iter().any(|&v| v > 0.0);
            let has_neg = values.iter().any(|&v| v < 0.0);
            if !has_pos || !has_neg {
                return Err(fe(FinError::General {
                    func: "irr",
                    message: "Cash flows must include at least one positive value (inflow) \
                              and one negative value (outflow). \
                              Example: '-10000,3000,4200,6800'.".into(),
                }));
            }

            write_f64(&mut out, i, calc_irr(&values));
        }
        Ok(())
    }
}

// ── MIRR ─────────────────────────────────────────────────────────────────────
pub struct MirrFunction;
impl VScalar for MirrFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let raw           = unsafe { read_varchar(input, 0, i) };
            let finance_rate  = input.flat_vector(1).as_slice::<f64>()[i];
            let reinvest_rate = input.flat_vector(2).as_slice::<f64>()[i];

            let values = parse_f64_list("mirr", "values", &raw).map_err(fe)?;
            validate_rate("mirr", finance_rate, /*allow_zero=*/true).map_err(fe)?;
            validate_rate("mirr", reinvest_rate, /*allow_zero=*/true).map_err(fe)?;

            let has_pos = values.iter().any(|&v| v > 0.0);
            let has_neg = values.iter().any(|&v| v < 0.0);
            if !has_pos || !has_neg {
                return Err(fe(FinError::General {
                    func: "mirr",
                    message: "Cash flows must include at least one positive value (inflow) \
                              and one negative value (outflow). \
                              Example: '-10000,3000,4200,6800'.".into(),
                }));
            }

            write_f64(&mut out, i, calc_mirr(&values, finance_rate, reinvest_rate));
        }
        Ok(())
    }
}

// ── XNPV ─────────────────────────────────────────────────────────────────────
pub struct XnpvFunction;
impl VScalar for XnpvFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), varchar(), varchar()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate  = input.flat_vector(0).as_slice::<f64>()[i];
            let raw_v = unsafe { read_varchar(input, 1, i) };
            let raw_d = unsafe { read_varchar(input, 2, i) };

            validate_rate("xnpv", rate, /*allow_zero=*/true).map_err(fe)?;
            let values = parse_f64_list("xnpv", "values", &raw_v).map_err(fe)?;
            let dates  = parse_csv_dates(&raw_d).map_err(|e| fe(FinError::General {
                func: "xnpv",
                message: format!("Argument 'dates' could not be parsed: {e}. \
                                  Dates must be YYYY-MM-DD, comma-separated. \
                                  Example: '2024-01-01,2024-07-01,2025-01-01'."),
            }))?;

            validate_non_empty("xnpv", "values", values.len()).map_err(fe)?;
            validate_list_lengths("xnpv", "values", "dates", values.len(), dates.len()).map_err(fe)?;

            let d0 = dates[0];
            let yf: Vec<f64> = dates.iter()
                .map(|&d| (d - d0).num_days() as f64 / 365.0)
                .collect();
            write_f64(&mut out, i, xnpv_at(&values, &yf, rate));
        }
        Ok(())
    }
}

// ── XIRR ─────────────────────────────────────────────────────────────────────
pub struct XirrFunction;
impl VScalar for XirrFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let raw_v = unsafe { read_varchar(input, 0, i) };
            let raw_d = unsafe { read_varchar(input, 1, i) };

            let values = parse_f64_list("xirr", "values", &raw_v).map_err(fe)?;
            let dates  = parse_csv_dates(&raw_d).map_err(|e| fe(FinError::General {
                func: "xirr",
                message: format!("Argument 'dates' could not be parsed: {e}. \
                                  Dates must be YYYY-MM-DD, comma-separated. \
                                  Example: '2024-01-01,2024-07-01,2025-01-01'."),
            }))?;

            if values.len() < 2 {
                return Err(fe(FinError::TooFewElements {
                    func: "xirr", arg: "values", found: values.len(), minimum: 2,
                }));
            }
            validate_list_lengths("xirr", "values", "dates", values.len(), dates.len()).map_err(fe)?;

            let has_pos = values.iter().any(|&v| v > 0.0);
            let has_neg = values.iter().any(|&v| v < 0.0);
            if !has_pos || !has_neg {
                return Err(fe(FinError::General {
                    func: "xirr",
                    message: "Cash flows must include at least one positive value (inflow) \
                              and one negative value (outflow).".into(),
                }));
            }

            write_f64(&mut out, i, calc_xirr(&values, &dates));
        }
        Ok(())
    }
}
