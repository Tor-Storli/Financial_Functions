// ── Annuity functions ─────────────────────────────────────────────────────────
// Excel: FV, PV, PMT, IPMT, PPMT, CUMIPMT, CUMPRINC, NPER, RATE, ISPMT

use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use std::error::Error;
use crate::helpers::write_f64;

// ── Pure-Rust calculation helpers ────────────────────────────────────────────

pub fn calc_pmt(rate: f64, nper: f64, pv: f64, fv: f64, pmt_at_beg: bool) -> f64 {
    if nper == 0.0 { return f64::NAN; }
    if rate == 0.0 { return -(pv + fv) / nper; }
    let r1n = (1.0 + rate).powf(nper);
    let due = if pmt_at_beg { 1.0 + rate } else { 1.0 };
    let denom = (r1n - 1.0) / rate * due;
    if denom.abs() < 1e-12 { return f64::NAN; }
    -(pv * r1n + fv) / denom
}

pub fn calc_ipmt(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, pmt_at_beg: bool) -> f64 {
    if rate == 0.0 { return 0.0; }
    let pmt = calc_pmt(rate, nper, pv, fv, pmt_at_beg);
    let r1 = 1.0 + rate;
    let balance = pv * r1.powf(per - 1.0)
        + pmt * (r1.powf(per - 1.0) - 1.0) / rate;
    -(balance * rate)
}

pub fn calc_ppmt(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, pmt_at_beg: bool) -> f64 {
    calc_pmt(rate, nper, pv, fv, pmt_at_beg) - calc_ipmt(rate, per, nper, pv, fv, pmt_at_beg)
}

pub fn calc_cumipmt(rate: f64, nper: f64, pv: f64, start: f64, end: f64, pmt_at_beg: bool) -> f64 {
    (start as usize..=end as usize)
        .map(|p| calc_ipmt(rate, p as f64, nper, pv, 0.0, pmt_at_beg))
        .sum()
}

pub fn calc_cumprinc(rate: f64, nper: f64, pv: f64, start: f64, end: f64, pmt_at_beg: bool) -> f64 {
    (start as usize..=end as usize)
        .map(|p| calc_ppmt(rate, p as f64, nper, pv, 0.0, pmt_at_beg))
        .sum()
}

pub fn calc_nper(rate: f64, pmt: f64, pv: f64, fv: f64, pmt_at_beg: bool) -> f64 {
    if rate == 0.0 {
        if pmt.abs() < 1e-12 { return f64::NAN; }
        return -(pv + fv) / pmt;
    }
    let due = if pmt_at_beg { 1.0 + rate } else { 1.0 };
    let adjusted_pmt = pmt * due;
    let num = adjusted_pmt - fv * rate;
    let den = adjusted_pmt + pv * rate;
    if den.abs() < 1e-12 || num / den <= 0.0 { return f64::NAN; }
    (num / den).ln() / (1.0 + rate).ln()
}

pub fn calc_rate(nper: f64, pmt: f64, pv: f64, fv: f64, pmt_at_beg: bool, guess: f64) -> f64 {
    let max_iter = 300;
    let tol = 1e-10;
    let mut r = guess;
    for _ in 0..max_iter {
        let r1 = 1.0 + r;
        let r1n = r1.powf(nper);
        let due = if pmt_at_beg { r1 } else { 1.0 };
        let f = if r.abs() < 1e-12 {
            pv + pmt * nper * due + fv
        } else {
            pv * r1n + pmt * due * (r1n - 1.0) / r + fv
        };
        let df = if r.abs() < 1e-12 {
            pv * nper + pmt * (nper * due / r1 + (r1n - 1.0) / r)
        } else {
            pv * nper * r1.powf(nper - 1.0)
                + pmt * due * (nper * r1.powf(nper - 1.0) * r - (r1n - 1.0)) / (r * r)
        };
        if df.abs() < 1e-20 { return f64::NAN; }
        let r_new = r - f / df;
        if (r_new - r).abs() < tol { return r_new; }
        r = r_new;
    }
    f64::NAN
}

pub fn calc_ispmt(rate: f64, per: f64, nper: f64, pv: f64) -> f64 {
    if nper == 0.0 { return f64::NAN; }
    let principal_per_period = pv / nper;
    let remaining = pv - principal_per_period * (per - 1.0);
    -(remaining * rate)
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn dbl() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Double) }
fn bln() -> LogicalTypeHandle { LogicalTypeHandle::from(LogicalTypeId::Boolean) }
fn sig(args: Vec<LogicalTypeHandle>) -> ScalarFunctionSignature {
    ScalarFunctionSignature::exact(args, dbl())
}

// ── FV ───────────────────────────────────────────────────────────────────────
pub struct FvFunction;
impl VScalar for FvFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            write_f64(&mut out, i, financial::fv(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                Some(input.flat_vector(2).as_slice::<f64>()[i]),
                Some(input.flat_vector(3).as_slice::<f64>()[i]),
                Some(input.flat_vector(4).as_slice::<bool>()[i]),
            ));
        }
        Ok(())
    }
}

// ── PV ───────────────────────────────────────────────────────────────────────
pub struct PvFunction;
impl VScalar for PvFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            write_f64(&mut out, i, financial::pv(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                Some(input.flat_vector(2).as_slice::<f64>()[i]),
                Some(input.flat_vector(3).as_slice::<f64>()[i]),
                Some(input.flat_vector(4).as_slice::<bool>()[i]),
            ));
        }
        Ok(())
    }
}

// ── PMT ──────────────────────────────────────────────────────────────────────
pub struct PmtFunction;
impl VScalar for PmtFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            write_f64(&mut out, i, calc_pmt(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<bool>()[i],
            ));
        }
        Ok(())
    }
}

// ── IPMT — Guard: per must be in [1, nper] ───────────────────────────────────
pub struct IpmtFunction;
impl VScalar for IpmtFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate = input.flat_vector(0).as_slice::<f64>()[i];
            let per  = input.flat_vector(1).as_slice::<f64>()[i];
            let nper = input.flat_vector(2).as_slice::<f64>()[i];
            let pv   = input.flat_vector(3).as_slice::<f64>()[i];
            let fv   = input.flat_vector(4).as_slice::<f64>()[i];
            let due  = input.flat_vector(5).as_slice::<bool>()[i];
            if per < 1.0 || per > nper || nper <= 0.0 { out.set_null(i); continue; }
            write_f64(&mut out, i, calc_ipmt(rate, per, nper, pv, fv, due));
        }
        Ok(())
    }
}

// ── PPMT — Guard: per must be in [1, nper] ───────────────────────────────────
pub struct PpmtFunction;
impl VScalar for PpmtFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate = input.flat_vector(0).as_slice::<f64>()[i];
            let per  = input.flat_vector(1).as_slice::<f64>()[i];
            let nper = input.flat_vector(2).as_slice::<f64>()[i];
            let pv   = input.flat_vector(3).as_slice::<f64>()[i];
            let fv   = input.flat_vector(4).as_slice::<f64>()[i];
            let due  = input.flat_vector(5).as_slice::<bool>()[i];
            if per < 1.0 || per > nper || nper <= 0.0 { out.set_null(i); continue; }
            write_f64(&mut out, i, calc_ppmt(rate, per, nper, pv, fv, due));
        }
        Ok(())
    }
}

// ── CUMIPMT — Guards: start <= end, both in [1, nper] ────────────────────────
pub struct CumipmtFunction;
impl VScalar for CumipmtFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate  = input.flat_vector(0).as_slice::<f64>()[i];
            let nper  = input.flat_vector(1).as_slice::<f64>()[i];
            let pv    = input.flat_vector(2).as_slice::<f64>()[i];
            let start = input.flat_vector(3).as_slice::<f64>()[i];
            let end   = input.flat_vector(4).as_slice::<f64>()[i];
            let due   = input.flat_vector(5).as_slice::<bool>()[i];
            if start < 1.0 || end < start || end > nper || nper <= 0.0 || rate < 0.0 {
                out.set_null(i); continue;
            }
            write_f64(&mut out, i, calc_cumipmt(rate, nper, pv, start, end, due));
        }
        Ok(())
    }
}

// ── CUMPRINC — Guards: start <= end, both in [1, nper] ───────────────────────
pub struct CumprincFunction;
impl VScalar for CumprincFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate  = input.flat_vector(0).as_slice::<f64>()[i];
            let nper  = input.flat_vector(1).as_slice::<f64>()[i];
            let pv    = input.flat_vector(2).as_slice::<f64>()[i];
            let start = input.flat_vector(3).as_slice::<f64>()[i];
            let end   = input.flat_vector(4).as_slice::<f64>()[i];
            let due   = input.flat_vector(5).as_slice::<bool>()[i];
            if start < 1.0 || end < start || end > nper || nper <= 0.0 || rate < 0.0 {
                out.set_null(i); continue;
            }
            write_f64(&mut out, i, calc_cumprinc(rate, nper, pv, start, end, due));
        }
        Ok(())
    }
}

// ── NPER ─────────────────────────────────────────────────────────────────────
pub struct NperFunction;
impl VScalar for NperFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), bln()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            write_f64(&mut out, i, calc_nper(
                input.flat_vector(0).as_slice::<f64>()[i],
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<bool>()[i],
            ));
        }
        Ok(())
    }
}

// ── RATE ─────────────────────────────────────────────────────────────────────
pub struct RateFunction;
impl VScalar for RateFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl(), bln(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let nper = input.flat_vector(0).as_slice::<f64>()[i];
            if nper <= 0.0 { out.set_null(i); continue; }
            write_f64(&mut out, i, calc_rate(
                nper,
                input.flat_vector(1).as_slice::<f64>()[i],
                input.flat_vector(2).as_slice::<f64>()[i],
                input.flat_vector(3).as_slice::<f64>()[i],
                input.flat_vector(4).as_slice::<bool>()[i],
                input.flat_vector(5).as_slice::<f64>()[i],
            ));
        }
        Ok(())
    }
}

// ── ISPMT — Guard: per must be in [1, nper] ───────────────────────────────────
pub struct IspmtFunction;
impl VScalar for IspmtFunction {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> { vec![sig(vec![dbl(), dbl(), dbl(), dbl()])] }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn Error>> {
        let len = input.len(); let mut out = output.flat_vector();
        for i in 0..len {
            let rate = input.flat_vector(0).as_slice::<f64>()[i];
            let per  = input.flat_vector(1).as_slice::<f64>()[i];
            let nper = input.flat_vector(2).as_slice::<f64>()[i];
            let pv   = input.flat_vector(3).as_slice::<f64>()[i];
            if nper <= 0.0 || per < 1.0 || per > nper { out.set_null(i); continue; }
            write_f64(&mut out, i, calc_ispmt(rate, per, nper, pv));
        }
        Ok(())
    }
}
