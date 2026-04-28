// ── Miscellaneous financial functions ────────────────────────────────────────
// Excel: EFFECT, NOMINAL, DOLLARDE, DOLLARFR, FVSCHEDULE, RRI, PDURATION,
//        TBILLEQ, TBILLPRICE, TBILLYIELD, ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::{read_varchar, parse_date, year_frac, freq_per_year, write_f64, add_months};
use crate::errors::{FinError, validate_basis, validate_frequency, validate_date_order, parse_f64_list};

/// Wrap a FinError into Box<dyn Error> for use with `?`.
#[inline]
fn fe(e: FinError) -> Box<dyn Error> { Box::new(e) }

fn dbl()     -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }

fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

// ── fn_impl! macro ────────────────────────────────────────────────────────────
//
// KEY CHANGE: body closure now returns Result<f64, Box<dyn Error>>.
// Errors propagate up via `?` — DuckDB shows them instead of silently writing NULL.

macro_rules! fn_impl {
    ($struct:ident, $sig:expr, $body:expr) => {
        pub struct $struct;
        impl VScalar for $struct {
            type State = ();
            fn signatures() -> Vec<ScalarFunctionSignature> { vec![$sig] }
            unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector)
                -> Result<(), Box<dyn Error>>
            {
                let len = input.len();
                let mut out = output.flat_vector();
                #[allow(clippy::redundant_closure_call)]
                for i in 0..len {
                    let result: Result<f64, Box<dyn Error>> = $body(input, i);
                    match result {
                        Ok(v)  => write_f64(&mut out, i, v),
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
        }
    };
}

// ── EFFECT ────────────────────────────────────────────────────────────────────
fn_impl!(EffectFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let nominal = input.flat_vector(0).as_slice::<f64>()[i];
        let npery   = input.flat_vector(1).as_slice::<f64>()[i];
        if npery < 1.0 {
            return Err(fe(FinError::OutOfRange {
                func: "effect", arg: "npery", value: npery,
                min: Some(1.0), max: None,
                exclusive_min: false, exclusive_max: false,
                hint: Some("'npery' is the number of compounding periods per year (e.g. 12 for monthly, 4 for quarterly)."),
            }));
        }
        Ok((1.0 + nominal / npery).powf(npery) - 1.0)
    }
);

// ── NOMINAL ───────────────────────────────────────────────────────────────────
fn_impl!(NominalFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let effect = input.flat_vector(0).as_slice::<f64>()[i];
        let npery  = input.flat_vector(1).as_slice::<f64>()[i];
        if npery < 1.0 {
            return Err(fe(FinError::OutOfRange {
                func: "nominal", arg: "npery", value: npery,
                min: Some(1.0), max: None,
                exclusive_min: false, exclusive_max: false,
                hint: Some("'npery' is the number of compounding periods per year (e.g. 12 for monthly, 4 for quarterly)."),
            }));
        }
        Ok(((1.0 + effect).powf(1.0 / npery) - 1.0) * npery)
    }
);

// ── DOLLARDE ──────────────────────────────────────────────────────────────────
fn_impl!(DollardeFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let frac_dollar = input.flat_vector(0).as_slice::<f64>()[i];
        let fraction    = input.flat_vector(1).as_slice::<f64>()[i].floor();
        if fraction < 1.0 {
            return Err(fe(FinError::OutOfRange {
                func: "dollarde", arg: "fraction", value: fraction,
                min: Some(1.0), max: None,
                exclusive_min: false, exclusive_max: false,
                hint: Some("'fraction' is the denominator (e.g. 32 for 32nds). Must be >= 1."),
            }));
        }
        let integer_part = frac_dollar.floor();
        let decimal_part = frac_dollar - integer_part;
        Ok(integer_part + decimal_part / fraction * 10.0f64.powf(fraction.log10().ceil()))
    }
);

// ── DOLLARFR ──────────────────────────────────────────────────────────────────
fn_impl!(DollarfrFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let dec_dollar = input.flat_vector(0).as_slice::<f64>()[i];
        let fraction   = input.flat_vector(1).as_slice::<f64>()[i].floor();
        if fraction < 1.0 {
            return Err(fe(FinError::OutOfRange {
                func: "dollarfr", arg: "fraction", value: fraction,
                min: Some(1.0), max: None,
                exclusive_min: false, exclusive_max: false,
                hint: Some("'fraction' is the denominator (e.g. 32 for 32nds). Must be >= 1."),
            }));
        }
        let integer_part = dec_dollar.floor();
        let frac_part    = dec_dollar - integer_part;
        Ok(integer_part + frac_part * fraction / 10.0f64.powf(fraction.log10().ceil()))
    }
);

// ── FVSCHEDULE ────────────────────────────────────────────────────────────────
pub struct FvscheduleFunction;
impl VScalar for FvscheduleFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), varchar()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let principal = input.flat_vector(0).as_slice::<f64>()[i];
            let raw       = unsafe { read_varchar(input, 1, i) };
            let rates     = parse_f64_list("fvschedule", "schedule", &raw).map_err(fe)?;
            let fv        = rates.iter().fold(principal, |acc, &r| acc * (1.0 + r));
            write_f64(&mut out, i, fv);
        }
        Ok(())
    }
}

// ── RRI ───────────────────────────────────────────────────────────────────────
fn_impl!(RriFunction, sig(vec![dbl(), dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let nper = input.flat_vector(0).as_slice::<f64>()[i];
        let pv   = input.flat_vector(1).as_slice::<f64>()[i];
        let fv   = input.flat_vector(2).as_slice::<f64>()[i];
        if nper <= 0.0 {
            return Err(fe(FinError::OutOfRange {
                func: "rri", arg: "nper", value: nper,
                min: Some(0.0), max: None, exclusive_min: true, exclusive_max: false,
                hint: Some("'nper' is the number of periods and must be positive."),
            }));
        }
        if pv == 0.0 {
            return Err(fe(FinError::DivisionByZero { func: "rri", arg: "pv" }));
        }
        Ok((fv / pv).powf(1.0 / nper) - 1.0)
    }
);

// ── PDURATION ─────────────────────────────────────────────────────────────────
fn_impl!(PdurationFunction, sig(vec![dbl(), dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| -> Result<f64, Box<dyn Error>> {
        let rate = input.flat_vector(0).as_slice::<f64>()[i];
        let pv   = input.flat_vector(1).as_slice::<f64>()[i];
        let fv   = input.flat_vector(2).as_slice::<f64>()[i];
        if rate <= -1.0 {
            return Err(fe(FinError::OutOfRange {
                func: "pduration", arg: "rate", value: rate,
                min: Some(-1.0), max: None, exclusive_min: true, exclusive_max: false,
                hint: Some("Express as a decimal: 5% → 0.05."),
            }));
        }
        if pv == 0.0 {
            return Err(fe(FinError::DivisionByZero { func: "pduration", arg: "pv" }));
        }
        let ln_denom = (1.0 + rate).ln();
        if ln_denom.abs() < 1e-12 {
            return Err(fe(FinError::DivisionByZero { func: "pduration", arg: "rate" }));
        }
        Ok((fv / pv).ln() / ln_denom)
    }
);

// ── TBILLPRICE ────────────────────────────────────────────────────────────────
pub struct TbillpriceFunction;
impl VScalar for TbillpriceFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s = unsafe { read_varchar(input, 0, i) };
            let m = unsafe { read_varchar(input, 1, i) };
            let settle = parse_date(&s).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbillprice", arg: "settlement", value: s.clone(),
            }))?;
            let mature = parse_date(&m).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbillprice", arg: "maturity", value: m.clone(),
            }))?;
            validate_date_order("tbillprice", &s, &m, "settlement", "maturity", false).map_err(fe)?;
            let discount = input.flat_vector(2).as_slice::<f64>()[i];
            let days     = (mature - settle).num_days() as f64;
            write_f64(&mut out, i, 100.0 * (1.0 - discount * days / 360.0));
        }
        Ok(())
    }
}

// ── TBILLYIELD ────────────────────────────────────────────────────────────────
pub struct TbillyieldFunction;
impl VScalar for TbillyieldFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s = unsafe { read_varchar(input, 0, i) };
            let m = unsafe { read_varchar(input, 1, i) };
            let settle = parse_date(&s).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbillyield", arg: "settlement", value: s.clone(),
            }))?;
            let mature = parse_date(&m).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbillyield", arg: "maturity", value: m.clone(),
            }))?;
            validate_date_order("tbillyield", &s, &m, "settlement", "maturity", false).map_err(fe)?;
            let pr   = input.flat_vector(2).as_slice::<f64>()[i];
            let days = (mature - settle).num_days() as f64;
            if pr == 0.0 {
                return Err(fe(FinError::DivisionByZero { func: "tbillyield", arg: "pr" }));
            }
            if days == 0.0 {
                return Err(fe(FinError::General {
                    func: "tbillyield",
                    message: "Settlement and maturity are the same date; period is zero.".into(),
                }));
            }
            write_f64(&mut out, i, (100.0 - pr) / pr * 360.0 / days);
        }
        Ok(())
    }
}

// ── TBILLEQ ───────────────────────────────────────────────────────────────────
pub struct TbilleqFunction;
impl VScalar for TbilleqFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s = unsafe { read_varchar(input, 0, i) };
            let m = unsafe { read_varchar(input, 1, i) };
            let settle = parse_date(&s).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbilleq", arg: "settlement", value: s.clone(),
            }))?;
            let mature = parse_date(&m).ok_or_else(|| fe(FinError::ParseDate {
                func: "tbilleq", arg: "maturity", value: m.clone(),
            }))?;
            validate_date_order("tbilleq", &s, &m, "settlement", "maturity", false).map_err(fe)?;
            let discount = input.flat_vector(2).as_slice::<f64>()[i];
            let days     = (mature - settle).num_days() as f64;
            if days <= 0.0 {
                return Err(fe(FinError::General {
                    func: "tbilleq",
                    message: "Settlement and maturity are the same date; period is zero.".into(),
                }));
            }
            let result = if days <= 182.0 {
                let denom = 360.0 - discount * days;
                if denom.abs() < 1e-12 {
                    return Err(fe(FinError::DivisionByZero { func: "tbilleq", arg: "discount" }));
                }
                365.0 * discount / denom
            } else {
                let a     = discount * days / 360.0;
                let denom = 1.0 - a;
                if denom.abs() < 1e-12 {
                    return Err(fe(FinError::DivisionByZero { func: "tbilleq", arg: "discount" }));
                }
                let inner = 2.0 * a / denom + 1.0;
                if inner < 0.0 {
                    return Err(fe(FinError::InvalidResult {
                        func: "tbilleq",
                        detail: "Square root argument is negative. Check that the discount rate is reasonable.",
                    }));
                }
                (2.0 * (inner.sqrt() - 1.0)) * 365.0 / days
            };
            write_f64(&mut out, i, result);
        }
        Ok(())
    }
}

// ── Odd period calculation helpers ────────────────────────────────────────────

fn basis_days(d1: chrono::NaiveDate, d2: chrono::NaiveDate, basis: i32) -> f64 {
    match basis {
        0 | 4 => year_frac(d1, d2, basis) * 360.0,
        _     => (d2 - d1).num_days() as f64,
    }
}

fn quasi_period(start: chrono::NaiveDate, end: chrono::NaiveDate, basis: i32, freq: i32) -> f64 {
    match basis {
        0 | 4 => 360.0 / freq as f64,
        _     => (end - start).num_days() as f64,
    }
}

fn count_coupon_periods(from: chrono::NaiveDate, to: chrono::NaiveDate, months: i32) -> i32 {
    let mut count = 0i32;
    let mut d = to;
    while d > from { count += 1; d = add_months(d, -months); }
    count
}

pub fn calc_oddfprice(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
    rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let coupon  = rate * 100.0 / freq as f64;
    let yp      = yld / freq as f64;
    if (1.0 + yp).abs() < 1e-12 { return f64::NAN; }
    let v       = 1.0 / (1.0 + yp);
    let months  = 12 / freq;
    let qc_prev = add_months(first_coupon, -(months as i32));
    let e       = quasi_period(qc_prev, first_coupon, basis, freq);
    if e == 0.0 { return f64::NAN; }
    let dsc = basis_days(settle, first_coupon, basis);
    let w   = dsc / e;
    let n   = count_coupon_periods(first_coupon, mature, months as i32);
    let (odd_coupon, accrued) = if issue >= qc_prev {
        let dfc = basis_days(issue, first_coupon, basis);
        let dci = basis_days(issue, settle, basis);
        (coupon * dfc / e, coupon * dci / e)
    } else {
        let mut odd = 0.0f64;
        let mut acc = 0.0f64;
        let mut qc_end = first_coupon;
        loop {
            let qc_start     = add_months(qc_end, -(months as i32));
            let ei           = quasi_period(qc_start, qc_end, basis, freq);
            if ei == 0.0 { break; }
            let actual_start = if issue > qc_start { issue } else { qc_start };
            let dc           = basis_days(actual_start, qc_end, basis);
            odd += coupon * dc / ei;
            if settle >= actual_start && settle < qc_end {
                acc += coupon * basis_days(actual_start, settle, basis) / ei;
            } else if settle >= qc_end {
                acc += coupon * dc / ei;
            }
            if issue >= qc_start { break; }
            qc_end = qc_start;
        }
        (odd, acc)
    };
    let vw            = v.powf(w);
    let sum_regular: f64 = (1..=n).map(|k| v.powf(k as f64)).sum::<f64>() * coupon;
    let pv_redemption = redemption * v.powf(n as f64);
    vw * (odd_coupon + sum_regular + pv_redemption) - accrued
}

pub fn calc_oddfyield(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
    rate: f64, pr: f64, redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let f = |y: f64| calc_oddfprice(settle, mature, issue, first_coupon,
                                     rate, y, redemption, freq, basis) - pr;
    bisect(f, 0.0, 1.0, 1e-9, 200)
}

pub fn calc_oddlprice(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    last_interest: chrono::NaiveDate, rate: f64, yld: f64,
    redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let coupon = rate * 100.0 / freq as f64;
    let yp     = yld / freq as f64;
    if (1.0 + yp).abs() < 1e-12 { return f64::NAN; }
    let v      = 1.0 / (1.0 + yp);
    let months = 12 / freq;
    let qc_ref_end = add_months(last_interest, months as i32);
    let e          = quasi_period(last_interest, qc_ref_end, basis, freq);
    if e == 0.0 { return f64::NAN; }
    let dcl        = basis_days(last_interest, mature, basis);
    let nl         = dcl / e;
    let odd_coupon = coupon * nl;
    if settle >= last_interest {
        let dsm     = basis_days(settle, mature, basis);
        let w       = dsm / e;
        let dirty   = (odd_coupon + redemption) * v.powf(w);
        let dci     = basis_days(last_interest, settle, basis);
        let accrued = odd_coupon * dci / dcl;
        dirty - accrued
    } else {
        let mut qc_end = last_interest;
        loop {
            let prev = add_months(qc_end, -(months as i32));
            if prev <= settle { break; }
            qc_end = prev;
        }
        let qc_prev  = add_months(qc_end, -(months as i32));
        let e_settle = quasi_period(qc_prev, qc_end, basis, freq);
        if e_settle == 0.0 { return f64::NAN; }
        let dsc      = basis_days(settle, qc_end, basis);
        let w        = dsc / e_settle;
        let n_full   = count_coupon_periods(qc_end, last_interest, months as i32);
        let sum_reg: f64 = (0..=n_full).map(|k| coupon * v.powf(w + k as f64)).sum();
        let pv_final = (odd_coupon + redemption) * v.powf(w + n_full as f64 + nl);
        let dirty    = sum_reg + pv_final;
        let accrued  = coupon * (e_settle - dsc) / e_settle;
        dirty - accrued
    }
}

pub fn calc_oddlyield(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    last_interest: chrono::NaiveDate, rate: f64, pr: f64,
    redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let f = |y: f64| calc_oddlprice(settle, mature, last_interest, rate, y, redemption, freq, basis) - pr;
    bisect(f, 0.0, 1.0, 1e-9, 100)
}

fn bisect<F: Fn(f64) -> f64>(f: F, mut lo: f64, mut hi: f64, tol: f64, max_iter: usize) -> f64 {
    for _ in 0..max_iter {
        let mid = (lo + hi) / 2.0;
        if (hi - lo) < tol { return mid; }
        if f(lo) * f(mid) < 0.0 { hi = mid; } else { lo = mid; }
    }
    (lo + hi) / 2.0
}

// ── Helper: parse two settlement/maturity date VARCHAR args ───────────────────
fn parse_two_dates_misc(
    func: &'static str, s: &str, m: &str,
) -> Result<(chrono::NaiveDate, chrono::NaiveDate), Box<dyn Error>> {
    let settle = parse_date(s).ok_or_else(|| fe(FinError::ParseDate {
        func, arg: "settlement", value: s.to_owned(),
    }))?;
    let mature = parse_date(m).ok_or_else(|| fe(FinError::ParseDate {
        func, arg: "maturity", value: m.to_owned(),
    }))?;
    validate_date_order(func, s, m, "settlement", "maturity", false).map_err(fe)?;
    Ok((settle, mature))
}

// ── ODDFPRICE ────────────────────────────────────────────────────────────────
pub struct OddfpriceFunction;
impl VScalar for OddfpriceFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), varchar(), varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()])]
    }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s  = unsafe { read_varchar(input, 0, i) };
            let m  = unsafe { read_varchar(input, 1, i) };
            let is = unsafe { read_varchar(input, 2, i) };
            let fc = unsafe { read_varchar(input, 3, i) };
            let (settle, mature) = parse_two_dates_misc("oddfprice", &s, &m)?;
            let issue = parse_date(&is).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddfprice", arg: "issue", value: is.clone(),
            }))?;
            let first = parse_date(&fc).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddfprice", arg: "first_coupon", value: fc.clone(),
            }))?;
            let freq_raw  = input.flat_vector(7).as_slice::<f64>()[i] as i64;
            let basis_raw = input.flat_vector(8).as_slice::<f64>()[i] as i64;
            validate_frequency("oddfprice", freq_raw).map_err(fe)?;
            validate_basis("oddfprice", basis_raw).map_err(fe)?;
            write_f64(&mut out, i, calc_oddfprice(
                settle, mature, issue, first,
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                input.flat_vector(6).as_slice::<f64>()[i],
                freq_per_year(freq_raw as i32), basis_raw as i32,
            ));
        }
        Ok(())
    }
}

// ── ODDFYIELD ────────────────────────────────────────────────────────────────
pub struct OddfyieldFunction;
impl VScalar for OddfyieldFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), varchar(), varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()])]
    }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s  = unsafe { read_varchar(input, 0, i) };
            let m  = unsafe { read_varchar(input, 1, i) };
            let is = unsafe { read_varchar(input, 2, i) };
            let fc = unsafe { read_varchar(input, 3, i) };
            let (settle, mature) = parse_two_dates_misc("oddfyield", &s, &m)?;
            let issue = parse_date(&is).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddfyield", arg: "issue", value: is.clone(),
            }))?;
            let first = parse_date(&fc).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddfyield", arg: "first_coupon", value: fc.clone(),
            }))?;
            let freq_raw  = input.flat_vector(7).as_slice::<f64>()[i] as i64;
            let basis_raw = input.flat_vector(8).as_slice::<f64>()[i] as i64;
            validate_frequency("oddfyield", freq_raw).map_err(fe)?;
            validate_basis("oddfyield", basis_raw).map_err(fe)?;
            write_f64(&mut out, i, calc_oddfyield(
                settle, mature, issue, first,
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                input.flat_vector(6).as_slice::<f64>()[i],
                freq_per_year(freq_raw as i32), basis_raw as i32,
            ));
        }
        Ok(())
    }
}

// ── ODDLPRICE ────────────────────────────────────────────────────────────────
pub struct OddlpriceFunction;
impl VScalar for OddlpriceFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()])]
    }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s  = unsafe { read_varchar(input, 0, i) };
            let m  = unsafe { read_varchar(input, 1, i) };
            let li = unsafe { read_varchar(input, 2, i) };
            let (settle, mature) = parse_two_dates_misc("oddlprice", &s, &m)?;
            let last = parse_date(&li).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddlprice", arg: "last_interest", value: li.clone(),
            }))?;
            let freq_raw  = input.flat_vector(6).as_slice::<f64>()[i] as i64;
            let basis_raw = input.flat_vector(7).as_slice::<f64>()[i] as i64;
            validate_frequency("oddlprice", freq_raw).map_err(fe)?;
            validate_basis("oddlprice", basis_raw).map_err(fe)?;
            write_f64(&mut out, i, calc_oddlprice(
                settle, mature, last,
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                freq_per_year(freq_raw as i32), basis_raw as i32,
            ));
        }
        Ok(())
    }
}

// ── ODDLYIELD ────────────────────────────────────────────────────────────────
pub struct OddlyieldFunction;
impl VScalar for OddlyieldFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![sig(vec![varchar(), varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()])]
    }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let s  = unsafe { read_varchar(input, 0, i) };
            let m  = unsafe { read_varchar(input, 1, i) };
            let li = unsafe { read_varchar(input, 2, i) };
            let (settle, mature) = parse_two_dates_misc("oddlyield", &s, &m)?;
            let last = parse_date(&li).ok_or_else(|| fe(FinError::ParseDate {
                func: "oddlyield", arg: "last_interest", value: li.clone(),
            }))?;
            let freq_raw  = input.flat_vector(6).as_slice::<f64>()[i] as i64;
            let basis_raw = input.flat_vector(7).as_slice::<f64>()[i] as i64;
            validate_frequency("oddlyield", freq_raw).map_err(fe)?;
            validate_basis("oddlyield", basis_raw).map_err(fe)?;
            write_f64(&mut out, i, calc_oddlyield(
                settle, mature, last,
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                freq_per_year(freq_raw as i32), basis_raw as i32,
            ));
        }
        Ok(())
    }
}
