// ── Cash-flow functions ───────────────────────────────────────────────────────
// Excel: NPV, IRR, MIRR, XNPV, XIRR
//
// NOTE: The `financial` crate is NOT used here. Its IRR/MIRR/XIRR implementations
// panic with "cannot unwind" on certain inputs, which bypasses catch_unwind and
// crashes DuckDB. All five functions are implemented from scratch.
//
// LIST variants (irr_list, npv_list, mirr_list, xnpv_list, xirr_list,
// fvschedule_list) accept native DOUBLE[] columns — zero CSV parsing overhead.

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::{read_varchar, parse_csv_dates, write_f64};
use crate::errors::{FinError, parse_f64_list, validate_list_lengths,
                    validate_non_empty, validate_rate};

/// Wrap a FinError into Box<dyn Error> for use with `?`.
#[inline]
fn fe(e: FinError) -> Box<dyn Error> { Box::new(e) }

fn dbl()     -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double)  }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }
fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

// ── Pure-Rust math implementations ───────────────────────────────────────────

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

fn bisect<F: Fn(f64) -> f64>(
    f: F, mut lo: f64, mut hi: f64, tol: f64, max_iter: usize,
) -> f64 {
    for _ in 0..max_iter {
        let mid = (lo + hi) / 2.0;
        if (hi - lo).abs() < tol { return mid; }
        let fm = f(mid);
        if fm.abs() < 1e-12 { return mid; }
        if f(lo) * fm <= 0.0 { hi = mid; } else { lo = mid; }
    }
    (lo + hi) / 2.0
}

// ── LIST helpers — must be defined before the LIST VScalar structs ────────────

/// Read a DuckDB LIST(DOUBLE) column at row `row` into Vec<f64>.
fn read_double_list(
    input: &DataChunkHandle,
    col:   usize,
    row:   usize,
) -> Option<Vec<f64>> {
    let list_vec  = input.flat_vector(col);
    let offsets   = list_vec.as_slice::<u64>();
    let sizes     = list_vec.as_slice_with_offset::<u64>(input.len());
    let start     = offsets[row] as usize;
    let len       = sizes[row]   as usize;
    if len == 0 { return None; }
    let child_vec = list_vec.child(0);
    let values    = child_vec.as_slice::<f64>();
    Some(values[start..start + len].to_vec())
}

/// Read a DuckDB LIST(VARCHAR) column at row `row` into Vec<String>.
fn read_string_list(
    input: &DataChunkHandle,
    col:   usize,
    row:   usize,
) -> Option<Vec<String>> {
    let list_vec  = input.flat_vector(col);
    let offsets   = list_vec.as_slice::<u64>();
    let sizes     = list_vec.as_slice_with_offset::<u64>(input.len());
    let start     = offsets[row] as usize;
    let len       = sizes[row]   as usize;
    if len == 0 { return None; }
    let child_vec = list_vec.child(0);
    let strings: Vec<String> = (start..start + len)
        .map(|idx| {
            child_vec
                .as_slice::<duckdb::core::StringView>()[idx]
                .as_str()
                .to_string()
        })
        .collect();
    Some(strings)
}

// ── NPV ──────────────────────────────────────────────────────────────────────

pub struct NpvFunction;
impl VScalar for NpvFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![dbl(), varchar()])]
    }
    unsafe fn invoke(
        _: &(),
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
        for i in 0..len {
            let rate = input.flat_vector(0).as_slice::<f64>()[i];
            let raw  = unsafe { read_varchar(input, 1, i) };
            validate_rate("npv", rate, true).map_err(fe)?;
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
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar()])]
    }
    unsafe fn invoke(
        _: &(),
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
        for i in 0..len {
            let raw    = unsafe { read_varchar(input, 0, i) };
            let values = parse_f64_list("irr", "values", &raw).map_err(fe)?;
            if values.len() < 2 {
                return Err(fe(FinError::TooFewElements {
                    func: "irr", arg: "values",
                    found: values.len(), minimum: 2,
                }));
            }
            if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
                return Err(fe(FinError::General {
                    func: "irr",
                    message: "Cash flows must include at least one positive value \
                              and one negative value. \
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
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), dbl(), dbl()])]
    }
    unsafe fn invoke(
        _: &(),
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
        for i in 0..len {
            let raw           = unsafe { read_varchar(input, 0, i) };
            let finance_rate  = input.flat_vector(1).as_slice::<f64>()[i];
            let reinvest_rate = input.flat_vector(2).as_slice::<f64>()[i];
            let values = parse_f64_list("mirr", "values", &raw).map_err(fe)?;
            validate_rate("mirr", finance_rate,  true).map_err(fe)?;
            validate_rate("mirr", reinvest_rate, true).map_err(fe)?;
            if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
                return Err(fe(FinError::General {
                    func: "mirr",
                    message: "Cash flows must include at least one positive value \
                              and one negative value. \
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
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![dbl(), varchar(), varchar()])]
    }
    unsafe fn invoke(
        _: &(),
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
        for i in 0..len {
            let rate  = input.flat_vector(0).as_slice::<f64>()[i];
            let raw_v = unsafe { read_varchar(input, 1, i) };
            let raw_d = unsafe { read_varchar(input, 2, i) };
            validate_rate("xnpv", rate, true).map_err(fe)?;
            let values = parse_f64_list("xnpv", "values", &raw_v).map_err(fe)?;
            let dates  = parse_csv_dates(&raw_d).map_err(|e| fe(FinError::General {
                func: "xnpv",
                message: format!("Argument 'dates' could not be parsed: {e}. \
                                  Dates must be YYYY-MM-DD, comma-separated. \
                                  Example: '2024-01-01,2024-07-01,2025-01-01'."),
            }))?;
            validate_non_empty("xnpv", "values", values.len()).map_err(fe)?;
            validate_list_lengths(
                "xnpv", "values", "dates", values.len(), dates.len(),
            ).map_err(fe)?;
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
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), varchar()])]
    }
    unsafe fn invoke(
        _: &(),
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
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
                    func: "xirr", arg: "values",
                    found: values.len(), minimum: 2,
                }));
            }
            validate_list_lengths(
                "xirr", "values", "dates", values.len(), dates.len(),
            ).map_err(fe)?;
            if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
                return Err(fe(FinError::General {
                    func: "xirr",
                    message: "Cash flows must include at least one positive value \
                              and one negative value.".into(),
                }));
            }
            write_f64(&mut out, i, calc_xirr(&values, &dates));
        }
        Ok(())
    }
}

// ── LIST variants ─────────────────────────────────────────────────────────────
// Accept native DOUBLE[] columns — zero CSV parsing overhead.
// Helpers read_double_list and read_string_list are defined above.
// ─────────────────────────────────────────────────────────────────────────────

// ── irr_list ──────────────────────────────────────────────────────────────────

pub struct IrrListFunction;
impl VScalar for IrrListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let list = LogicalTypeHandle::list(&dbl());
        vec![ScalarFunctionSignature::exact(vec![list], dbl())]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size    = input.len();
        let mut out = output.flat_vector();
        for i in 0..size {
            let result = match read_double_list(input, 0, i) {
                Some(cf) if cf.len() >= 2 => calc_irr(&cf),
                _                          => f64::NAN,
            };
            write_f64(&mut out, i, result);
        }
        Ok(())
    }
}

// ── npv_list ──────────────────────────────────────────────────────────────────

pub struct NpvListFunction;
impl VScalar for NpvListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let list = LogicalTypeHandle::list(&dbl());
        vec![ScalarFunctionSignature::exact(vec![dbl(), list], dbl())]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size    = input.len();
        let mut out = output.flat_vector();
        let rates   = input.flat_vector(0).as_slice::<f64>().to_vec();
        for i in 0..size {
            let result = match read_double_list(input, 1, i) {
                Some(cf) if !cf.is_empty() => npv_calc(rates[i], &cf),
                _                           => f64::NAN,
            };
            write_f64(&mut out, i, result);
        }
        Ok(())
    }
}

// ── mirr_list ─────────────────────────────────────────────────────────────────

pub struct MirrListFunction;
impl VScalar for MirrListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let list = LogicalTypeHandle::list(&dbl());
        vec![ScalarFunctionSignature::exact(vec![list, dbl(), dbl()], dbl())]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size           = input.len();
        let mut out        = output.flat_vector();
        let finance_rates  = input.flat_vector(1).as_slice::<f64>().to_vec();
        let reinvest_rates = input.flat_vector(2).as_slice::<f64>().to_vec();
        for i in 0..size {
            let result = match read_double_list(input, 0, i) {
                Some(cf) if cf.len() >= 2 => {
                    calc_mirr(&cf, finance_rates[i], reinvest_rates[i])
                }
                _ => f64::NAN,
            };
            write_f64(&mut out, i, result);
        }
        Ok(())
    }
}

// ── xnpv_list ────────────────────────────────────────────────────────────────

pub struct XnpvListFunction;
impl VScalar for XnpvListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let dbl_list = LogicalTypeHandle::list(&dbl());
        let str_list = LogicalTypeHandle::list(&varchar());
        vec![ScalarFunctionSignature::exact(
            vec![dbl(), dbl_list, str_list], dbl(),
        )]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size    = input.len();
        let mut out = output.flat_vector();
        let rates   = input.flat_vector(0).as_slice::<f64>().to_vec();
        for i in 0..size {
            let rate   = rates[i];
            let values = match read_double_list(input, 1, i) {
                Some(v) if v.len() >= 2 => v,
                _ => { write_f64(&mut out, i, f64::NAN); continue; }
            };
            let date_strs = match read_string_list(input, 2, i) {
                Some(d) if d.len() == values.len() => d,
                _ => { write_f64(&mut out, i, f64::NAN); continue; }
            };
            let dates: Vec<chrono::NaiveDate> = date_strs.iter()
                .filter_map(|s| {
                    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
                })
                .collect();
            if dates.len() != values.len() {
                write_f64(&mut out, i, f64::NAN);
                continue;
            }
            let d0 = dates[0];
            let yf: Vec<f64> = dates.iter()
                .map(|&d| (d - d0).num_days() as f64 / 365.0)
                .collect();
            write_f64(&mut out, i, xnpv_at(&values, &yf, rate));
        }
        Ok(())
    }
}

// ── xirr_list ────────────────────────────────────────────────────────────────

pub struct XirrListFunction;
impl VScalar for XirrListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let dbl_list = LogicalTypeHandle::list(&dbl());
        let str_list = LogicalTypeHandle::list(&varchar());
        vec![ScalarFunctionSignature::exact(
            vec![dbl_list, str_list], dbl(),
        )]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size    = input.len();
        let mut out = output.flat_vector();
        for i in 0..size {
            let values = match read_double_list(input, 0, i) {
                Some(v) if v.len() >= 2 => v,
                _ => { write_f64(&mut out, i, f64::NAN); continue; }
            };
            let date_strs = match read_string_list(input, 1, i) {
                Some(d) if d.len() == values.len() => d,
                _ => { write_f64(&mut out, i, f64::NAN); continue; }
            };
            let dates: Vec<chrono::NaiveDate> = date_strs.iter()
                .filter_map(|s| {
                    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
                })
                .collect();
            if dates.len() != values.len() {
                write_f64(&mut out, i, f64::NAN);
                continue;
            }
            write_f64(&mut out, i, calc_xirr(&values, &dates));
        }
        Ok(())
    }
}

// ── fvschedule_list ───────────────────────────────────────────────────────────

pub struct FvscheduleListFunction;
impl VScalar for FvscheduleListFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        let list = LogicalTypeHandle::list(&dbl());
        vec![ScalarFunctionSignature::exact(vec![dbl(), list], dbl())]
    }
    unsafe fn invoke(
        _state: &Self::State,
        input:  &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>> {
        let size       = input.len();
        let mut out    = output.flat_vector();
        let principals = input.flat_vector(0).as_slice::<f64>().to_vec();
        for i in 0..size {
            let result = match read_double_list(input, 1, i) {
                Some(schedule) if !schedule.is_empty() => {
                    schedule.iter().fold(principals[i], |acc, &r| acc * (1.0 + r))
                }
                _ => f64::NAN,
            };
            write_f64(&mut out, i, result);
        }
        Ok(())
    }
}
