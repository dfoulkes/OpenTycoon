//! A portable state hash for determinism checks.
//!
//! `std`'s `DefaultHasher` is randomly keyed and its integer writes are native-endian, so it is
//! unfit for comparing state across runs or machines. [`StableHasher`] is FNV-1a 64 with every
//! integer written little-endian and `usize` widened to `u64`.

use std::hash::{Hash, Hasher};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64, endian- and pointer-width-independent.
#[derive(Debug, Clone)]
pub struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        StableHasher(FNV_OFFSET)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
    fn write_u8(&mut self, i: u8) {
        self.write(&[i]);
    }
    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }
    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }
    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }
    fn write_u128(&mut self, i: u128) {
        self.write(&i.to_le_bytes());
    }
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    fn write_i8(&mut self, i: i8) {
        self.write(&i.to_le_bytes());
    }
    fn write_i16(&mut self, i: i16) {
        self.write(&i.to_le_bytes());
    }
    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }
    fn write_i64(&mut self, i: i64) {
        self.write(&i.to_le_bytes());
    }
    fn write_i128(&mut self, i: i128) {
        self.write(&i.to_le_bytes());
    }
    fn write_isize(&mut self, i: isize) {
        self.write_i64(i as i64);
    }
}

/// Hashes any `Hash` value with [`StableHasher`].
pub fn stable_hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut h = StableHasher::default();
    value.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // FNV-1a 64 of "a" is 0xaf63dc4c8601ec8c.
        let mut h = StableHasher::default();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn integers_hash_little_endian() {
        let mut a = StableHasher::default();
        a.write_u32(1);
        let mut b = StableHasher::default();
        b.write(&[1, 0, 0, 0]);
        assert_eq!(a.finish(), b.finish());
    }
}
