// ── Bond/security pricing functions ──────────────────────────────────────────
// Excel: PRICE, PRICEDISC, PRICEMAT, YIELD, YIELDDISC, YIELDMAT,
//        DISC, INTRATE, RECEIVED, DURATION, MDURATION, ACCRINT, ACCRINTM

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::{read_varchar, parse_date, year_frac, freq_per_year, write_f64};
use crate::coupons::{next_coupon, calc_coupdays, calc_coupnum};

fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }

// ── Pure-Rust helpers ─────────────────────────────────────────────────────────
// ── Corrected bond_price: returns CLEAN price (dirty price - accrued interest)
pub fn calc_price(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32) -> f64
{
    let n = calc_coupnum(settle, maturity, freq);
    if n <= 0.0 { return f64::NAN; }
    bond_price(settle, maturity, rate, yld, redemption, freq, basis)
}

fn bond_price(
    settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32,
) -> f64 {
    let n = calc_coupnum(settle, maturity, freq) as i32;
    if n <= 0 { return f64::NAN; }

    let coupon   = rate * 100.0 / freq as f64;
    let yld_per  = yld / freq as f64;
    let e        = calc_coupdays(settle, maturity, freq, basis);
    if e == 0.0 { return f64::NAN; }

    let dsc = match basis {
        0 | 4 => {
            // 30/360: use day-count formula for DSC
            use crate::helpers::year_frac;
            let ncd = next_coupon(settle, maturity, freq);
            year_frac(settle, ncd, basis) * 360.0
        }
        _ => (next_coupon(settle, maturity, freq) - settle).num_days() as f64,
    };

    let w = dsc / e; // fraction of period to next coupon

    // ── Dirty price ──────────────────────────────────────────────────────────
    let dirty = if yld_per.abs() < 1e-12 {
        // Zero yield: simple sum
        coupon * n as f64 + redemption
    } else {
        let v  = 1.0 / (1.0 + yld_per);
        let vw = v.powf(w);
        let mut pv = 0.0f64;
        for k in 0..n {
            pv += coupon * v.powf(k as f64);
        }
        pv += redemption * v.powf((n - 1) as f64);
        pv * vw
    };

    // ── Accrued interest (days from previous coupon to settlement) ───────────
    // A = coupon * (E - DSC) / E
    let accrued = coupon * (e - dsc) / e;

    // ── Clean price = dirty - accrued ────────────────────────────────────────
    dirty - accrued
}

// pub fn calc_price(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
//     rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32) -> f64
// {
//     let n = calc_coupnum(settle, maturity, freq);
//     if n <= 0.0 { return f64::NAN; }
//     bond_price(settle, maturity, rate, yld, redemption, freq, basis)
// }

// fn bond_price(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
//     rate: f64, yld: f64, redemption: f64, freq: i32, basis: i32) -> f64
// {
//     let n = calc_coupnum(settle, maturity, freq) as i32;
//     if n <= 0 { return f64::NAN; }
//     let coupon  = rate * 100.0 / freq as f64;
//     let yld_per = yld / freq as f64;
//     let dsc = (next_coupon(settle, maturity, freq) - settle).num_days() as f64;
//     let e   = calc_coupdays(settle, maturity, freq, basis);
//     if e == 0.0 { return f64::NAN; }
//     let w = dsc / e;
//     if yld_per.abs() < 1e-12 {
//         return redemption + coupon * n as f64;
//     }
//     let v  = 1.0 / (1.0 + yld_per);
//     let vw = v.powf(w);
//     let mut pv_coupons = 0.0f64;
//     for k in 0..n {
//         pv_coupons += coupon * v.powf(k as f64) * vw;
//     }
//     let pv_redemption = redemption * v.powf((n - 1) as f64) * vw;
//     pv_coupons + pv_redemption
// }

pub fn calc_pricedisc(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    discount: f64, redemption: f64, basis: i32) -> f64
{
    let yf = year_frac(settle, maturity, basis);
    redemption * (1.0 - discount * yf)
}

pub fn calc_pricemat(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    issue: chrono::NaiveDate, rate: f64, yld: f64, basis: i32) -> f64
{
    let dim = year_frac(issue, maturity, basis);
    let dis = year_frac(issue, settle, basis);
    let dsm = year_frac(settle, maturity, basis);
    if (1.0 + yld * dsm).abs() < 1e-12 { return f64::NAN; }
    (100.0 * (1.0 + rate * dim)) / (1.0 + yld * dsm) - 100.0 * rate * dis
}

pub fn calc_yield(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    rate: f64, pr: f64, redemption: f64, freq: i32, basis: i32) -> f64
{
    let f = |y: f64| bond_price(settle, maturity, rate, y, redemption, freq, basis) - pr;
    newton_raphson(f, 0.05, 1e-10, 200)
}

pub fn calc_yielddisc(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    pr: f64, redemption: f64, basis: i32) -> f64
{
    let yf = year_frac(settle, maturity, basis);
    if yf == 0.0 || pr == 0.0 { return f64::NAN; }
    (redemption / pr - 1.0) / yf
}

pub fn calc_yieldmat(
    settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    issue: chrono::NaiveDate, rate: f64, pr: f64, basis: i32,
) -> f64 {
    let dim = year_frac(issue, maturity, basis);   // issue → maturity
    let dis = year_frac(issue, settle, basis);     // issue → settlement
    let dsm = year_frac(settle, maturity, basis);  // settlement → maturity
    if dsm.abs() < 1e-12 { return f64::NAN; }

    // Excel formula: ((100*(1+rate*dim)) / (pr + 100*rate*dis) - 1) / dsm
    let numer = 100.0 * (1.0 + rate * dim);
    let denom = pr + 100.0 * rate * dis;
    if denom.abs() < 1e-12 { return f64::NAN; }
    (numer / denom - 1.0) / dsm
}

pub fn calc_disc(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    pr: f64, redemption: f64, basis: i32) -> f64
{
    let yf = year_frac(settle, maturity, basis);
    if redemption == 0.0 || yf == 0.0 { return f64::NAN; }
    (redemption - pr) / redemption / yf
}

pub fn calc_intrate(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    investment: f64, redemption: f64, basis: i32) -> f64
{
    let yf = year_frac(settle, maturity, basis);
    if investment == 0.0 || yf == 0.0 { return f64::NAN; }
    (redemption / investment - 1.0) / yf
}

pub fn calc_received(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    investment: f64, discount: f64, basis: i32) -> f64
{
    let yf = year_frac(settle, maturity, basis);
    let denom = 1.0 - discount * yf;
    if denom.abs() < 1e-12 { return f64::NAN; }
    investment / denom
}

pub fn calc_duration(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    coupon: f64, yld: f64, freq: i32, basis: i32) -> f64
{
    let n = calc_coupnum(settle, maturity, freq) as i32;
    if n <= 0 { return f64::NAN; }
    let c = coupon * 100.0 / freq as f64;
    let y = yld / freq as f64;
    let dsc = (next_coupon(settle, maturity, freq) - settle).num_days() as f64;
    let e   = calc_coupdays(settle, maturity, freq, basis);
    if e == 0.0 { return f64::NAN; }
    let w = dsc / e;
    let v = 1.0 / (1.0 + y);
    let mut sum_pv = 0.0f64;
    let mut sum_t_pv = 0.0f64;
    for k in 0..n {
        let t  = w + k as f64;
        let pv = c * v.powf(t);
        sum_pv   += pv;
        sum_t_pv += t * pv;
    }
    let t_red  = w + (n - 1) as f64;
    let pv_red = 100.0 * v.powf(t_red);
    sum_pv   += pv_red;
    sum_t_pv += t_red * pv_red;
    if sum_pv.abs() < 1e-12 { return f64::NAN; }
    (sum_t_pv / sum_pv) / freq as f64
}

pub fn calc_mduration(settle: chrono::NaiveDate, maturity: chrono::NaiveDate,
    coupon: f64, yld: f64, freq: i32, basis: i32) -> f64
{
    let dur = calc_duration(settle, maturity, coupon, yld, freq, basis);
    let denom = 1.0 + yld / freq as f64;
    if denom.abs() < 1e-12 { return f64::NAN; }
    dur / denom
}

pub fn calc_accrint(issue: chrono::NaiveDate, _first_interest: chrono::NaiveDate,
    settle: chrono::NaiveDate, rate: f64, par: f64, freq: i32, basis: i32) -> f64
{
    let nc = calc_coupnum(settle, settle, freq).max(1.0);
    let _ = (nc, freq);
    par * rate * year_frac(issue, settle, basis)
}

pub fn calc_accrintm(issue: chrono::NaiveDate, settle: chrono::NaiveDate,
    rate: f64, par: f64, basis: i32) -> f64
{
    par * rate * year_frac(issue, settle, basis)
}

/// Newton-Raphson root finder
fn newton_raphson<F: Fn(f64) -> f64>(f: F, guess: f64, tol: f64, max_iter: usize) -> f64 {
    let mut x = guess;
    for _ in 0..max_iter {
        let fx  = f(x);
        let dfx = (f(x + 1e-7) - fx) / 1e-7;
        if dfx.abs() < 1e-20 { return f64::NAN; }
        let x_new = x - fx / dfx;
        if (x_new - x).abs() < tol { return x_new; }
        x = x_new;
    }
    f64::NAN
}

// ── Macro to reduce boilerplate for date-bearing functions ───────────────────

// Helper: parse two date columns, returning null on bad input
macro_rules! bond_fn {
    ($struct:ident, $sig:expr, $body:expr) => {
        pub struct $struct;
        impl VScalar for $struct {
            type State = ();
            fn signatures() -> Vec<ScalarFunctionSignature> { vec![$sig] }
            unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
                let len = input.len(); let mut out = output.flat_vector();
                for i in 0..len {
                    #[allow(clippy::redundant_closure_call)]
                    { $body(input, &mut out, i); }
                }
                Ok(())
            }
        }
    };
}

// ── PRICE ────────────────────────────────────────────────────────────────────
bond_fn!(PriceFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_price(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                freq_per_year(input.flat_vector(5).as_slice::<f64>()[i] as i32),
                input.flat_vector(6).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── PRICEDISC ────────────────────────────────────────────────────────────────
bond_fn!(PricediscFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_pricedisc(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── PRICEMAT ─────────────────────────────────────────────────────────────────
bond_fn!(PricematFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s  = unsafe { read_varchar(input, 0, i) };
        let m  = unsafe { read_varchar(input, 1, i) };
        let is = unsafe { read_varchar(input, 2, i) };
        match (parse_date(&s), parse_date(&m), parse_date(&is)) {
            (Some(settle), Some(mature), Some(issue)) => write_f64(out, i, calc_pricemat(
                settle, mature, issue,
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── YIELD ────────────────────────────────────────────────────────────────────
bond_fn!(YieldFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_yield(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                freq_per_year(input.flat_vector(5).as_slice::<f64>()[i] as i32),
                input.flat_vector(6).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── YIELDDISC ────────────────────────────────────────────────────────────────
bond_fn!(YielddiscFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_yielddisc(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── YIELDMAT ─────────────────────────────────────────────────────────────────
bond_fn!(YieldmatFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s  = unsafe { read_varchar(input, 0, i) };
        let m  = unsafe { read_varchar(input, 1, i) };
        let is = unsafe { read_varchar(input, 2, i) };
        match (parse_date(&s), parse_date(&m), parse_date(&is)) {
            (Some(settle), Some(mature), Some(issue)) => write_f64(out, i, calc_yieldmat(
                settle, mature, issue,
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── DISC ─────────────────────────────────────────────────────────────────────
bond_fn!(DiscFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_disc(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── INTRATE ──────────────────────────────────────────────────────────────────
bond_fn!(IntrateFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_intrate(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── RECEIVED ─────────────────────────────────────────────────────────────────
bond_fn!(ReceivedFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_received(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── DURATION ─────────────────────────────────────────────────────────────────
bond_fn!(DurationFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_duration(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                freq_per_year(input.flat_vector(4).as_slice::<f64>()[i] as i32),
                input.flat_vector(5).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── MDURATION ────────────────────────────────────────────────────────────────
bond_fn!(MdurationFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let s = unsafe { read_varchar(input, 0, i) };
        let m = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&s), parse_date(&m)) {
            (Some(settle), Some(mature)) => write_f64(out, i, calc_mduration(
                settle, mature,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                freq_per_year(input.flat_vector(4).as_slice::<f64>()[i] as i32),
                input.flat_vector(5).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── ACCRINT ──────────────────────────────────────────────────────────────────
bond_fn!(AccrintFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), varchar(), dbl(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let is = unsafe { read_varchar(input, 0, i) };
        let fi = unsafe { read_varchar(input, 1, i) };
        let se = unsafe { read_varchar(input, 2, i) };
        match (parse_date(&is), parse_date(&fi), parse_date(&se)) {
            (Some(issue), Some(first), Some(settle)) => write_f64(out, i, calc_accrint(
                issue, first, settle,
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                freq_per_year(input.flat_vector(5).as_slice::<f64>()[i] as i32),
                input.flat_vector(6).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);

// ── ACCRINTM ─────────────────────────────────────────────────────────────────
bond_fn!(AccrintmFunction,
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl(), dbl()], dbl()),
    |input: &mut DataChunkHandle, out: &mut duckdb::core::FlatVector, i: usize| {
        let is = unsafe { read_varchar(input, 0, i) };
        let se = unsafe { read_varchar(input, 1, i) };
        match (parse_date(&is), parse_date(&se)) {
            (Some(issue), Some(settle)) => write_f64(out, i, calc_accrintm(
                issue, settle,
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i] as i32,
            )),
            _ => out.set_null(i),
        }
    }
);
