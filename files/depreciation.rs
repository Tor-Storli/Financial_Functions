// ── Depreciation functions ────────────────────────────────────────────────────
// Excel: SLN, SYD, DB, DDB, VDB, AMORDEGRC, AMORLINC

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::write_f64;
use crate::errors::{FinError, validate_cost, validate_life, validate_period, validate_salvage};

/// Wrap a FinError into Box<dyn Error> for use with `?`.
#[inline]
fn fe(e: FinError) -> Box<dyn Error> { Box::new(e) }

fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

// ── Pure-Rust helpers ─────────────────────────────────────────────────────────

pub fn calc_sln(cost: f64, salvage: f64, life: f64) -> f64 {
    if life == 0.0 { return f64::NAN; }
    (cost - salvage) / life
}

pub fn calc_syd(cost: f64, salvage: f64, life: f64, per: f64) -> f64 {
    if life == 0.0 { return f64::NAN; }
    let sum_digits = life * (life + 1.0) / 2.0;
    (cost - salvage) * (life - per + 1.0) / sum_digits
}

pub fn calc_db(cost: f64, salvage: f64, life: f64, per: f64, month: f64) -> f64 {
    if cost == 0.0 { return 0.0; }
    if life == 0.0 { return f64::NAN; }
    let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
    let mut book = cost;
    let mut dep = 0.0;
    for p in 1..=(per as usize) {
        dep = if p == 1 {
            cost * rate * month / 12.0
        } else if p == (life as usize + 1) {
            (book - salvage.max(0.0)) * rate * (12.0 - month) / 12.0
        } else {
            book * rate
        };
        book -= dep;
    }
    dep
}

pub fn calc_ddb(cost: f64, salvage: f64, life: f64, per: f64, factor: f64) -> f64 {
    if life == 0.0 { return f64::NAN; }
    let rate = factor / life;
    let mut book = cost;
    let mut dep = 0.0;
    for _p in 1..=(per as usize) {
        dep = (book * rate).min(book - salvage);
        if dep < 0.0 { dep = 0.0; }
        book -= dep;
    }
    dep
}

pub fn calc_vdb(cost: f64, salvage: f64, life: f64, start_per: f64, end_per: f64, factor: f64, no_switch: bool) -> f64 {
    if life == 0.0 { return f64::NAN; }
    let rate = factor / life;
    let mut total = 0.0;
    let i_start = start_per.floor() as usize;
    let i_end   = end_per.ceil() as usize;
    let n = life.ceil() as usize;
    let mut book = cost;
    let mut schedule = vec![0.0f64; n + 1];
    for p in 1..=n {
        let ddb = (book * rate).max(0.0);
        let sl  = if life - (p as f64 - 1.0) > 0.0 {
            (book - salvage) / (life - (p as f64 - 1.0))
        } else { 0.0 };
        let dep = if !no_switch && sl > ddb { sl } else { ddb };
        let dep = dep.min(book - salvage).max(0.0);
        schedule[p] = dep;
        book -= dep;
    }
    for p in (i_start + 1)..=i_end {
        let frac = if p as f64 <= start_per {
            0.0
        } else if (p as f64 - 1.0) < start_per {
            (p as f64 - start_per).min(1.0)
        } else if p as f64 > end_per {
            end_per - (p as f64 - 1.0)
        } else {
            1.0
        };
        if p <= n { total += schedule[p] * frac; }
    }
    total
}

pub fn calc_amorlinc(cost: f64, _date_purch: f64, _first_period: f64, salvage: f64, _period: f64, rate: f64, _basis: f64) -> f64 {
    let annual_dep = cost * rate;
    let total_dep  = cost - salvage;
    annual_dep.min(total_dep)
}

pub fn calc_amordegrc(cost: f64, date_purch: f64, first_period: f64, salvage: f64, period: f64, rate: f64, basis: f64) -> f64 {
    if rate <= 0.0 || cost <= salvage { return f64::NAN; }
    let life  = 1.0 / rate;
    let coeff = if life >= 3.0 && life < 4.0      { 1.5 }
                else if life >= 5.0 && life < 6.0 { 2.0 }
                else if life >= 6.0                { 2.5 }
                else                               { return f64::NAN; };
    let depr_rate = rate * coeff;
    let year_days = match basis as i32 {
        3     => 365.0,
        0 | 4 => 360.0,
        2     => 360.0,
        _     => 365.0,
    };
    let first_frac = ((first_period - date_purch) / year_days).clamp(0.0, 1.0);
    let dep0 = (cost * depr_rate * first_frac).floor().min(cost - salvage).max(0.0);
    if period == 0.0 { return dep0; }
    let mut book = cost - dep0;
    let total_periods = life.ceil() as usize;
    for p in 1..=(period as usize) {
        if book <= salvage { return 0.0; }
        let remaining = total_periods.saturating_sub(p);
        let dep = if remaining == 0 {
            (book - salvage).max(0.0)
        } else if remaining == 1 {
            (book * 0.5).floor().min(book - salvage).max(0.0)
        } else {
            (book * depr_rate).floor().min(book - salvage).max(0.0)
        };
        if p == period as usize { return dep; }
        book -= dep;
    }
    0.0
}

// ── VScalar structs ───────────────────────────────────────────────────────────

// SLN(cost, salvage, life)
pub struct SlnFunction;
impl VScalar for SlnFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(1).as_slice::<f64>()[i];
            let life    = input.flat_vector(2).as_slice::<f64>()[i];
            validate_cost("sln", "cost", cost).map_err(fe)?;
            validate_life("sln", life).map_err(fe)?;
            validate_salvage("sln", salvage, cost).map_err(fe)?;
            write_f64(&mut out, i, calc_sln(cost, salvage, life));
        }
        Ok(())
    }
}

// SYD(cost, salvage, life, per)
pub struct SydFunction;
impl VScalar for SydFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(1).as_slice::<f64>()[i];
            let life    = input.flat_vector(2).as_slice::<f64>()[i];
            let per     = input.flat_vector(3).as_slice::<f64>()[i];
            validate_cost("syd", "cost", cost).map_err(fe)?;
            validate_life("syd", life).map_err(fe)?;
            validate_salvage("syd", salvage, cost).map_err(fe)?;
            validate_period("syd", per, life).map_err(fe)?;
            write_f64(&mut out, i, calc_syd(cost, salvage, life, per));
        }
        Ok(())
    }
}

// DB(cost, salvage, life, per, month)
// Note: per can go up to life+1 in DB (covers the partial first year remainder).
pub struct DbFunction;
impl VScalar for DbFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(1).as_slice::<f64>()[i];
            let life    = input.flat_vector(2).as_slice::<f64>()[i];
            let per     = input.flat_vector(3).as_slice::<f64>()[i];
            let month   = input.flat_vector(4).as_slice::<f64>()[i];
            validate_cost("db", "cost", cost).map_err(fe)?;
            validate_life("db", life).map_err(fe)?;
            validate_salvage("db", salvage, cost).map_err(fe)?;
            // per range is 1 to life+1 for DB
            if per < 1.0 || per > life + 1.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "db", arg: "per", value: per,
                    min: Some(1.0), max: Some(life + 1.0),
                    exclusive_min: false, exclusive_max: false,
                    hint: Some("DB allows period up to life+1 to cover a partial first/last year."),
                }));
            }
            if month < 1.0 || month > 12.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "db", arg: "month", value: month,
                    min: Some(1.0), max: Some(12.0),
                    exclusive_min: false, exclusive_max: false,
                    hint: Some("'month' is the number of months in the first year (1–12)."),
                }));
            }
            write_f64(&mut out, i, calc_db(cost, salvage, life, per, month));
        }
        Ok(())
    }
}

// DDB(cost, salvage, life, per, factor)
pub struct DdbFunction;
impl VScalar for DdbFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(1).as_slice::<f64>()[i];
            let life    = input.flat_vector(2).as_slice::<f64>()[i];
            let per     = input.flat_vector(3).as_slice::<f64>()[i];
            let factor  = input.flat_vector(4).as_slice::<f64>()[i];
            validate_cost("ddb", "cost", cost).map_err(fe)?;
            validate_life("ddb", life).map_err(fe)?;
            validate_salvage("ddb", salvage, cost).map_err(fe)?;
            validate_period("ddb", per, life).map_err(fe)?;
            if factor <= 0.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "ddb", arg: "factor", value: factor,
                    min: Some(0.0), max: None,
                    exclusive_min: true, exclusive_max: false,
                    hint: Some("'factor' is the rate at which the balance declines (default is 2 for double-declining)."),
                }));
            }
            write_f64(&mut out, i, calc_ddb(cost, salvage, life, per, factor));
        }
        Ok(())
    }
}

// VDB(cost, salvage, life, start_per, end_per, factor, no_switch)
pub struct VdbFunction;
impl VScalar for VdbFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost      = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage   = input.flat_vector(1).as_slice::<f64>()[i];
            let life      = input.flat_vector(2).as_slice::<f64>()[i];
            let start_per = input.flat_vector(3).as_slice::<f64>()[i];
            let end_per   = input.flat_vector(4).as_slice::<f64>()[i];
            let factor    = input.flat_vector(5).as_slice::<f64>()[i];
            let no_switch = input.flat_vector(6).as_slice::<f64>()[i] != 0.0;
            validate_cost("vdb", "cost", cost).map_err(fe)?;
            validate_life("vdb", life).map_err(fe)?;
            validate_salvage("vdb", salvage, cost).map_err(fe)?;
            if factor <= 0.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "vdb", arg: "factor", value: factor,
                    min: Some(0.0), max: None,
                    exclusive_min: true, exclusive_max: false,
                    hint: Some("'factor' is the rate of depreciation (default 2 for double-declining balance)."),
                }));
            }
            if start_per < 0.0 || start_per >= end_per || end_per > life {
                return Err(fe(FinError::General {
                    func: "vdb",
                    message: format!(
                        "Period range is invalid: start_period={start_per}, end_period={end_per}, life={life}. \
                         Required: 0 ≤ start_period < end_period ≤ life."
                    ),
                }));
            }
            write_f64(&mut out, i, calc_vdb(cost, salvage, life, start_per, end_per, factor, no_switch));
        }
        Ok(())
    }
}

// AMORLINC(cost, date_purchased, first_period, salvage, period, rate, basis)
pub struct AmorlincFunction;
impl VScalar for AmorlincFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(3).as_slice::<f64>()[i];
            let rate    = input.flat_vector(5).as_slice::<f64>()[i];
            validate_cost("amorlinc", "cost", cost).map_err(fe)?;
            validate_salvage("amorlinc", salvage, cost).map_err(fe)?;
            if rate <= 0.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "amorlinc", arg: "rate", value: rate,
                    min: Some(0.0), max: None,
                    exclusive_min: true, exclusive_max: false,
                    hint: Some("'rate' is the annual depreciation rate and must be positive (e.g. 0.15 for 15%)."),
                }));
            }
            write_f64(&mut out, i, calc_amorlinc(
                cost,
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                salvage,
                input.flat_vector(4).as_slice::<f64>()[i],
                rate,
                input.flat_vector(6).as_slice::<f64>()[i],
            ));
        }
        Ok(())
    }
}

// AMORDEGRC(cost, date_purchased, first_period, salvage, period, rate, basis)
pub struct AmordegrcFunction;
impl VScalar for AmordegrcFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let cost    = input.flat_vector(0).as_slice::<f64>()[i];
            let salvage = input.flat_vector(3).as_slice::<f64>()[i];
            let rate    = input.flat_vector(5).as_slice::<f64>()[i];
            validate_cost("amordegrc", "cost", cost).map_err(fe)?;
            validate_salvage("amordegrc", salvage, cost).map_err(fe)?;
            if rate <= 0.0 {
                return Err(fe(FinError::OutOfRange {
                    func: "amordegrc", arg: "rate", value: rate,
                    min: Some(0.0), max: None,
                    exclusive_min: true, exclusive_max: false,
                    hint: Some("'rate' is the annual depreciation rate and must be positive (e.g. 0.25 for 25%)."),
                }));
            }
            write_f64(&mut out, i, calc_amordegrc(
                cost,
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                salvage,
                input.flat_vector(4).as_slice::<f64>()[i],
                rate,
                input.flat_vector(6).as_slice::<f64>()[i],
            ));
        }
        Ok(())
    }
}
