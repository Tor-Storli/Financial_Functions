// ── Shared helpers ────────────────────────────────────────────────────────────
use duckdb::core::DataChunkHandle;
use libduckdb_sys::{duckdb_string_t, duckdb_string_t_data, duckdb_string_t_length};
use chrono::NaiveDate;
use chrono::Datelike;
use std::slice;

// ── Safety: write f64 result, mapping NAN/Inf → NULL ─────────────────────────
/// Write a f64 result to a FlatVector at row i.
/// If the value is NaN or Infinite, write NULL instead of crashing DuckDB.
#[inline]
pub fn write_f64(out: &mut duckdb::core::FlatVector, i: usize, value: f64) {
    if value.is_nan() || value.is_infinite() {
        out.set_null(i);
    } else {
        out.as_mut_slice::<f64>()[i] = value;
    }
}

/// Read a VARCHAR value from a FlatVector at row i
pub unsafe fn read_varchar(input: &mut DataChunkHandle, col: usize, row: usize) -> String {
    let vec = input.flat_vector(col);
    let strings = vec.as_slice::<duckdb_string_t>();
    let s_copy: duckdb_string_t = strings[row];
    let ptr = unsafe { duckdb_string_t_data(&s_copy as *const _ as *mut _) };
    let len = unsafe { duckdb_string_t_length(s_copy) } as usize;
    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

/// Parse a comma-separated string into Vec<f64>.
/// Returns Err if the string is empty or any token fails to parse.
pub fn parse_csv_f64(s: &str) -> Result<Vec<f64>, String> {
    if s.trim().is_empty() {
        return Err("Empty values string".into());
    }
    s.split(',')
        .map(|v| {
            v.trim()
                .parse::<f64>()
                .map_err(|e| format!("Invalid number '{}': {}", v.trim(), e))
        })
        .collect()
}

/// Parse a comma-separated string into Vec<NaiveDate>.
/// Returns Err if the string is empty or any date fails to parse.
pub fn parse_csv_dates(s: &str) -> Result<Vec<NaiveDate>, String> {
    if s.trim().is_empty() {
        return Err("Empty dates string".into());
    }
    s.split(',')
        .map(|v| {
            NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{}': {}", v.trim(), e))
        })
        .collect()
}

/// Parse a date string into Option<NaiveDate> — None on failure.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

// ── Day-count convention (basis) ─────────────────────────────────────────────
//
// Excel basis codes:
//   0 = US (NASD) 30/360
//   1 = Actual/actual
//   2 = Actual/360
//   3 = Actual/365
//   4 = European 30/360

/// Fraction of year between two dates for a given basis (0-4).
pub fn year_frac(d1: NaiveDate, d2: NaiveDate, basis: i32) -> f64 {
    let days = (d2 - d1).num_days() as f64;
    match basis {
        0 => days_30_360_us(d1, d2) / 360.0,
        1 => actual_actual(d1, d2),
        2 => days / 360.0,
        3 => days / 365.0,
        4 => days_30_360_eu(d1, d2) / 360.0,
        _ => days / 365.0,
    }
}

fn days_30_360_us(d1: NaiveDate, d2: NaiveDate) -> f64 {
    let (y1, m1, mut dd1) = (d1.year(), d1.month(), d1.day());
    let (y2, m2, mut dd2) = (d2.year(), d2.month(), d2.day());
    if dd1 == 31 { dd1 = 30; }
    if dd2 == 31 && dd1 == 30 { dd2 = 30; }
    (360 * (y2 - y1) + 30 * (m2 as i32 - m1 as i32) + (dd2 as i32 - dd1 as i32)) as f64
}

fn days_30_360_eu(d1: NaiveDate, d2: NaiveDate) -> f64 {
    let (y1, m1, mut dd1) = (d1.year(), d1.month(), d1.day());
    let (y2, m2, mut dd2) = (d2.year(), d2.month(), d2.day());
    if dd1 == 31 { dd1 = 30; }
    if dd2 == 31 { dd2 = 30; }
    (360 * (y2 - y1) + 30 * (m2 as i32 - m1 as i32) + (dd2 as i32 - dd1 as i32)) as f64
}

fn actual_actual(d1: NaiveDate, d2: NaiveDate) -> f64 {
    let days = (d2 - d1).num_days() as f64;
    let y1 = d1.year();
    let y2 = d2.year();
    if y1 == y2 {
        let year_days = if is_leap(y1) { 366.0 } else { 365.0 };
        return days / year_days;
    }
    let end_y1 = NaiveDate::from_ymd_opt(y1 + 1, 1, 1).unwrap();
    let start_y2 = NaiveDate::from_ymd_opt(y2, 1, 1).unwrap();
    let frac1 = (end_y1 - d1).num_days() as f64 / (if is_leap(y1) { 366.0 } else { 365.0 });
    let frac2 = (d2 - start_y2).num_days() as f64 / (if is_leap(y2) { 366.0 } else { 365.0 });
    let full_years = (y2 - y1 - 1) as f64;
    frac1 + full_years + frac2
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Add `months` calendar months to a date, clamping to end-of-month
pub fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total_months = d.month() as i32 - 1 + months;
    // div_euclid/rem_euclid always floor-divides correctly for negatives
    // Rust's normal / truncates toward zero which breaks negative month arithmetic
    let year = d.year() + total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) as u32 + 1;
    let max_day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, d.day().min(max_day)).unwrap()
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap(year) { 29 } else { 28 },
        _ => 30,
    }
}

/// Number of coupons per year from frequency code (1=annual, 2=semi, 4=quarterly)
pub fn freq_per_year(frequency: i32) -> i32 {
    match frequency { 1 | 2 | 4 => frequency, _ => 2 }
}
