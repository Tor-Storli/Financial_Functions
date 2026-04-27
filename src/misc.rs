// ── Miscellaneous financial functions ────────────────────────────────────────
// Excel: EFFECT, NOMINAL, DOLLARDE, DOLLARFR, FVSCHEDULE, RRI, PDURATION,
//        TBILLEQ, TBILLPRICE, TBILLYIELD, ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::{read_varchar, parse_date, parse_csv_f64, year_frac, freq_per_year, write_f64, add_months};
// use crate::coupons::{next_coupon, calc_coupdays, calc_coupnum};

fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }

fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

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
                    let result = $body(input, i);
                    write_f64(&mut out, i, result);
                }
                Ok(())
            }
        }
    };
}

// ── EFFECT ────────────────────────────────────────────────────────────────────
// Excel: EFFECT(nominal_rate, npery)
fn_impl!(EffectFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let nominal = input.flat_vector(0).as_slice::<f64>()[i];
        let npery   = input.flat_vector(1).as_slice::<f64>()[i];
        if npery == 0.0 { return f64::NAN; }
        (1.0 + nominal / npery).powf(npery) - 1.0
    }
);

// ── NOMINAL ───────────────────────────────────────────────────────────────────
// Excel: NOMINAL(effect_rate, npery)
fn_impl!(NominalFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let effect = input.flat_vector(0).as_slice::<f64>()[i];
        let npery  = input.flat_vector(1).as_slice::<f64>()[i];
        if npery == 0.0 { return f64::NAN; }
        ((1.0 + effect).powf(1.0 / npery) - 1.0) * npery
    }
);

// ── DOLLARDE ──────────────────────────────────────────────────────────────────
fn_impl!(DollardeFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let frac_dollar = input.flat_vector(0).as_slice::<f64>()[i];
        let fraction    = input.flat_vector(1).as_slice::<f64>()[i].floor();
        if fraction == 0.0 { return f64::NAN; }
        let integer_part = frac_dollar.floor();
        let decimal_part = frac_dollar - integer_part;
        integer_part + decimal_part / fraction * 10.0f64.powf(fraction.log10().ceil())
    }
);

// ── DOLLARFR ──────────────────────────────────────────────────────────────────
fn_impl!(DollarfrFunction, sig(vec![dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let dec_dollar = input.flat_vector(0).as_slice::<f64>()[i];
        let fraction   = input.flat_vector(1).as_slice::<f64>()[i].floor();
        if fraction == 0.0 { return f64::NAN; }
        let integer_part = dec_dollar.floor();
        let frac_part   = dec_dollar - integer_part;
        integer_part + frac_part * fraction / 10.0f64.powf(fraction.log10().ceil())
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
            match parse_csv_f64(&raw) {
                Ok(rates) => {
                    let fv = rates.iter().fold(principal, |acc, &r| acc * (1.0 + r));
                    write_f64(&mut out, i, fv);
                }
                Err(_) => out.set_null(i),
            }
        }
        Ok(())
    }
}

// ── RRI ───────────────────────────────────────────────────────────────────────
fn_impl!(RriFunction, sig(vec![dbl(), dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let nper = input.flat_vector(0).as_slice::<f64>()[i];
        let pv   = input.flat_vector(1).as_slice::<f64>()[i];
        let fv   = input.flat_vector(2).as_slice::<f64>()[i];
        if nper == 0.0 || pv == 0.0 { return f64::NAN; }
        (fv / pv).powf(1.0 / nper) - 1.0
    }
);

// ── PDURATION ─────────────────────────────────────────────────────────────────
fn_impl!(PdurationFunction, sig(vec![dbl(), dbl(), dbl()]),
    |input: &mut DataChunkHandle, i: usize| {
        let rate = input.flat_vector(0).as_slice::<f64>()[i];
        let pv   = input.flat_vector(1).as_slice::<f64>()[i];
        let fv   = input.flat_vector(2).as_slice::<f64>()[i];
        if pv == 0.0 || rate <= -1.0 { return f64::NAN; }
        let ln_denom = (1.0 + rate).ln();
        if ln_denom.abs() < 1e-12 { return f64::NAN; }
        (fv / pv).ln() / ln_denom
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
            match (parse_date(&s), parse_date(&m)) {
                (Some(settle), Some(mature)) => {
                    let discount = input.flat_vector(2).as_slice::<f64>()[i];
                    let days = (mature - settle).num_days() as f64;
                    write_f64(&mut out, i, 100.0 * (1.0 - discount * days / 360.0));
                }
                _ => out.set_null(i),
            }
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
            match (parse_date(&s), parse_date(&m)) {
                (Some(settle), Some(mature)) => {
                    let pr   = input.flat_vector(2).as_slice::<f64>()[i];
                    let days = (mature - settle).num_days() as f64;
                    if pr == 0.0 || days == 0.0 { out.set_null(i); continue; }
                    write_f64(&mut out, i, (100.0 - pr) / pr * 360.0 / days);
                }
                _ => out.set_null(i),
            }
        }
        Ok(())
    }
}

// // ── TBILLEQ ───────────────────────────────────────────────────────────────────
pub struct TbilleqFunction;
impl VScalar for TbilleqFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len();
        let mut out = output.flat_vector();
        for i in 0..len {
            let s = unsafe { read_varchar(input, 0, i) };
            let m = unsafe { read_varchar(input, 1, i) };
            match (parse_date(&s), parse_date(&m)) {
                (Some(settle), Some(mature)) => {
                    let discount = input.flat_vector(2).as_slice::<f64>()[i];
                    let days = (mature - settle).num_days() as f64;
                    if days <= 0.0 { out.set_null(i); continue; }

                    let result = if days <= 182.0 {
                        // Simple formula for T-bills up to 182 days
                        let denom = 360.0 - discount * days;
                        if denom.abs() < 1e-12 { out.set_null(i); continue; }
                        365.0 * discount / denom
                    } else {
                        // Excel's formula for T-bills over 182 days (quadratic solution)
                        let a = discount * days / 360.0;
                        let denom = 1.0 - a;
                        if denom.abs() < 1e-12 { out.set_null(i); continue; }
                        let inner = (2.0 * a / denom + 1.0).sqrt();
                        if inner < 0.0 { out.set_null(i); continue; }
                        (2.0 * (inner - 1.0)) * 365.0 / days
                    };
                    write_f64(&mut out, i, result);
                }
                _ => out.set_null(i),
            }
        }
        Ok(())
    }
}

// pub struct TbilleqFunction;
// impl VScalar for TbilleqFunction {
//     type State = ();
//     fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![varchar(), varchar(), dbl()])] }
//     unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
//         let len = input.len(); let mut out = output.flat_vector();
//         for i in 0..len {
//             let s = unsafe { read_varchar(input, 0, i) };
//             let m = unsafe { read_varchar(input, 1, i) };
//             match (parse_date(&s), parse_date(&m)) {
//                 (Some(settle), Some(mature)) => {
//                     let discount = input.flat_vector(2).as_slice::<f64>()[i];
//                     let days = (mature - settle).num_days() as f64;
//                     if days == 0.0 { out.set_null(i); continue; }
//                     let denom = 1.0 - discount * days / 360.0;
//                     if denom.abs() < 1e-12 { out.set_null(i); continue; }
//                     write_f64(&mut out, i, (365.0 * discount) / denom);
//                 }
//                 _ => out.set_null(i),
//             }
//         }
//         Ok(())
//     }
// }

// ── Odd period helpers ────────────────────────────────────────────────────────

// pub fn calc_oddfprice(
//     settle: chrono::NaiveDate, mature: chrono::NaiveDate,
//     issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
//     rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32,
// ) -> f64 {
//     let coupon = rate * 100.0 / freq as f64;
//     let y = yld / freq as f64;
//     let n_full = calc_coupnum(settle, mature, freq) as i32;
//     if n_full < 0 { return f64::NAN; }
//     let odd_days = year_frac(issue, first_coupon, basis);
//     let odd_coupon = coupon * odd_days * freq as f64;
//     let dsc = (next_coupon(settle, mature, freq) - settle).num_days() as f64;
//     let e   = calc_coupdays(settle, mature, freq, basis);
//     if e == 0.0 { return f64::NAN; }
//     let t = 1.0 + dsc / e * y;
//     if t.abs() < 1e-12 { return f64::NAN; }
//     let mut price = odd_coupon / t;
//     for k in 1..=n_full as usize {
//         price += coupon / t.powf(k as f64 + 1.0);
//     }
//     price += redemption / t.powf(n_full as f64 + 1.0);
//     let accrued = if settle > issue {
//         coupon * year_frac(issue, settle, basis) * freq as f64
//     } else { 0.0 };
//     price - accrued
// }

// pub fn calc_oddfyield(
//     settle: chrono::NaiveDate, mature: chrono::NaiveDate,
//     issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
//     rate: f64, pr: f64, redemption: f64, freq: i32, basis: i32,
// ) -> f64 {
//     let f = |y: f64| calc_oddfprice(settle, mature, issue, first_coupon, rate, y, redemption, freq, basis) - pr;
//     bisect(f, 0.0, 1.0, 1e-9, 100)
// }

// ── Helper: actual days between two dates respecting basis ──────────────────
fn basis_days(d1: chrono::NaiveDate, d2: chrono::NaiveDate, basis: i32) -> f64 {
    match basis {
        0 | 4 => year_frac(d1, d2, basis) * 360.0,
        _     => (d2 - d1).num_days() as f64,
    }
}

// ── Helper: quasi-coupon period length ──────────────────────────────────────
fn quasi_period(start: chrono::NaiveDate, end: chrono::NaiveDate, basis: i32, freq: i32) -> f64 {
    match basis {
        0 | 4 => 360.0 / freq as f64,
        _     => (end - start).num_days() as f64,
    }
}

// ── Helper: count coupon periods from `from` to `to` ────────────────────────
fn count_coupon_periods(from: chrono::NaiveDate, to: chrono::NaiveDate, months: i32) -> i32 {
    let mut count = 0i32;
    let mut d = to;
    while d > from {
        count += 1;
        d = add_months(d, -months);
    }
    count
}

pub fn calc_oddfprice(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
    rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let coupon = rate * 100.0 / freq as f64;
    let yp     = yld / freq as f64;
    if (1.0 + yp).abs() < 1e-12 { return f64::NAN; }
    let v      = 1.0 / (1.0 + yp);
    let months = 12 / freq;

    // Quasi-coupon period that contains settlement ends at first_coupon
    let qc_prev = add_months(first_coupon, -(months as i32));

    // E = length of quasi-coupon period containing settlement
    let e = quasi_period(qc_prev, first_coupon, basis, freq);
    if e == 0.0 { return f64::NAN; }

    // DSC = days from settlement to first_coupon
    let dsc = basis_days(settle, first_coupon, basis);
    let w   = dsc / e; // fraction of quasi-period from settle to first_coupon

    // N = number of REGULAR coupon periods from first_coupon to maturity
    let n = count_coupon_periods(first_coupon, mature, months as i32);

    let (odd_coupon, accrued) = if issue >= qc_prev {
        // ── SHORT first period ────────────────────────────────────────────────
        // Issue falls within the quasi-coupon period before first_coupon
        let dfc = basis_days(issue, first_coupon, basis); // issue → first_coupon
        let dci = basis_days(issue, settle, basis);       // issue → settlement
        (coupon * dfc / e, coupon * dci / e)
    } else {
        // ── LONG first period ─────────────────────────────────────────────────
        // Issue falls before qc_prev — sum contributions from each quasi-period
        let mut odd = 0.0f64;
        let mut acc = 0.0f64;
        let mut qc_end = first_coupon;

        loop {
            let qc_start = add_months(qc_end, -(months as i32));
            let ei = quasi_period(qc_start, qc_end, basis, freq);
            if ei == 0.0 { break; }

            // Actual start of contribution in this period
            let actual_start = if issue > qc_start { issue } else { qc_start };
            let dc = basis_days(actual_start, qc_end, basis);
            odd += coupon * dc / ei;

            // Accrued: days from actual_start to settlement (only if settle in this period)
            if settle >= actual_start && settle < qc_end {
                acc += coupon * basis_days(actual_start, settle, basis) / ei;
            } else if settle >= qc_end {
                // Settlement is past this period — full period accrues
                acc += coupon * dc / ei;
            }

            if issue >= qc_start { break; } // reached the period containing issue
            qc_end = qc_start;
        }
        (odd, acc)
    };

    // PV of all cash flows, discounted back to settlement
    // 1) Odd first coupon paid at first_coupon (w periods away)
    // 2) Regular coupons paid at periods 1, 2, ..., N after first_coupon
    // 3) Redemption paid at period N after first_coupon
    let vw = v.powf(w);
    let sum_regular: f64 = (1..=n).map(|k| v.powf(k as f64)).sum::<f64>() * coupon;
    let pv_redemption = redemption * v.powf(n as f64);
    let dirty = vw * (odd_coupon + sum_regular + pv_redemption);

    dirty - accrued
}

pub fn calc_oddfyield(
    settle: chrono::NaiveDate, mature: chrono::NaiveDate,
    issue: chrono::NaiveDate, first_coupon: chrono::NaiveDate,
    rate: f64, pr: f64, redemption: f64, freq: i32, basis: i32,
) -> f64 {
    // Bisect on calc_oddfprice
    let f = |y: f64| calc_oddfprice(settle, mature, issue, first_coupon,
                                     rate, y, redemption, freq, basis) - pr;
    bisect(f, 0.0, 1.0, 1e-9, 200)
}

// // pub fn calc_oddlprice(
//     settle: chrono::NaiveDate, mature: chrono::NaiveDate,
//     last_interest: chrono::NaiveDate, rate: f64, yld: f64,
//     redemption: f64, freq: i32, basis: i32,
// ) -> f64 {
//     let coupon = rate * 100.0 / freq as f64;
//     let y = yld / freq as f64;
//     let dcl = year_frac(last_interest, mature, basis);
//     let nl  = dcl * freq as f64;
//     let n_full = calc_coupnum(settle, last_interest, freq) as i32;
//     let dsc = (next_coupon(settle, last_interest, freq) - settle).num_days() as f64;
//     let e   = calc_coupdays(settle, last_interest, freq, basis);
//     if e == 0.0 { return f64::NAN; }
//     let t = 1.0 + dsc / e * y;
//     let mut price = 0.0f64;
//     for k in 0..n_full as usize {
//         price += coupon / t.powf(k as f64 + 1.0);
//     }
//     let last_cf = coupon * nl + redemption;
//     price += last_cf / t.powf(n_full as f64 + 1.0);
//     price
// }

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

    // E = normal quasi-coupon period length
    let qc_ref_end   = add_months(last_interest, months as i32);
    let e = quasi_period(last_interest, qc_ref_end, basis, freq);
    if e == 0.0 { return f64::NAN; }

    // DCL = days from last_interest to maturity → Nl = number of quasi-periods in odd last period
    let dcl = basis_days(last_interest, mature, basis);
    let nl  = dcl / e;
    let odd_coupon = coupon * nl;

    if settle >= last_interest {
        // ── Case 1: settlement is WITHIN the odd last period ─────────────────
        // Only cash flow: odd_coupon + redemption at maturity

        // DSM = days from settle to maturity, in quasi-coupon periods
        let dsm = basis_days(settle, mature, basis);
        let w   = dsm / e;

        let dirty = (odd_coupon + redemption) * v.powf(w);

        // Accrued = odd_coupon × (days from last_interest to settle) / DCL
        let dci     = basis_days(last_interest, settle, basis);
        let accrued = odd_coupon * dci / dcl;

        dirty - accrued

    } else {
        // ── Case 2: settlement is BEFORE the odd last period ──────────────────
        // There are regular coupons from settle to last_interest,
        // then the odd coupon + redemption at maturity.

        // Find the quasi-coupon period containing settlement (going backwards from last_interest)
        let mut qc_end = last_interest;
        loop {
            let prev = add_months(qc_end, -(months as i32));
            if prev <= settle { break; }
            qc_end = prev;
        }
        let qc_prev = add_months(qc_end, -(months as i32));

        // E for the period containing settlement
        let e_settle = quasi_period(qc_prev, qc_end, basis, freq);
        if e_settle == 0.0 { return f64::NAN; }

        // DSC = days from settle to qc_end (next quasi-coupon), w = DSC/E
        let dsc = basis_days(settle, qc_end, basis);
        let w   = dsc / e_settle;

        // N = number of full coupon periods from qc_end to last_interest
        let n_full = count_coupon_periods(qc_end, last_interest, months as i32);

        // Sum of regular coupons at times w, w+1, ..., w+n_full from settle
        let sum_reg: f64 = (0..=n_full)
            .map(|k| coupon * v.powf(w + k as f64))
            .sum();

        // Odd coupon + redemption at time w + n_full + nl from settle
        let pv_final = (odd_coupon + redemption) * v.powf(w + n_full as f64 + nl);

        let dirty = sum_reg + pv_final;

        // Accrued = coupon × (days from qc_prev to settle) / E
        let accrued = coupon * (e_settle - dsc) / e_settle;

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
            match (parse_date(&s), parse_date(&m), parse_date(&is), parse_date(&fc)) {
                (Some(settle), Some(mature), Some(issue), Some(first)) => write_f64(&mut out, i, calc_oddfprice(
                    settle, mature, issue, first,
                    input.flat_vector(4).as_slice::<f64>()[i],
                    input.flat_vector(5).as_slice::<f64>()[i],
                    input.flat_vector(6).as_slice::<f64>()[i],
                    freq_per_year(input.flat_vector(7).as_slice::<f64>()[i] as i32),
                    input.flat_vector(8).as_slice::<f64>()[i] as i32,
                )),
                _ => out.set_null(i),
            }
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
            match (parse_date(&s), parse_date(&m), parse_date(&is), parse_date(&fc)) {
                (Some(settle), Some(mature), Some(issue), Some(first)) => write_f64(&mut out, i, calc_oddfyield(
                    settle, mature, issue, first,
                    input.flat_vector(4).as_slice::<f64>()[i],
                    input.flat_vector(5).as_slice::<f64>()[i],
                    input.flat_vector(6).as_slice::<f64>()[i],
                    freq_per_year(input.flat_vector(7).as_slice::<f64>()[i] as i32),
                    input.flat_vector(8).as_slice::<f64>()[i] as i32,
                )),
                _ => out.set_null(i),
            }
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
            match (parse_date(&s), parse_date(&m), parse_date(&li)) {
                (Some(settle), Some(mature), Some(last)) => write_f64(&mut out, i, calc_oddlprice(
                    settle, mature, last,
                    input.flat_vector(3).as_slice::<f64>()[i],
                    input.flat_vector(4).as_slice::<f64>()[i],
                    input.flat_vector(5).as_slice::<f64>()[i],
                    freq_per_year(input.flat_vector(6).as_slice::<f64>()[i] as i32),
                    input.flat_vector(7).as_slice::<f64>()[i] as i32,
                )),
                _ => out.set_null(i),
            }
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
            match (parse_date(&s), parse_date(&m), parse_date(&li)) {
                (Some(settle), Some(mature), Some(last)) => write_f64(&mut out, i, calc_oddlyield(
                    settle, mature, last,
                    input.flat_vector(3).as_slice::<f64>()[i],
                    input.flat_vector(4).as_slice::<f64>()[i],
                    input.flat_vector(5).as_slice::<f64>()[i],
                    freq_per_year(input.flat_vector(6).as_slice::<f64>()[i] as i32),
                    input.flat_vector(7).as_slice::<f64>()[i] as i32,
                )),
                _ => out.set_null(i),
            }
        }
        Ok(())
    }
}
