//! Latin hypercube sampling (R64), deterministic from the study seed.

use super::def::Parameter;

/// `count` points in `[0, 1)^dims`: for each dimension, one point in each of `count` equal
/// strata. `[point][dim]`.
pub fn unit_samples(dims: usize, count: usize, seed: u64) -> Vec<Vec<f64>> {
    let _ = (dims, count, seed);
    todo!("lhs")
}

/// The value of `p` at unit sample `u`, rounded as the parameter asks.
pub fn value_at(p: &Parameter, u: f64) -> f64 {
    let _ = (p, u);
    todo!("value_at")
}
