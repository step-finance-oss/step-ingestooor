mod common;
pub mod parser;
mod rate;
mod rate_limiter;
mod solend_decimal;
mod state;

pub use common::*;
pub use rate::*;
pub use rate_limiter::*;
pub use solend_decimal::*;

//helpers
fn pack_solend_decimal(solend_decimal: SolendDecimal, dst: &mut [u8; 16]) {
    *dst = solend_decimal
        .to_scaled_val()
        .expect("SolendDecimal cannot be packed")
        .to_le_bytes();
}

fn unpack_solend_decimal(src: &[u8; 16]) -> SolendDecimal {
    SolendDecimal::from_scaled_val(u128::from_le_bytes(*src))
}
fn unpack_bool(src: &[u8; 1]) -> anyhow::Result<bool> {
    match u8::from_le_bytes(*src) {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(anyhow::anyhow!("Boolean cannot be unpacked")),
    }
}
