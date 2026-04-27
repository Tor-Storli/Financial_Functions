// fin_functions — DuckDB extension implementing all 54 Excel financial functions
//
// Module layout:
//   helpers      — shared utilities: varchar reading, date parsing, day-count conventions
//   annuity      — FV, PV, PMT, IPMT, PPMT, CUMIPMT, CUMPRINC, NPER, RATE, ISPMT
//   cash_flows   — NPV, IRR, MIRR, XNPV, XIRR
//   depreciation — SLN, SYD, DB, DDB, VDB, AMORDEGRC, AMORLINC
//   coupons      — COUPDAYBS, COUPDAYS, COUPDAYSNC, COUPNCD, COUPNUM, COUPPCD
//   bonds        — PRICE, PRICEDISC, PRICEMAT, YIELD, YIELDDISC, YIELDMAT,
//                  DISC, INTRATE, RECEIVED, DURATION, MDURATION, ACCRINT, ACCRINTM
//   misc         — EFFECT, NOMINAL, DOLLARDE, DOLLARFR, FVSCHEDULE, RRI, PDURATION,
//                  TBILLEQ, TBILLPRICE, TBILLYIELD,
//                  ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD

mod helpers;
mod annuity;
mod cash_flows;
mod depreciation;
mod coupons;
mod bonds;
mod misc;

use duckdb::Connection;
use duckdb_loadable_macros::duckdb_entrypoint_c_api;
use std::error::Error;

// Re-export all function structs for registration
use annuity::*;
use cash_flows::*;
use depreciation::*;
use coupons::*;
use bonds::*;
use misc::*;

#[duckdb_entrypoint_c_api]
pub unsafe fn extension_entrypoint(con: Connection) -> Result<(), Box<dyn Error>> {

    // ── Annuity (10) ────────────────────────────────────────────────────────
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

    // ── Cash flows (5) ──────────────────────────────────────────────────────
    con.register_scalar_function::<NpvFunction>("npv")?;
    con.register_scalar_function::<IrrFunction>("irr")?;
    con.register_scalar_function::<MirrFunction>("mirr")?;
    con.register_scalar_function::<XnpvFunction>("xnpv")?;
    con.register_scalar_function::<XirrFunction>("xirr")?;

    // ── Depreciation (7) ────────────────────────────────────────────────────
    con.register_scalar_function::<SlnFunction>("sln")?;
    con.register_scalar_function::<SydFunction>("syd")?;
    con.register_scalar_function::<DbFunction>("db")?;
    con.register_scalar_function::<DdbFunction>("ddb")?;
    con.register_scalar_function::<VdbFunction>("vdb")?;
    con.register_scalar_function::<AmorlincFunction>("amorlinc")?;
    con.register_scalar_function::<AmordegrcFunction>("amordegrc")?;

    // ── Coupon dates (6) ────────────────────────────────────────────────────
    con.register_scalar_function::<CoupdaybsFunction>("coupdaybs")?;
    con.register_scalar_function::<CoupdaysFunction>("coupdays")?;
    con.register_scalar_function::<CoupdayssncFunction>("coupdaysnc")?;
    con.register_scalar_function::<CoupncdFunction>("coupncd")?;
    con.register_scalar_function::<CouppcdFunction>("couppcd")?;
    con.register_scalar_function::<CoupnumFunction>("coupnum")?;

    // ── Bonds & securities (13) ─────────────────────────────────────────────
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

    // ── Misc (14) ───────────────────────────────────────────────────────────
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
