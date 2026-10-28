use primitive_types::U256;

pub fn mul_rsh(val: U256, mul_by: u128, shift_by: usize) -> U256 {
    let mul_res = val * U256::from(mul_by);

    mul_res >> shift_by
}
