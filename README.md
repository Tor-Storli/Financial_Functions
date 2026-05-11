---
title: "Building a DuckDB Financial Extension in Rust"
subtitle: "Implementing All 55 Excel-Compatible Financial Functions"
author: "Tor Storli and  Claude Sonnet 4.6 (Anthropic's AI assistant)"
date: today
format:
  html:
    toc: true
    toc-depth: 3
    toc-title: "Contents"
    code-fold: false
    highlight-style: github
    theme: cosmo
    self-contained: true
  pdf:
    toc: true
    toc-depth: 3
    highlight-style: github
execute:
  eval: false
---

## Documentation

[View Creating Extension Documentation](https://tor-storli.github.io/Financial_Functions/duckdb_financial_extension.html){target="_blank"}      

[View Publish Extension Documentation](https://tor-storli.github.io/Financial_Functions/publishing_duckdb_extension.html){target="_blank"}

# Overview

This tutorial walks through building a **DuckDB community extension in Rust** that exposes 55 Excel-compatible financial functions directly inside DuckDB SQL queries. By the end you will understand:

- How the DuckDB `VScalar` trait works in Rust
- How the `financial` crate implements Excel financial formulas
- How to organise a multi-module Rust extension
- How to register scalar functions in a loadable DuckDB extension
- How to use all 55 functions in real SQL queries

## What We Are Building

The 55 functions are organised into six groups:

| Group                  | Functions                                                                                                                                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Annuity**            | `fv`, `pv`, `pmt`, `ipmt`, `ppmt`, `cumipmt`, `cumprinc`, `nper`, `rate`, `ispmt`                                                                                        |
| **Cash Flows**         | `npv`, `xnpv`, `irr`, `xirr`, `mirr`                                                                                                                                     |
| **Depreciation**       | `sln`, `syd`, `db`, `ddb`, `vdb`, `amorlinc`, `amordegrc`                                                                                                                |
| **Coupon Dates**       | `coupdaybs`, `coupdays`, `coupdaysnc`, `coupncd`, `couppcd`, `coupnum`                                                                                                   |
| **Bonds & Securities** | `price`, `pricedisc`, `pricemat`, `yield`, `yielddisc`, `yieldmat`, `disc`, `intrate`, `received`, `duration`, `mduration`, `accrint`, `accrintm`                        |
| **Miscellaneous**      | `effect`, `nominal`, `dollarde`, `dollarfr`, `fvschedule`, `rri`, `pduration`, `tbillprice`, `tbillyield`, `tbilleq`, `oddfprice`, `oddfyield`, `oddlprice`, `oddlyield` |

---

### Excel Financial Functions

::: {.callout-note}

### Excel Financial Functions - Note

You can find the Excel Financial Functions and examples here:<br>
[Excel Financial Functions Reference](https://support.microsoft.com/en-us/office/financial-functions-reference-5658d81e-6035-4f24-89c1-fbf124c2b1d8){target="\_blank"}
:::

# Project Setup

## Project Structure

As the number of functions grows, splitting into modules keeps the code maintainable:

```
Financial_Functions/
├── Cargo.toml
└── src/
    ├── lib.rs           ← registers all 55 functions, nothing else
    ├── helpers.rs       ← read_varchar, parse_csv_f64, year_frac, day-count conventions
    ├── annuity.rs       ← FV, PV, PMT, IPMT, PPMT, CUMIPMT, CUMPRINC, NPER, RATE, ISPMT
    ├── cash_flows.rs    ← NPV, IRR, MIRR, XNPV, XIRR
    ├── depreciation.rs  ← SLN, SYD, DB, DDB, VDB, AMORDEGRC, AMORLINC
    ├── coupons.rs       ← COUPDAYBS, COUPDAYS, COUPDAYSNC, COUPNCD, COUPPCD, COUPNUM
    ├── bonds.rs         ← PRICE, PRICEDISC, PRICEMAT, YIELD, YIELDDISC, YIELDMAT,
    │                       DISC, INTRATE, RECEIVED, DURATION, MDURATION, ACCRINT, ACCRINTM
    └── misc.rs          ← EFFECT, NOMINAL, DOLLARDE, DOLLARFR, FVSCHEDULE, RRI, PDURATION,
                            TBILLEQ, TBILLPRICE, TBILLYIELD, ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD
```

## Cargo.toml

```toml
[package]
name = "fin_functions"
version = "0.1.0"
edition = "2024"

[dependencies]
duckdb = { version = "1.10501.0", features = ["vtab-loadable", "vscalar"] }
duckdb-ext-macros = "0.1.0"
duckdb-loadable-macros = "1.10501.0"
libduckdb-sys = { version = "1.10501.0", features = ["loadable-extension"] }
financial = "1.1.5"
chrono = "0.4"

[lib]
crate-type = ["cdylib"]

[profile.release]
lto = true
strip = true
```

::: {.callout-note}
The `duckdb-rs` crate encodes the DuckDB version in its semver string. Version `1.10501.0` corresponds to **DuckDB v1.5.1**. Always keep `duckdb`, `duckdb-loadable-macros`, and `libduckdb-sys` on the same version.  
[duckdb Rust Crate](https://lib.rs/crates/duckdb){target="\_blank"}  
[duckdb-ext-macros Rust Crate](https://crates.io/crates/duckdb-ext-macros){target="\_blank"}  
[duckdb-loadable-macros Rust Crate](https://crates.io/crates/duckdb-loadable-macros){target="\_blank"}  
[libduckdb-sys Rust Crate](https://crates.io/crates/libduckdb-sys){target="\_blank"}
:::

### Install Rust

::: {.callout-note}

### Install Rust - Note

You can install Rust from here:<br>
[Install Rust](https://rust-lang.org/tools/install){target="\_blank"}
:::

### Find Crates for the Rust language

::: {.callout-note}

### Find Crates - Note

You can find available crates for Rust here:<br>
[Find Available Crates](https://crates.io/){target="\_blank"}
:::

### Documentation for Crates

::: {.callout-note}

### More detailed Crate Documentation - Note

You can find more details about a crate here:<br>
[Show Crates Documentation](https://docs.rs/){target="\_blank"}
:::

### Build a DuckDB Extension in Rust in 4 Commands

::: {.callout-note}

### cargo-duckdb-ext-tools v0.5.0 - Note

A Cargo plugin that makes writing DuckDB extensions in Rust feel like a normal Rust workflow:<br>
[cargo-duckdb-ext-tools v0.5.0](https://www.reddit.com/r/DuckDB/comments/1rgz377/build_a_duckdb_extension_in_rust_in_4_commands/){target="\_blank"}
:::

## Useful Terminal Commands (bash)

```bash
rustc --version                 # Rust version
cargo --version                 # Cargo version
cargo help                      # What cargo can do
cargo new Financial_Functions   # Create new project
cargo init --lib                # initialize a fresh Rust project (from empty folder)
cargo run                       # Run the project
cargo tree                      # View dependency tree
cargo add financial@1.1.5       # Add a specific crate version
cargo metadata                  # Full dependency list as JSON
grep "financial" Cargo.toml     # Is financial crate listed in toml file?
```

## Building and Packaging

```bash
# To create a new Rust project in a bash terminal window
# navigate to the Root folder where the new project will be created
# i.e.  C:/Users/<Your User Name>
# Run this command to create a new project
# i.e cargo new Financial_Functions
cargo new <name of new project>

# OR

# Open the empty Financial_Functions folder in VSCode
# initialize a fresh Rust project properly first to
# create the src, .gitignore, and Cargo.toml files:
cargo init --lib

# Remove any target folder - if exist
cargo clean

# Build and publish project in target folder
cargo build --release

# Package as a DuckDB extension file using latest
# version of Duckdb 1.5.2
cargo duckdb-ext build --duckdb-version v1.5.2 -- --release
```

This produces `target/release/fin_functions.duckdb_extension`.

---

# The VScalar Trait

::: {.callout-note}

### The Rust Programming Language - Note

In Rust, Traits are similar to interfaces in other languages (like C# or Java).<br>
[The Rust Programming Language - Traits](https://doc.rust-lang.org/book/ch10-02-traits.html){target="\_blank"}
:::

Every scalar function in this extension implements the `VScalar` trait:

```rust
pub trait VScalar: Sized {
    type State: Sized + Send + Sync + 'static;
    fn signatures() -> Vec<ScalarFunctionSignature>;
    unsafe fn invoke(
        state: &Self::State,
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn Error>>;
}
```

::: {.callout-important}

### Rust 2024 Edition Note

In Rust 2024 edition, calling an `unsafe fn` from inside another `unsafe fn` still requires an explicit `unsafe {}` block at the call site. This is a breaking change from earlier editions.
:::

## Standard Imports

```rust
use duckdb::core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId};
use duckdb::vscalar::{ScalarFunctionSignature, VScalar};
use duckdb::vtab::arrow::WritableVector;
use libduckdb_sys::{duckdb_string_t, duckdb_string_t_data, duckdb_string_t_length};
use duckdb_loadable_macros::duckdb_entrypoint_c_api;
use std::error::Error;
use std::slice;
use chrono::NaiveDate;
use chrono::Datelike;   // required for .year(), .month(), .day()
```

---

# Helper Functions

The `helpers.rs` module is the foundation of the entire extension. Every other module imports from it. Getting these helpers right — especially around error handling and date arithmetic — determines whether the extension crashes DuckDB on bad input or handles it gracefully.

## Reading VARCHAR Columns

DuckDB stores `VARCHAR` values internally as `duckdb_string_t`. There is no high-level `get_string()` method, so we read the raw struct directly:

```rust
pub unsafe fn read_varchar(input: &mut DataChunkHandle, col: usize, row: usize) -> String {
    let vec = input.flat_vector(col);
    let strings = vec.as_slice::<duckdb_string_t>();
    let s_copy: duckdb_string_t = strings[row];
    let ptr = unsafe { duckdb_string_t_data(&s_copy as *const _ as *mut _) };
    let len = unsafe { duckdb_string_t_length(s_copy) } as usize;
    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}
```

## Safe Output Writing — `write_f64`

Every function that produces a numeric result routes through this single helper. It intercepts `NaN` and `Infinity` values — which would crash DuckDB — and writes `NULL` instead:

```rust
#[inline]
pub fn write_f64(out: &mut duckdb::core::FlatVector, i: usize, value: f64) {
    if value.is_nan() || value.is_infinite() {
        out.set_null(i);
    } else {
        out.as_mut_slice::<f64>()[i] = value;
    }
}
```

::: {.callout-important}

### Why This Matters

Without `write_f64`, a function like `RATE` that returns `NaN` when Newton-Raphson fails to converge would write a raw `NaN` bit pattern into DuckDB's output buffer. DuckDB does not expect this and the query crashes. By routing every output through `write_f64`, any edge case silently becomes `NULL` — the SQL-idiomatic way to say "no result".
:::

The pattern every function body follows:

```rust
let result = some_calculation(...);
write_f64(&mut out, i, result);  // NaN/Inf → NULL automatically
```

## Parsing CSV Inputs — Returning `Result`

Array inputs (NPV, IRR, XNPV, XIRR, MIRR, FVSCHEDULE) are passed as comma-separated strings since DuckDB scalar functions operate row-by-row. The parsers return `Result` so callers can write `NULL` on bad input rather than panicking or silently using `0.0`:

```rust
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
```

::: {.callout-note}

### Earlier Version Used `unwrap_or(0.0)`

The original helpers used `.unwrap_or(0.0)` on every parse, which silently used zero for any bad token. This produced wrong answers without any indication of the problem. The `Result`-returning version surfaces the error so callers can write `NULL` and let the user know something was wrong.
:::

The calling pattern in every cash-flow function:

```rust
match parse_csv_f64(&raw) {
    Ok(values) => write_f64(&mut out, i, calculate(&values)),
    Err(_)     => out.set_null(i),   // bad input → NULL, not a crash
}
```

## Parsing Single Dates — Returning `Option`

Single date parameters (bonds, coupons, odd-period functions) use `parse_date`, which returns `Option<NaiveDate>` so the caller can write `NULL` when the date string is invalid rather than substituting a fallback date like `1970-01-01`:

```rust
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}
```

The calling pattern in every bond and coupon function:

```rust
match (parse_date(&settle_str), parse_date(&mature_str)) {
    (Some(settle), Some(mature)) => write_f64(&mut out, i, calc_price(settle, mature, ...)),
    _ => out.set_null(i),   // any invalid date → NULL
}
```

## Month Arithmetic — `add_months` and the `div_euclid` Fix

Coupon date functions need to step backwards and forwards through coupon dates by a fixed number of months. The naive implementation using Rust's `/` operator has a subtle but catastrophic bug:

```rust
// BUG: Rust's / truncates toward zero for negative numbers
// add_months(2011-05-15, -6):
//   total_months = 4 + (-6) = -2
//   -2 / 12 = 0  ← should be -1 !
//   year stays 2011 instead of becoming 2010
//   → next_coupon() enters an infinite loop
```

This caused `coupdaybs`, `coupdays`, `coupdaysnc`, `coupncd`, `couppcd`, and `coupnum` to hang indefinitely on any query. The fix uses `div_euclid` and `rem_euclid`, which always floor-divide correctly regardless of sign:

```rust
pub fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total_months = d.month() as i32 - 1 + months;
    // div_euclid always floors toward negative infinity — correct for calendar math
    // Rust's normal / truncates toward zero, which breaks negative month steps
    let year  = d.year() + total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) as u32 + 1;
    let max_day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, d.day().min(max_day)).unwrap()
}
```

With the fix:

- `-2.div_euclid(12) = -1` → year becomes `2010` ✓
- `-2.rem_euclid(12) = 10` → month becomes `11` ✓
- `add_months(2011-05-15, -6)` correctly returns `2010-11-15` ✓

## Day-Count Conventions

Bond and coupon functions share a `year_frac` helper that supports all five Excel basis codes:

| Code | Convention       |
| ---- | ---------------- |
| 0    | US (NASD) 30/360 |
| 1    | Actual/Actual    |
| 2    | Actual/360       |
| 3    | Actual/365       |
| 4    | European 30/360  |

```rust
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
```

::: {.callout-note}

### Why Centralise Day-Count Logic?

Fourteen bond/coupon functions all need the same day-count logic. If each function computed it independently, a subtle difference in the 30/360 end-of-month rules would produce different results for the same inputs. Centralising in `year_frac` ensures all functions agree — and a single fix propagates everywhere.
:::

## Helper Functions Summary

| Helper            | Returns                  | Purpose                                         |
| ----------------- | ------------------------ | ----------------------------------------------- |
| `read_varchar`    | `String`                 | Read a VARCHAR column value from DuckDB         |
| `write_f64`       | `()`                     | Write result, mapping NaN/Inf → NULL            |
| `parse_csv_f64`   | `Result<Vec<f64>>`       | Parse comma-separated numbers                   |
| `parse_csv_dates` | `Result<Vec<NaiveDate>>` | Parse comma-separated date strings              |
| `parse_date`      | `Option<NaiveDate>`      | Parse a single date string                      |
| `add_months`      | `NaiveDate`              | Add/subtract months with correct floor division |
| `year_frac`       | `f64`                    | Year fraction between two dates (5 basis codes) |
| `freq_per_year`   | `i32`                    | Validate coupon frequency (1/2/4)               |

---

# Iterative Solvers — Newton-Raphson and Bisection

Several financial functions have no closed-form solution. Given a price, what is the yield? Given cash flows, what rate makes NPV exactly zero? These questions require iterative numerical methods that trial-and-error their way to an answer.

## The Core Problem

Consider IRR for this cash flow stream from the Excel examples:

```
Period 0: -$70,000  (initial investment)
Period 1: +$12,000
Period 2: +$15,000
Period 3: +$18,000
Period 4: +$21,000
Period 5: +$26,000
```

IRR is the rate `r` where:

$$NPV(r) = -70000 + \frac{12000}{(1+r)^1} + \frac{15000}{(1+r)^2} + \frac{18000}{(1+r)^3} + \frac{21000}{(1+r)^4} + \frac{26000}{(1+r)^5} = 0$$

There is no algebraic way to solve a polynomial of this degree for `r`. We need to search for it numerically.

## Newton-Raphson — Used by `RATE`

Newton-Raphson is the faster method. It uses the **derivative** (slope) of the function to jump directly toward the root rather than searching blindly.

### The Algorithm

Starting from an initial guess `r₀`:

$$r_{n+1} = r_n - \frac{f(r_n)}{f'(r_n)}$$

Where:

- `f(r)` = NPV at rate `r`
- `f'(r)` = slope of NPV curve at `r`

The idea is geometric: draw a tangent line at your current point and see where it hits zero. That intersection is your next, better guess.

### Step-by-Step Example: RATE

Suppose we have a $8,000 loan, 48 monthly payments of $200, and want to find the monthly rate.

```sql
SELECT ROUND(rate(48, -200, 8000, 0, false, 0.1) * 12, 4) AS annual_rate;
-- Result: 0.0924  (9.24% annual)
```

Internally, the solver works like this:

| Iteration | Current guess `r` | `f(r)` = NPV error | `f'(r)` = slope | Next guess                        |
| --------- | ----------------- | ------------------ | --------------- | --------------------------------- |
| 0         | 0.10000           | +4,521.33          | -18,422.1       | 0.10000 − (4521/−18422) = 0.34546 |
| 1         | 0.34546           | +2,108.44          | −8,931.2        | 0.10940                           |
| 2         | 0.10940           | +48.22             | −8,204.1        | 0.10881                           |
| 3         | 0.10881           | +0.03              | −8,199.7        | 0.10881                           |
| 4         | 0.10881           | < 1e-10            | —               | **converged**                     |

Monthly rate = `0.10881 / 12 = 0.007701`. Annual = `0.007701 × 12 = 9.24%` ✓

```rust
pub fn calc_rate(nper: f64, pmt: f64, pv: f64, fv: f64, pmt_at_beg: bool, guess: f64) -> f64 {
    let max_iter = 300;
    let tol = 1e-10;
    let mut r = guess;
    for _ in 0..max_iter {
        let r1 = 1.0 + r;
        let r1n = r1.powf(nper);
        let due = if pmt_at_beg { r1 } else { 1.0 };
        // f(r): the TVM equation evaluated at current r
        let f = pv * r1n + pmt * due * (r1n - 1.0) / r + fv;
        // f'(r): analytical derivative of the TVM equation
        let df = pv * nper * r1.powf(nper - 1.0)
                + pmt * due * (nper * r1.powf(nper - 1.0) * r - (r1n - 1.0)) / (r * r);
        if df.abs() < 1e-20 { return f64::NAN; }  // flat slope — cannot proceed
        let r_new = r - f / df;
        if (r_new - r).abs() < tol { return r_new; }  // converged
        r = r_new;
    }
    f64::NAN  // did not converge → write_f64 maps this to NULL
}
```

::: {.callout-warning}

### When Newton-Raphson Fails

Newton-Raphson can fail to converge if:

- The initial `guess` is far from the true answer and the function is highly curved
- The derivative `f'(r)` is near zero (flat region of the curve)
- The function has multiple roots and the solver oscillates between them

When `calc_rate` returns `NaN`, try a different `guess` value. The most common fix is to start closer to the expected answer (e.g., `0.01` for a typical loan rate rather than the default `0.1`).
:::

## Bisection — Used by IRR, YIELD, XIRR, and Odd-Period Functions

Bisection is slower but **guaranteed to converge** if you can find a bracket `[lo, hi]` where `f(lo)` and `f(hi)` have opposite signs — meaning the root must lie between them.

### The Algorithm

1. Evaluate `f` at both ends of the bracket
2. If signs differ, a root exists between them (Intermediate Value Theorem)
3. Evaluate at the midpoint
4. Replace whichever end has the same sign as the midpoint
5. The bracket halves each iteration — repeat until narrow enough

### Step-by-Step Example: IRR

Using the five-year cash flow example:

```sql
SELECT ROUND(irr('-70000,12000,15000,18000,21000,26000'), 4) AS irr_result;
-- Result: 0.0866  (8.66%)
```

The solver scans bracket candidates `[-0.9999, -0.5, -0.2, -0.1, -0.01, 0.0, 0.01, 0.1, 0.5, 1.0, ...]` looking for a sign change:

| Bracket         | `NPV(lo)` | `NPV(hi)`  | Sign change? |
| --------------- | --------- | ---------- | ------------ |
| [-0.9999, -0.5] | −∞        | −47,233    | No           |
| [-0.5, -0.2]    | −47,233   | −14,891    | No           |
| [-0.2, -0.1]    | −14,891   | −3,822     | No           |
| [-0.1, -0.01]   | −3,822    | −368       | No           |
| [-0.01, 0.0]    | −368      | +2,000     | No           |
| [0.0, 0.01]     | +2,000    | +1,718     | No           |
| [0.01, 0.1]     | +1,718    | **−1,023** | **Yes ✓**    |

Now bisect `[0.01, 0.1]`:

| Iteration | lo      | hi      | mid         | NPV(mid) | Action        |
| --------- | ------- | ------- | ----------- | -------- | ------------- |
| 1         | 0.0100  | 0.1000  | 0.0550      | +513.2   | lo = 0.0550   |
| 2         | 0.0550  | 0.1000  | 0.0775      | −205.1   | hi = 0.0775   |
| 3         | 0.0550  | 0.0775  | 0.0663      | +150.3   | lo = 0.0663   |
| 4         | 0.0663  | 0.0775  | 0.0719      | −23.4    | hi = 0.0719   |
| ...       | ...     | ...     | ...         | ...      | ...           |
| 30        | 0.08659 | 0.08660 | **0.08660** | ≈ 0.0    | **converged** |

```sql
SELECT ROUND(irr('-70000,12000,15000,18000,21000,26000'), 4) AS irr_result;
-- Result: 0.0866  ✓
```

The Rust implementation:

```rust
fn calc_irr(values: &[f64]) -> f64 {
    // Guard: no solution if all cash flows have the same sign
    if !values.iter().any(|&v| v > 0.0) || !values.iter().any(|&v| v < 0.0) {
        return f64::NAN;
    }
    // Scan bracket candidates
    let points = [-0.9999f64, -0.5, -0.2, -0.1, -0.01, 0.0,
                   0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 100.0];
    for w in points.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let flo = npv_at(values, lo);
        let fhi = npv_at(values, hi);
        // Found a sign change — root must be in this bracket
        if flo.is_finite() && fhi.is_finite() && flo * fhi <= 0.0 {
            return bisect_irr(values, lo, hi);
        }
    }
    f64::NAN  // no bracket found
}

fn bisect_irr(values: &[f64], mut lo: f64, mut hi: f64) -> f64 {
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if (hi - lo).abs() < 1e-10 { return mid; }
        let fm = npv_at(values, mid);
        if fm.abs() < 1e-12 { return mid; }
        if npv_at(values, lo) * fm <= 0.0 { hi = mid; } else { lo = mid; }
    }
    (lo + hi) / 2.0
}
```

## Why IRR Uses Bisection Instead of Newton-Raphson

The `financial` crate's Newton-Raphson IRR implementation can **panic** (not just return an error) with certain inputs — specifically when the bracket search passes bounds where the lower bound is greater than the upper bound. Because this is a Rust panic rather than a returned `Err`, even `std::panic::catch_unwind` cannot reliably catch it inside a DuckDB extension:

```
thread panicked at financial-1.1.5\src\common\find_root.rs:157:13:
lower cannot be greater than upper bounds
panic in a function that cannot unwind
```

The bisection implementation replaces the `financial` crate entirely for IRR, XIRR, and MIRR, giving full control over the search bounds and eliminating any possibility of a panic reaching DuckDB.

## MIRR — No Iteration Required

MIRR has a closed-form solution, so no iteration is needed at all. It is included here because it is often grouped with IRR:

$$MIRR = \left(\frac{NPV_{positive}}{|NPV_{negative}|}\right)^{\frac{1}{n-1}} \times (1 + r_{reinvest}) - 1$$

```sql
SELECT ROUND(mirr('-120000,39000,30000,21000,37000,46000', 0.10, 0.12), 4) AS mirr_result;
-- Result: 0.1261  (12.61%)
```

Step by step:

- Positive cash flows `[39000, 30000, 21000, 37000, 46000]` discounted at `reinvest_rate=12%` to `t=0`
- NPV_positive = `39000/1.12 + 30000/1.12² + 21000/1.12³ + 37000/1.12⁴ + 46000/1.12⁵ = 116,281`
- Negative cash flows `[-120000]` discounted at `finance_rate=10%` to `t=0`
- NPV_negative = `120000` (already at t=0)
- MIRR = `(116281/120000)^(1/5) × 1.12 − 1 = 0.1261` ✓

## Which Functions Use Which Solver

| Function    | Method                                 | Fallback                              |
| ----------- | -------------------------------------- | ------------------------------------- |
| `rate`      | Newton-Raphson (analytical derivative) | Returns NULL on non-convergence       |
| `irr`       | Bisection (13 bracket candidates)      | Returns NULL if no bracket found      |
| `xirr`      | Bisection (13 bracket candidates)      | Returns NULL if no bracket found      |
| `mirr`      | Closed-form formula                    | Returns NULL on invalid input         |
| `yield`     | Newton-Raphson (numerical derivative)  | Returns NULL on non-convergence       |
| `oddfyield` | Bisection `[0, 1]`                     | Returns midpoint after 200 iterations |
| `oddlyield` | Bisection `[0, 1]`                     | Returns midpoint after 200 iterations |

---

# Annuity Functions

These functions cover time-value-of-money calculations for loans and savings.

## FV — Future Value

**DuckDB signature:** `fv(rate, nper, pmt, pv, pmt_at_beginning)`

**Formula:** $FV = -\left(PV \cdot (1+r)^n + PMT \cdot \frac{(1+r)^n - 1}{r} \cdot (1 + r \cdot \text{type})\right)$

```sql
-- Excel example: FV(0.06/12, 10, -200, -500, 1) = 2581.40
SELECT ROUND(fv(0.06/12, 10, -200, -500, true), 2) AS fv_result;
-- Result: 2581.40

-- Save $200/month for 10 years at 5% annual interest
SELECT ROUND(fv(0.05/12, 120.0, -200.0, 0.0, false), 2) AS future_value;
-- Result: 31056.46
```

## PV — Present Value

**DuckDB signature:** `pv(rate, nper, pmt, fv, pmt_at_beginning)`

**Formula:** $PV = -\left(PMT \cdot \frac{1 - (1+r)^{-n}}{r} \cdot (1 + r \cdot \text{type}) + FV \cdot (1+r)^{-n}\right)$

```sql
-- Excel example: PV(8%/12, 20*12, 500, 0, false) = -59777.15
SELECT ROUND(pv(0.08/12, 240, 500, 0, false), 2) AS pv_result;
-- Result: -59777.15
```

## PMT — Periodic Payment

**DuckDB signature:** `pmt(rate, nper, pv, fv, pmt_at_beginning)`

**Formula:** $PMT = -\frac{PV \cdot (1+r)^n + FV}{\frac{(1+r)^n - 1}{r} \cdot (1 + r \cdot \text{type})}$

```sql
-- Excel example: PMT(8%/12, 10, 10000, 0, false) = -1037.03
SELECT ROUND(pmt(0.08/12, 10, 10000, 0, false), 2) AS pmt_result;
-- Result: -1037.03

-- Monthly payment on $350,000 mortgage at 3.25% over 15 years
SELECT ROUND(pmt(0.0325/12, 180.0, 350000.0, 0.0, false), 2) AS monthly_payment;
-- Result: -2459.34
```

## IPMT — Interest Portion of Payment

**DuckDB signature:** `ipmt(rate, per, nper, pv, fv, pmt_at_beginning)`

```sql
-- Excel example: IPMT(10%/12, 1, 3, 8000, 0, false) = -66.67
SELECT ROUND(ipmt(0.10/12, 1, 3, 8000, 0, false), 2) AS ipmt_result;
-- Result: -66.67

-- Interest in month 1 of the $350k mortgage
SELECT ROUND(ipmt(0.0325/12, 1.0, 180.0, 350000.0, 0.0, false), 2) AS interest_month_1;
-- Result: -947.92
```

## PPMT — Principal Portion of Payment

**DuckDB signature:** `ppmt(rate, per, nper, pv, fv, pmt_at_beginning)`

```sql
-- Excel example: PPMT(10%/12, 1, 2*12, 2000, 0, false) = -75.62
SELECT ROUND(ppmt(0.1/12,1 , 2 * 12, 2000, 0, false), 2) AS ppmt_result;
-- Result: -75.62
```

## CUMIPMT — Cumulative Interest

**DuckDB signature:** `cumipmt(rate, nper, pv, start_period, end_period, pmt_at_beginning)`

```sql
-- Excel example: CUMIPMT(9%/12, 30*12, 125000, 1, 1, false) = -937.50
SELECT ROUND(cumipmt(0.09/12, 360, 125000, 1, 1, false), 2) AS cumipmt_result;
-- Result: -937.50

-- Total interest paid in year 1 of the $350k mortgage (periods 1–12)
SELECT ROUND(cumipmt(0.0325/12, 180.0, 350000.0, 1.0, 12.0, false), 2) AS interest_year_1;
-- Result: -11102.38
```

## CUMPRINC — Cumulative Principal

**DuckDB signature:** `cumprinc(rate, nper, pv, start_period, end_period, pmt_at_beginning)`

```sql
-- Excel example: The total principal paid in the
--                second year of payments, periods 13 through 24
SELECT ROUND(cumprinc(0.09/12, 360, 125000, 13, 24, false), 2) AS cumprinc_result;
-- Result:  -934.11
-- The principal paid in a single payment in the first month
SELECT ROUND(cumprinc(0.09/12, 360, 125000, 1, 1, false), 2) AS cumprinc_result;
-- Result: -68.28
```

## NPER — Number of Periods

**DuckDB signature:** `nper(rate, pmt, pv, fv, pmt_at_beginning)`

**Formula:** $NPER = \frac{\ln\left(\frac{PMT \cdot due - FV \cdot r}{PMT \cdot due + PV \cdot r}\right)}{\ln(1+r)}$

```sql
-- Excel example: NPER(12%/12, -100, -1000, 10000, true) = 59.67
-- Payment is due at the beginning of the period
SELECT ROUND(nper(0.12/12, -100, -1000, 10000, true), 2) AS nper_result;
-- Result: 59.67
```

## RATE — Periodic Interest Rate

**DuckDB signature:** `rate(nper, pmt, pv, fv, pmt_at_beginning, guess)`

Uses Newton-Raphson iteration — no closed-form solution exists. Returns `NULL` if it cannot converge; try a different `guess` value.

```sql
-- Excel example: RATE(48, -200, 8000, 0, false, 0.1) * 12 = 9.24% annual
SELECT ROUND(rate(4*12, -200, 8000, 0, false, 0.1) * 12, 4) AS annual_rate;
-- Result: 0.0924

-- Round-trip validation with PMT and NPER
SELECT
    ROUND(pmt(0.0325/12, 180.0, 350000.0, 0.0, false), 2)             AS pmt,
    ROUND(nper(0.0325/12, -2459.34, 350000.0, 0.0, false), 2)         AS nper,
    ROUND(rate(180.0, -2459.34, 350000.0, 0.0, false, 0.01) * 12, 4) AS annual_rate;
-- pmt: -2459.34 | nper: 180.00 | annual_rate: 0.0325  ✓
```

## ISPMT — Interest Paid (Straight-Line Loan)

**DuckDB signature:** `ispmt(rate, per, nper, pv)`

Returns interest paid in a specific period of a straight-line (non-amortizing) loan, where principal is repaid in equal instalments each period.

```sql
-- Example: ISPMT(10%/12, 1, 36, 8000000) =  -66666.67
SELECT ROUND(ispmt(0.10/12, 1, 36, 8000000), 2) AS ispmt_result;
-- Result:  -66666.67
```

## Loan Amortization Table

With all annuity functions registered, a complete amortization table is a single query:

```sql
WITH params AS (
    SELECT 0.0325 AS annual_rate, 180 AS nper, 350000 AS pv
),
periods AS (
    SELECT unnest(generate_series(1, (SELECT nper FROM params))) AS period
)
SELECT
    period,
    ROUND(pmt(p.annual_rate/12, p.nper, p.pv, 0.0, false), 2)                                 AS pmt,
    ROUND(ipmt(p.annual_rate/12, period::DOUBLE, p.nper, p.pv, 0.0, false), 2)                AS ipmt,
    ROUND(ppmt(p.annual_rate/12, period::DOUBLE, p.nper, p.pv, 0.0, false), 2)                AS ppmt,
    ROUND(cumipmt(p.annual_rate/12, p.nper, p.pv, 1.0, period::DOUBLE, false), 2)             AS cumipmt,
    ROUND(cumprinc(p.annual_rate/12, p.nper, p.pv, 1.0, period::DOUBLE, false), 2)            AS cumprinc,
    ROUND(p.pv + cumprinc(p.annual_rate/12, p.nper, p.pv, 1.0, period::DOUBLE, false), 2)     AS balance
FROM periods
CROSS JOIN params p
ORDER BY period;
```

| period | pmt      | ipmt    | ppmt     | cumipmt   | cumprinc   | balance    |
| ------ | -------- | ------- | -------- | --------- | ---------- | ---------- |
| 1      | -2459.34 | -947.92 | -1511.42 | -947.92   | -1511.42   | 348,488.58 |
| 2      | -2459.34 | -943.82 | -1515.52 | -1891.74  | -3026.94   | 346,973.06 |
| ...    |          |         |          |           |            |            |
| 180    | -2459.34 | -6.64   | -2452.70 | -92681.32 | -350000.00 | 0.00       |

::: {.callout-warning}

### Sign Convention

All payment functions return **negative values** for cash outflows, matching Excel. When computing `balance`, use `350000 + cumprinc(...)` — the `+` is correct because `cumprinc` returns a negative number.
:::

---

# Cash Flow Functions

## NPV — Net Present Value

**DuckDB signature:** `npv(rate, values_csv)`

**Formula:** $NPV = \sum_{i=1}^{n} \frac{CF_i}{(1+r)^i}$

Cash flows are passed as a comma-separated string since scalar functions operate row-by-row.

```sql
-- Excel example: NPV(10%, -10000, 3000, 4200, 6800) = 1188.44
-- Note: Initial cost of investment "ONE YEAR FROM TODAY" = -10000
SELECT ROUND(npv(0.10, '-10000,3000,4200,6800'), 2) AS npv_result;
-- Result: 1188.44

-- Excel example:
-- Project: $40,000 investment, returns $8,000, $9,200, $10,000, $12,000, $14,500 at 8%
-- Note: Initial cost of investment "NOW" = -40000
SELECT ROUND(npv(0.08, '8000,9200,10000, 12000, 14500') + -40000, 2) AS npv_result;
-- Result: 1922.06   (positive = invest!)
```

## IRR — Internal Rate of Return

**DuckDB signature:** `irr(values_csv)`

Uses bisection across 13 bracket candidates. Returns `NULL` if no bracket with a sign change is found.

```sql
-- Excel examples:
-- Investment's internal rate of return after five years
SELECT ROUND(irr('-70000,12000,15000,18000,21000,26000'), 4) AS irr_result;
-- Result: 0.0866

-- Investment's internal rate of return after four years
SELECT ROUND(irr('-70000,12000,15000,18000,21000'), 4) AS irr_result;
-- Result: -0.0212

-- Investment's internal rate of return after two years
SELECT ROUND(irr('-70000,12000,15000'), 4) AS irr_result;
-- Result: -0.4435
```

## MIRR — Modified Internal Rate of Return

**DuckDB signature:** `mirr(values_csv, finance_rate, reinvest_rate)`

An improvement on IRR that uses separate rates for financing and reinvestment. Computed via closed-form formula — no iteration required.

```sql
-- Excel example: MIRR(-120000, 39000, 30000, 21000, 37000, 46000, 10%, 12%) = 12.61%
SELECT ROUND(mirr('-120000,39000,30000,21000,37000,46000', 0.10, 0.12), 4) AS mirr_result;
-- Result: 0.1261
```

## XNPV — Net Present Value with Irregular Dates

**DuckDB signature:** `xnpv(rate, values_csv, dates_csv)`

**Formula:** $XNPV = \sum_{i=0}^{n} \frac{CF_i}{(1+r)^{(d_i - d_0)/365}}$

```sql
-- Excel example: XNPV(9%, cash flows, dates) = 2086.65
SELECT ROUND(xnpv(
    0.09,
    '-10000,2750,4250,3250,2750',
    '2008-01-01,2008-03-01,2008-10-30,2009-02-15,2009-04-01'
), 2) AS xnpv_result;
-- Result: 2086.65
```

## XIRR — Internal Rate of Return with Irregular Dates

**DuckDB signature:** `xirr(values_csv, dates_csv)`

```sql
-- Excel example: XIRR(cash flows, dates) = 37.34%
SELECT ROUND(xirr(
    '-10000,2750,4250,3250,2750',
    '2008-01-01,2008-03-01,2008-10-30,2009-02-15,2009-04-01'
), 4) AS xirr_result;
-- Result: 0.3734
```

---

# Depreciation Functions

## SLN — Straight-Line Depreciation

**DuckDB signature:** `sln(cost, salvage, life)`

Equal depreciation each period.

```sql
-- Excel example: SLN(30000, 7500, 10) = 2250
SELECT sln(30000, 7500, 10) AS sln_result;
-- Result: 2250.00
```

## SYD — Sum-of-Years-Digits Depreciation

**DuckDB signature:** `syd(cost, salvage, life, per)`

Accelerated depreciation — higher in early years.

```sql
-- Excel example: SYD(30000, 7500, 10, 1) = 4090.91
SELECT ROUND(syd(30000, 7500, 10, 1), 2) AS syd_year1;
-- Result: 4090.91

-- Compare depreciation in year 1 vs year 10
SELECT
    ROUND(syd(30000, 7500, 10, 1), 2)  AS year_1,
    ROUND(syd(30000, 7500, 10, 10), 2) AS year_10;
```

## DB — Fixed-Declining Balance Depreciation

**DuckDB signature:** `db(cost, salvage, life, per, month)`

`month` = number of months in first year (default 12).

```sql
-- Excel example: DB(1000000, 100000, 6, 1, 7) = 186083.33
SELECT ROUND(db(1000000, 100000, 6, 1, 7), 2) AS db_result;
-- Result: 186083.33
```

## DDB — Double-Declining Balance Depreciation

**DuckDB signature:** `ddb(cost, salvage, life, per, factor)`

`factor` = 2.0 for double-declining (Excel default).

```sql
-- Excel example: DDB(2400, 300, 10, 1, 2) = 480
SELECT ddb(2400, 300, 10, 1, 2) AS ddb_result;
-- Result: 480.00
```

## VDB — Variable Declining Balance

**DuckDB signature:** `vdb(cost, salvage, life, start_per, end_per, factor, no_switch)`

Depreciation between any two fractional periods. `no_switch = 0` means switch to straight-line when SL > DDB (Excel default).

::: {.callout-warning}

### Input Types

All seven parameters must be `DOUBLE`. Use decimal points (e.g. `2400.0` not `2400`) or explicit casts (`2400::DOUBLE`) to avoid a type mismatch error.
:::

```sql
-- First day's depreciation.
SELECT ROUND(vdb(2400.0, 300.0, 10*365, 0.0, 1.0, 2.0, 0.0), 2) AS first_day_dep;
-- Result: 1.32

-- First month's depreciation.
SELECT ROUND(vdb(2400.0, 300.0, 10.0*12, 0.0, 1.0, 2.0, 0.0), 2) AS first_month_dep;
-- Result: 40.00

-- First year depreciation
SELECT ROUND(vdb(2400.0, 300.0, 10.0, 0.0, 1.0, 2.0, 0.0), 2) AS year_1_dep;
-- Result: 480.00

-- Depreciation between the sixth month and the eighteenth month.
SELECT ROUND(vdb(2400.0, 300.0, 10.0*12, 6.0, 18.0, 2.0, 0.0), 2) AS month_6_18_dep;
-- Result: 396.31
```

## AMORLINC — French Straight-Line Depreciation

**DuckDB signature:** `amorlinc(cost, date_purchased, first_period, salvage, period, rate, basis)`

French accounting straight-line depreciation, prorated for the first period.

```sql
-- Excel example: AMORLINC(2400, 39679, 39813, 300, 1, 0.15, 1)
-- WHERE 39679 = 8/19/2008 and 39813 = 12/31/2008 (Excel date serial representation)
SELECT ROUND(amorlinc(2400, 39679, 39813, 300, 1, 0.15, 1), 2) AS amorlinc_result;
-- Result: 360.00
```

## AMORDEGRC — French Degressive Depreciation

### This Function is Deprecated

::: {.callout-warning}

### Important: - Note

Because of changes to the depreciation methodology of the French accounting system, this function is deprecated and should no longer be used. It is included for compatibility of old workbooks only.<br>
[Excel Support Reference](https://support.microsoft.com/en-us/office/amordegrc-function-a14d0ca1-64a4-42eb-9b3d-b0dededf9e51){target="\_blank"}
:::

**DuckDB signature:** `amordegrc(cost, date_purchased, first_period, salvage, period, rate, basis)`

Applies an Excel-specified coefficient to the depreciation rate based on asset life. Switches to straight-line in the final two periods. The coefficient table is:

| Asset life | Coefficient            |
| ---------- | ---------------------- |
| 3–4 years  | 1.5                    |
| 5–6 years  | 2.0                    |
| 6+ years   | 2.5                    |
| < 3 years  | Returns NULL (invalid) |

```sql
-- Excel example: AMORDEGRC(2400, 39679, 39813, 300, 1, 0.15, 1) = 776
SELECT ROUND(amordegrc(2400, 39679, 39813, 300, 1, 0.15, 1), 2) AS amordegrc_result;
-- Result: 776.00
```

---

# Coupon Date Functions

All coupon functions take: `settlement (VARCHAR)`, `maturity (VARCHAR)`, `frequency (DOUBLE: 1/2/4)`, `basis (DOUBLE: 0–4)`. Dates are passed as `'YYYY-MM-DD'` strings.

## COUPDAYBS — Days from Coupon Start to Settlement

```sql
-- Excel example: COUPDAYBS(2011-01-25, 2011-11-15, 2, 1) = 71
SELECT coupdaybs('2011-01-25', '2011-11-15', 2, 1) AS coupdaybs_result;
-- Result: 71
```

## COUPDAYS — Days in Coupon Period

```sql
-- Excel example: COUPDAYS(2011-01-25, 2011-11-15, 2, 1) = 181
SELECT coupdays('2011-01-25', '2011-11-15', 2, 1) AS coupdays_result;
-- Result: 181
```

## COUPDAYSNC — Days from Settlement to Next Coupon

```sql
-- Excel example: COUPDAYSNC(2011-01-25, 2011-11-15, 2, 1) = 110
SELECT coupdaysnc('2011-01-25', '2011-11-15', 2, 1) AS coupdaysnc_result;
-- Result: 110
```

## COUPNCD — Next Coupon Date

Returns an Excel serial date number (days since 1899-12-30).

```sql
-- Excel example: next coupon = 2011-05-15
SELECT coupncd('2011-01-25', '2011-11-15', 2, 1) AS coupncd_serial;
-- Result: 40678  (Excel serial for 2011-05-15)
```

## COUPPCD — Previous Coupon Date

```sql
-- Excel example: previous coupon = 2010-11-15
SELECT couppcd('2011-01-25', '2011-11-15', 2, 1) AS couppcd_serial;
-- Result: 40497  (Excel serial for 2010-11-15)
```

## COUPNUM — Number of Coupon Payments

```sql
-- Excel example: COUPNUM(2007-01-25, 2008-11-15, 2, 1) = 4
SELECT coupnum('2007-01-25', '2008-11-15', 2, 1) AS coupnum_result;
-- Result: 4
```

---

# Bond & Security Functions

All bond functions accept date parameters as `'YYYY-MM-DD'` VARCHAR strings. Frequency codes: 1 = annual, 2 = semi-annual, 4 = quarterly.

## PRICE — Bond Price

**DuckDB signature:** `price(settlement, maturity, rate, yld, redemption, frequency, basis)`

Price per $100 face value of a periodic-coupon bond. Returns the **clean price** (dirty price minus accrued interest), matching Excel.

```sql
-- Excel example: PRICE(2008-02-15, 2017-11-15, 5.75%, 6.5%, 100, 2, 0) = 94.63
SELECT ROUND(price('2008-02-15', '2017-11-15', 0.0575, 0.065, 100, 2, 0), 2) AS price_result;
-- Result: 94.63
```

## PRICEDISC — Price of Discounted Security

**DuckDB signature:** `pricedisc(settlement, maturity, discount, redemption, basis)`

```sql
-- Excel example: PRICEDISC(2008-02-16, 2008-03-01, 5.25%, 100, 2) = 99.795
SELECT ROUND(pricedisc('2008-02-16', '2008-03-01', 0.0525, 100, 2), 3) AS pricedisc_result;
-- Result: 99.795
```

## PRICEMAT — Price at Maturity

**DuckDB signature:** `pricemat(settlement, maturity, issue, rate, yld, basis)`

Price of a security that pays interest at maturity.

```sql
-- Excel example: PRICEMAT(2008-02-15, 2008-04-13, 2007-11-11, 6.1%, 6.1%, 0) = 99.98
SELECT ROUND(pricemat('2008-02-15', '2008-04-13', '2007-11-11', 0.061, 0.061, 0), 2) AS pricemat_result;
-- Result: 99.98
```

## YIELD — Bond Yield

**DuckDB signature:** `yield(settlement, maturity, rate, pr, redemption, frequency, basis)`

Inverts PRICE via Newton-Raphson iteration.

```sql
-- Excel example: YIELD(2008-02-15, 2016-11-15, 5.75%, 95.04287, 100, 2, 0) = 6.5%
SELECT ROUND(yield('2008-02-15', '2016-11-15', 0.0575, 95.04287, 100, 2, 0), 4) AS yield_result;
-- Result: 0.0650
```

## YIELDDISC — Yield of Discounted Security

**DuckDB signature:** `yielddisc(settlement, maturity, pr, redemption, basis)`

```sql
-- Excel example: YIELDDISC(2008-02-16, 2008-03-01, 99.795, 100, 2) = 5.25%
SELECT ROUND(yielddisc('2008-02-16', '2008-03-01', 99.795, 100, 2), 4) AS yielddisc_result;
-- Result: 0.0525
```

## YIELDMAT — Yield at Maturity

**DuckDB signature:** `yieldmat(settlement, maturity, issue, rate, pr, basis)`

```sql
-- Excel example: YIELDMAT(2008-03-15, 2008-11-03, 2007-11-08, 6.25%, 100.0123, 0) = 6.10%
SELECT ROUND(yieldmat('2008-03-15', '2008-11-03', '2007-11-08', 0.0625, 100.0123, 0), 4) AS yieldmat_result;
-- Result: 0.0610
```

## DISC — Discount Rate

**DuckDB signature:** `disc(settlement, maturity, pr, redemption, basis)`

```sql
-- Excel example: DISC(2008-02-16, 2008-03-01, 97.975, 100, 2)
SELECT ROUND(disc('2008-02-16', '2008-03-01', 97.975, 100, 2), 4) AS disc_result;
```

## INTRATE — Interest Rate for Fully Invested Security

**DuckDB signature:** `intrate(settlement, maturity, investment, redemption, basis)`

```sql
-- Excel example: INTRATE(2008-02-15, 2008-05-15, 1000000, 1014420, 2) = 5.768%
SELECT ROUND(intrate('2008-02-15', '2008-05-15', 1000000, 1014420, 2), 5) AS intrate_result;
-- Result: 0.05768
```

## RECEIVED — Amount Received at Maturity

**DuckDB signature:** `received(settlement, maturity, investment, discount, basis)`

```sql
-- Excel example: RECEIVED(2008-02-15, 2008-05-15, 1000000, 5.75%, 2) = 1014584.65
SELECT ROUND(received('2008-02-15', '2008-05-15', 1000000, 0.0575, 2), 2) AS received_result;
-- Result: 1014584.65
```

## DURATION — Macaulay Duration

**DuckDB signature:** `duration(settlement, maturity, coupon, yld, frequency, basis)`

Weighted average time to receive the bond's cash flows. Result is in years.

```sql
-- Excel example: DURATION(2008-01-01, 2016-01-01, 8%, 9%, 2, 1) = 5.9938
SELECT ROUND(duration('2008-01-01', '2016-01-01', 0.08, 0.09, 2, 1), 4) AS duration_result;
-- Result: 5.9938
```

## MDURATION — Modified Duration

**DuckDB signature:** `mduration(settlement, maturity, coupon, yld, frequency, basis)`

$MDURATION = \frac{Duration}{1 + yield/frequency}$. Measures price sensitivity to yield changes.

```sql
-- Excel example: MDURATION(2008-01-01, 2016-01-01, 8%, 9%, 2, 1) = 5.7355
SELECT ROUND(mduration('2008-01-01', '2016-01-01', 0.08, 0.09, 2, 1), 4) AS mduration_result;
-- Result: 5.7355
```

## ACCRINT — Accrued Interest (Periodic Coupon)

**DuckDB signature:** `accrint(issue, first_interest, settlement, rate, par, frequency, basis)`

```sql
-- Excel example: ACCRINT(2008-03-01, 2008-08-31, 2008-05-01, 10%, 1000, 2, 0) = 16.94
SELECT ROUND(accrint('2008-03-01', '2008-08-31', '2008-05-01', 0.10, 1000, 2, 0), 2) AS accrint_result;
-- Result: 16.94
```

## ACCRINTM — Accrued Interest (Maturity)

**DuckDB signature:** `accrintm(issue, settlement, rate, par, basis)`

```sql
-- Excel example: ACCRINTM(2008-04-01, 2008-06-15, 10%, 1000, 3) = 20.54
SELECT ROUND(accrintm('2008-04-01', '2008-06-15', 0.10, 1000, 3), 2) AS accrintm_result;
-- Result: 20.54
```

---

# Miscellaneous Financial Functions

## EFFECT — Effective Annual Rate

**DuckDB signature:** `effect(nominal_rate, npery)`

Converts a nominal rate to effective annual rate accounting for compounding.

```sql
-- Excel example: EFFECT(5.25%, 4) = 5.3543%
SELECT ROUND(effect(0.0525, 4), 6) AS effect_result;
-- Result: 0.053543
```

## NOMINAL — Nominal Annual Rate

**DuckDB signature:** `nominal(effect_rate, npery)`

Inverse of EFFECT.

```sql
-- Excel example: NOMINAL(5.3543%, 4) = 5.25%
SELECT ROUND(nominal(0.053543, 4), 4) AS nominal_result;
-- Result: 0.0525
```

## DOLLARDE — Dollar Price to Decimal

**DuckDB signature:** `dollarde(fractional_dollar, fraction)`

Converts a price expressed as an integer plus a fraction (e.g. 1.02 meaning 1 + 2/16) to a decimal.

```sql
-- Excel example: DOLLARDE(1.02, 16) = 1.125  (1 + 2/16)
SELECT dollarde(1.02, 16) AS dollarde_result;
-- Result: 1.125

-- Excel Example: DOLLARDE(1.1, 32) = 1.3125  (1 + 10/32)
SELECT dollarde(1.1, 32) AS dollarde_result;
```

## DOLLARFR — Dollar Price to Fraction

**DuckDB signature:** `dollarfr(decimal_dollar, fraction)`

Inverse of DOLLARDE.

```sql
-- Excel example: DOLLARFR(1.125, 16) = 1.02
SELECT dollarfr(1.125, 16) AS dollarfr_result;
-- Result: 1.02

-- Excel example: DOLLARFR(1.125, 32) = 1.04
SELECT dollarfr(1.125, 32) AS dollarfr_result;
-- Result: 1.04
```

## FVSCHEDULE — Future Value with Variable Rates

**DuckDB signature:** `fvschedule(principal, csv_rates)`

Applies a schedule of variable compound interest rates to a principal amount.

```sql
-- Excel example: FVSCHEDULE(1, 9%, 11%, 10%) = 1.3309
SELECT ROUND(fvschedule(1, '0.09,0.11,0.10'), 4) AS fvschedule_result;
-- Result: 1.3309
```

## RRI — Equivalent Interest Rate

**DuckDB signature:** `rri(nper, pv, fv)`

Returns the equivalent periodic interest rate for an investment growing from `pv` to `fv` over `nper` periods.

```sql
-- Excel example: RRI(96, 10000, 11000) = 0.0988% per period
SELECT ROUND(rri(96, 10000, 11000), 6) AS rri_result;
-- Result: 0.000988
```

## PDURATION — Periods to Reach a Value

**DuckDB signature:** `pduration(rate, pv, fv)`

Returns the number of periods required for an investment to reach a target value.

```sql
-- Excel example: PDURATION(2.5%, 2000, 2200) = 3.86 periods
SELECT ROUND(pduration(0.025, 2000, 2200), 4) AS pduration_result;
-- Result:  3.8599

-- Excel example: PDURATION(2.5%/12, 1000, 1200) = 87.6 periods
SELECT ROUND(pduration(0.025/12, 1000, 1200), 4) AS pduration_result;
-- Result:   87.6055
```

## TBILLPRICE — Treasury Bill Price

**DuckDB signature:** `tbillprice(settlement, maturity, discount)`

Price per $100 face value for a Treasury bill.

```sql
-- Excel example: TBILLPRICE(2008-03-31, 2008-06-01, 9.0%) = 98.45
SELECT ROUND(tbillprice('2008-03-31', '2008-06-01', 0.09), 2) AS tbillprice_result;
-- Result: 98.45
```

## TBILLYIELD — Treasury Bill Yield

**DuckDB signature:** `tbillyield(settlement, maturity, pr)`

```sql
-- Excel example: TBILLYIELD(2008-03-31, 2008-06-01, 98.45) = 9.14%
SELECT ROUND(tbillyield('2008-03-31', '2008-06-01', 98.45), 4) AS tbillyield_result;
-- Result: 0.0914
```

## TBILLEQ — Treasury Bill Bond-Equivalent Yield

**DuckDB signature:** `tbilleq(settlement, maturity, discount)`

Converts a T-bill discount rate to a bond-equivalent yield. Uses two formulas depending on the number of days to maturity:

- **≤ 182 days:** $\frac{365 \times discount}{360 - discount \times DSM}$
- **> 182 days:** Quadratic solution to account for reinvestment compounding

```sql
-- Excel example: TBILLEQ(2008-03-31, 2008-06-01, 9.14%) = 9.42%
SELECT ROUND(tbilleq('2008-03-31', '2008-06-01', 0.0914), 4) AS tbilleq_result;
-- Result: 0.0942
```

## ODDFPRICE — Odd First Period Bond Price

**DuckDB signature:** `oddfprice(settlement, maturity, issue, first_coupon, rate, yld, redemption, frequency, basis)`

Price of a bond with an irregular (short or long) first coupon period.

```sql
-- Excel example: ODDFPRICE(2008-11-11, 2021-03-01, 2008-10-15, 2009-03-01, 7.85%, 6.25%, 100, 2, 1)
SELECT ROUND(oddfprice(
    '2008-11-11', '2021-03-01', '2008-10-15', '2009-03-01',
    0.0785, 0.0625, 100, 2, 1
), 2) AS oddfprice_result;
-- Result: 113.60
```

## ODDFYIELD — Odd First Period Bond Yield

**DuckDB signature:** `oddfyield(settlement, maturity, issue, first_coupon, rate, pr, redemption, frequency, basis)`

Inverts ODDFPRICE via bisection.

```sql
-- Excel example: ODDFYIELD(2008-11-11, 2021-03-01, 2008-10-15, 2009-03-01, 5.75%, 84.50, 100, 2, 1) = 7.72%
SELECT ROUND(oddfyield(
    '2008-11-11', '2021-03-01', '2008-10-15', '2009-03-01',
    0.0575, 84.50, 100, 2, 1
), 4) AS oddfyield_result;
-- Result: 0.0772
```

## ODDLPRICE — Odd Last Period Bond Price

**DuckDB signature:** `oddlprice(settlement, maturity, last_interest, rate, yld, redemption, frequency, basis)`

Price of a bond with an irregular last coupon period.

```sql
-- Excel example: ODDLPRICE(2008-02-07, 2008-06-15, 2007-10-15, 3.75%, 4.05%, 100, 2, 0) = 99.88
SELECT ROUND(oddlprice(
    '2008-02-07', '2008-06-15', '2007-10-15',
    0.0375, 0.0405, 100, 2, 0
), 2) AS oddlprice_result;
-- Result: 99.88
```

## ODDLYIELD — Odd Last Period Bond Yield

**DuckDB signature:** `oddlyield(settlement, maturity, last_interest, rate, pr, redemption, frequency, basis)`

```sql
-- Excel example: ODDLYIELD(2008-04-20, 2008-06-15, 2007-12-24, 3.75%, 99.875, 100, 2, 0) = 4.52%
SELECT ROUND(oddlyield(
    '2008-04-20', '2008-06-15', '2007-12-24',
    0.0375, 99.875, 100, 2, 0
), 4) AS oddlyield_result;
-- Result: 0.0452
```

---

# Extension Entry Point

All 55 functions are registered in a single entry point:

```rust
#[duckdb_entrypoint_c_api]
pub unsafe fn extension_entrypoint(con: Connection) -> Result<(), Box<dyn Error>> {
    // Annuity (10)
    con.register_scalar_function::<FvFunction>("fv")?;
    con.register_scalar_function::<PvFunction>("pv")?;
    con.register_scalar_function::<PmtFunction>("pmt")?;
    con.register_scalar_function::<IpmtFunction>("ipmt")?;
    con.register_scalar_function::<PpmtFunction>("ppmt")?;
    con.register_scalar_function::<CumipmtFunction>("cumipmt")?;
    con.register_scalar_function::<CumprincFunction>("cumprinc")?;
    con.register_scalar_function::<NperFunction>("nper")?;
    con.register_scalar_function::<RateFunction>("rate")?;
    con.register_scalar_function::<IspmtFunction>("ispmt")?;
    // Cash flows (5)
    con.register_scalar_function::<NpvFunction>("npv")?;
    con.register_scalar_function::<IrrFunction>("irr")?;
    con.register_scalar_function::<MirrFunction>("mirr")?;
    con.register_scalar_function::<XnpvFunction>("xnpv")?;
    con.register_scalar_function::<XirrFunction>("xirr")?;
    // Depreciation (7)
    con.register_scalar_function::<SlnFunction>("sln")?;
    con.register_scalar_function::<SydFunction>("syd")?;
    con.register_scalar_function::<DbFunction>("db")?;
    con.register_scalar_function::<DdbFunction>("ddb")?;
    con.register_scalar_function::<VdbFunction>("vdb")?;
    con.register_scalar_function::<AmorlincFunction>("amorlinc")?;
    con.register_scalar_function::<AmordegrcFunction>("amordegrc")?;
    // Coupon dates (6)
    con.register_scalar_function::<CoupdaybsFunction>("coupdaybs")?;
    con.register_scalar_function::<CoupdaysFunction>("coupdays")?;
    con.register_scalar_function::<CoupdayssncFunction>("coupdaysnc")?;
    con.register_scalar_function::<CoupncdFunction>("coupncd")?;
    con.register_scalar_function::<CouppcdFunction>("couppcd")?;
    con.register_scalar_function::<CoupnumFunction>("coupnum")?;
    // Bonds (13)
    con.register_scalar_function::<PriceFunction>("price")?;
    con.register_scalar_function::<PricediscFunction>("pricedisc")?;
    con.register_scalar_function::<PricematFunction>("pricemat")?;
    con.register_scalar_function::<YieldFunction>("yield")?;
    con.register_scalar_function::<YielddiscFunction>("yielddisc")?;
    con.register_scalar_function::<YieldmatFunction>("yieldmat")?;
    con.register_scalar_function::<DiscFunction>("disc")?;
    con.register_scalar_function::<IntrateFunction>("intrate")?;
    con.register_scalar_function::<ReceivedFunction>("received")?;
    con.register_scalar_function::<DurationFunction>("duration")?;
    con.register_scalar_function::<MdurationFunction>("mduration")?;
    con.register_scalar_function::<AccrintFunction>("accrint")?;
    con.register_scalar_function::<AccrintmFunction>("accrintm")?;
    // Misc (14)
    con.register_scalar_function::<EffectFunction>("effect")?;
    con.register_scalar_function::<NominalFunction>("nominal")?;
    con.register_scalar_function::<DollardeFunction>("dollarde")?;
    con.register_scalar_function::<DollarfrFunction>("dollarfr")?;
    con.register_scalar_function::<FvscheduleFunction>("fvschedule")?;
    con.register_scalar_function::<RriFunction>("rri")?;
    con.register_scalar_function::<PdurationFunction>("pduration")?;
    con.register_scalar_function::<TbillpriceFunction>("tbillprice")?;
    con.register_scalar_function::<TbillyieldFunction>("tbillyield")?;
    con.register_scalar_function::<TbilleqFunction>("tbilleq")?;
    con.register_scalar_function::<OddfpriceFunction>("oddfprice")?;
    con.register_scalar_function::<OddfyieldFunction>("oddfyield")?;
    con.register_scalar_function::<OddlpriceFunction>("oddlprice")?;
    con.register_scalar_function::<OddlyieldFunction>("oddlyield")?;
    Ok(())
}
```

::: {.callout-note}
The function name `extension_entrypoint` does not matter — the macro renames the exported symbol based on your **crate name**. For a crate named `fin_functions`, the exported symbol will be `fin_functions_init_c_api`.
:::

---

# Loading and Using the Extension

```sql
-- Launch DuckDB with --unsigned flag (required for unsigned extensions)
-- duckdb --unsigned

LOAD '<your Extension path>/fin_functions.duckdb_extension';
-- LOAD 'C:/Users/storl/Desktop/Financial_Functions/fin_functions.duckdb_extension';

-- Verify all 55 functions loaded
SELECT COUNT(*) AS loaded
FROM duckdb_functions()
WHERE function_name IN (
    'fv','pv','pmt','ipmt','ppmt','cumipmt','cumprinc','nper','rate','ispmt',
    'npv','irr','mirr','xnpv','xirr',
    'sln','syd','db','ddb','vdb','amorlinc','amordegrc',
    'coupdaybs','coupdays','coupdaysnc','coupncd','couppcd','coupnum',
    'price','pricedisc','pricemat','yield','yielddisc','yieldmat',
    'disc','intrate','received','duration','mduration','accrint','accrintm',
    'effect','nominal','dollarde','dollarfr','fvschedule','rri','pduration',
    'tbillprice','tbillyield','tbilleq',
    'oddfprice','oddfyield','oddlprice','oddlyield'
);
-- Expected: 55
```

---

# Key Lessons

1. **`VScalar` trait** — implement `State`, `signatures()`, and `invoke()`. Use `type State = ()` for stateless functions.

2. **VARCHAR reading** — there is no `get_string()`. Read the raw `duckdb_string_t` struct using `libduckdb_sys` helpers. The `Datelike` trait must be explicitly imported for `.year()`, `.month()`, `.day()` to work on `NaiveDate`.

3. **Sign convention** — all payment functions (PMT, IPMT, PPMT) return **negative values** for cash outflows, exactly matching Excel.

4. **Rust 2024 edition** — explicit `unsafe {}` blocks are required inside `unsafe fn`. This catches unsafe calls that were previously implicit.

5. **Version pinning** — `duckdb`, `duckdb-loadable-macros`, and `libduckdb-sys` must all be on the **same version string**. The version encodes the DuckDB release: `1.10501.0` = DuckDB v1.5.1.

6. **Day-count conventions** — bond and coupon functions share a central `year_frac(d1, d2, basis)` helper. Centralising it prevents subtle per-function inconsistencies.

7. **Date inputs as VARCHAR** — all date parameters use `'YYYY-MM-DD'` strings, consistent with the XIRR/XNPV pattern already established in the base project.

8. **Iterative solvers** — RATE and YIELD use Newton-Raphson; IRR, XIRR, and odd-period yield functions use bisection. Always handle `NULL` returns gracefully in queries — they signal non-convergence or invalid input rather than a crash.

9. **`write_f64` everywhere** — never write a raw `f64` to DuckDB output. Route all results through `write_f64` so that `NaN` and `Infinity` become `NULL` instead of crashing the query.

10. **`div_euclid` for month arithmetic** — Rust's `/` truncates toward zero for negative numbers. Month subtraction requires floor division. Always use `div_euclid` / `rem_euclid` in `add_months` or coupon functions will hang indefinitely.

11. **`Result` and `Option` for parsing** — use `parse_csv_f64` → `Result` and `parse_date` → `Option` so that bad input writes `NULL` rather than substituting a silent zero or a fallback date.

12. **`catch_unwind` is insufficient** — some third-party crates panic in a way marked `cannot unwind`, which bypasses `std::panic::catch_unwind`. The only safe fix is to replace those functions with your own implementation that cannot panic.

13. **Always `cargo clean`** — when a fix does not appear to take effect, run `cargo clean` before rebuilding.

14. **Deploying the extension** — only the single `.duckdb_extension` file is needed on the target machine. It must match the DuckDB version and platform (e.g. `windows_amd64`), and DuckDB must be launched with `--unsigned`.

---

_Based on the code in this project's source files._

<b>Resource Consulted:</b> <i>Claude Sonnet 4.6 (Anthropic's AI assistant, April 2026), which provided detailed explanations of this project's code and concepts.</i>
