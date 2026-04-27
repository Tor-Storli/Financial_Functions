// ── Coupon date functions ─────────────────────────────────────────────────────
// Excel: COUPDAYBS, COUPDAYS, COUPDAYSNC, COUPNCD, COUPNUM, COUPPCD

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use chrono::NaiveDate;
use std::error::Error;
use crate::helpers::{read_varchar, parse_date, add_months, year_frac, freq_per_year, write_f64};

fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn varchar() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Varchar) }

fn coupon_sig() -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(vec![varchar(), varchar(), dbl(), dbl()], dbl())
}

// ── Coupon date arithmetic helpers ────────────────────────────────────────────

pub fn next_coupon(settlement: NaiveDate, maturity: NaiveDate, freq: i32) -> NaiveDate {
    let months_per_period = 12 / freq;
    let mut d = maturity;
    while d > settlement {
        let prev = add_months(d, -months_per_period);
        if prev <= settlement { return d; }
        d = prev;
    }
    add_months(d, months_per_period)
}

pub fn prev_coupon(settlement: NaiveDate, maturity: NaiveDate, freq: i32) -> NaiveDate {
    let ncd = next_coupon(settlement, maturity, freq);
    add_months(ncd, -(12 / freq))
}

pub fn coupon_period_days(settlement: NaiveDate, maturity: NaiveDate, freq: i32, basis: i32) -> f64 {
    let pcd = prev_coupon(settlement, maturity, freq);
    let ncd = next_coupon(settlement, maturity, freq);
    match basis {
        0 | 4 => 360.0 / freq as f64,
        _ => (ncd - pcd).num_days() as f64,
    }
}

pub fn calc_coupdaybs(settlement: NaiveDate, maturity: NaiveDate, freq: i32, basis: i32) -> f64 {
    let pcd = prev_coupon(settlement, maturity, freq);
    match basis {
        0 | 4 => year_frac(pcd, settlement, basis) * 360.0,
        _ => (settlement - pcd).num_days() as f64,
    }
}

pub fn calc_coupdays(settlement: NaiveDate, maturity: NaiveDate, freq: i32, basis: i32) -> f64 {
    coupon_period_days(settlement, maturity, freq, basis)
}

pub fn calc_coupdaysnc(settlement: NaiveDate, maturity: NaiveDate, freq: i32, basis: i32) -> f64 {
    let ncd = next_coupon(settlement, maturity, freq);
    match basis {
        0 | 4 => year_frac(settlement, ncd, basis) * 360.0,
        _ => (ncd - settlement).num_days() as f64,
    }
}

pub fn calc_coupncd(settlement: NaiveDate, maturity: NaiveDate, freq: i32) -> f64 {
    date_to_excel_serial(next_coupon(settlement, maturity, freq))
}

pub fn calc_couppcd(settlement: NaiveDate, maturity: NaiveDate, freq: i32) -> f64 {
    date_to_excel_serial(prev_coupon(settlement, maturity, freq))
}

pub fn calc_coupnum(settlement: NaiveDate, maturity: NaiveDate, freq: i32) -> f64 {
    let months_per_period = 12 / freq;
    let mut count = 0i32;
    let mut d = maturity;
    while d > settlement {
        count += 1;
        d = add_months(d, -months_per_period);
    }
    count as f64
}

fn date_to_excel_serial(d: NaiveDate) -> f64 {
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
    (d - epoch).num_days() as f64
}

// ── VScalar structs ───────────────────────────────────────────────────────────
//
// Guard applied in every function: settlement must be < maturity and both dates
// must parse correctly. Invalid input → NULL (never a crash or infinite loop).

macro_rules! coupon_fn {
    ($struct:ident, $calc:expr) => {
        pub struct $struct;
        impl VScalar for $struct {
            type State = ();
            fn signatures() -> Vec<ScalarFunctionSignature> { vec![coupon_sig()] }
            unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
                let len = input.len();
                let mut out = output.flat_vector();
                for i in 0..len {
                    let ss = unsafe { read_varchar(input, 0, i) };
                    let ms = unsafe { read_varchar(input, 1, i) };
                    let freq  = freq_per_year(input.flat_vector(2).as_slice::<f64>()[i] as i32);
                    let basis = input.flat_vector(3).as_slice::<f64>()[i] as i32;
                    match (parse_date(&ss), parse_date(&ms)) {
                        (Some(settle), Some(mature)) if settle < mature => {
                            #[allow(clippy::redundant_closure_call)]
                            write_f64(&mut out, i, $calc(settle, mature, freq, basis));
                        }
                        _ => out.set_null(i),
                    }
                }
                Ok(())
            }
        }
    };
    ($struct:ident, nobasis, $calc:expr) => {
        pub struct $struct;
        impl VScalar for $struct {
            type State = ();
            fn signatures() -> Vec<ScalarFunctionSignature> { vec![coupon_sig()] }
            unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
                let len = input.len();
                let mut out = output.flat_vector();
                for i in 0..len {
                    let ss = unsafe { read_varchar(input, 0, i) };
                    let ms = unsafe { read_varchar(input, 1, i) };
                    let freq = freq_per_year(input.flat_vector(2).as_slice::<f64>()[i] as i32);
                    match (parse_date(&ss), parse_date(&ms)) {
                        (Some(settle), Some(mature)) if settle < mature => {
                            #[allow(clippy::redundant_closure_call)]
                            write_f64(&mut out, i, $calc(settle, mature, freq));
                        }
                        _ => out.set_null(i),
                    }
                }
                Ok(())
            }
        }
    };
}

coupon_fn!(CoupdaybsFunction,   |s, m, f, b| calc_coupdaybs(s, m, f, b));
coupon_fn!(CoupdaysFunction,    |s, m, f, b| calc_coupdays(s, m, f, b));
coupon_fn!(CoupdayssncFunction, |s, m, f, b| calc_coupdaysnc(s, m, f, b));
coupon_fn!(CoupncdFunction,     nobasis, |s, m, f| calc_coupncd(s, m, f));
coupon_fn!(CouppcdFunction,     nobasis, |s, m, f| calc_couppcd(s, m, f));
coupon_fn!(CoupnumFunction,     nobasis, |s, m, f| calc_coupnum(s, m, f));
