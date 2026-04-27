// ── Depreciation functions ────────────────────────────────────────────────────
// Excel: SLN, SYD, DB, DDB, VDB, AMORDEGRC, AMORLINC

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::write_f64;

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
   // let rate = (1.0 - (salvage / cost).powf(1.0 / life) * 1000.0).round() / 1000.0;
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
    let total_dep = cost - salvage;
    annual_dep.min(total_dep)
}

pub fn calc_amordegrc(cost: f64, date_purch: f64, first_period: f64, salvage: f64, period: f64, rate: f64, basis: f64) -> f64 {
    if rate <= 0.0 || cost <= salvage { return f64::NAN; }

    let life = 1.0 / rate;

    // Excel spec: these life ranges return #NUM! error
    // Valid ranges: 3-4 years, 5-6 years, 6+ years
    let coeff = if life >= 3.0 && life < 4.0      { 1.5 }
                else if life >= 5.0 && life < 6.0 { 2.0 }
                else if life >= 6.0                { 2.5 }
                else                               { return f64::NAN; };

    let depr_rate = rate * coeff;

    // Year length based on basis
    let year_days = match basis as i32 {
        3     => 365.0,
        0 | 4 => 360.0,
        2     => 360.0,
        _     => 365.0, // basis 1 = actual
    };

    // Period 0 — prorated first period using actual purchase/first-period dates
    let first_frac = ((first_period - date_purch) / year_days).clamp(0.0, 1.0);
    let dep0 = (cost * depr_rate * first_frac).floor().min(cost - salvage).max(0.0);

    if period == 0.0 {
        return dep0;
    }

    let mut book = cost - dep0;
    // Total full periods over asset life (used to detect penultimate/last period)
    let total_periods = life.ceil() as usize;

    for p in 1..=(period as usize) {
        if book <= salvage { return 0.0; }

        let remaining = total_periods.saturating_sub(p);

        let dep = if remaining == 0 {
            // Last period: 100% of remaining depreciable value
            (book - salvage).max(0.0)
        } else if remaining == 1 {
            // Penultimate period: rate grows to 50%
            (book * 0.5).floor().min(book - salvage).max(0.0)
        } else {
            // Normal period
            (book * depr_rate).floor().min(book - salvage).max(0.0)
        };

        if p == period as usize {
            return dep;
        }
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
            write_f64(&mut out, i, calc_sln(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
            ));
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
            write_f64(&mut out, i, calc_syd(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
            ));
        }
        Ok(())
    }
}

// DB(cost, salvage, life, per, month)
pub struct DbFunction;
impl VScalar for DbFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            write_f64(&mut out, i, calc_db(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
            ));
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
            write_f64(&mut out, i, calc_ddb(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
            ));
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
            let no_switch = input.flat_vector(6).as_slice::<f64>()[i] != 0.0;
            write_f64(&mut out, i, calc_vdb(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                no_switch,
            ));
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
            write_f64(&mut out, i, calc_amorlinc(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
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
            write_f64(&mut out, i, calc_amordegrc(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<f64>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
                input.flat_vector(6).as_slice::<f64>()[i],
            ));
        }
        Ok(())
    }
}
