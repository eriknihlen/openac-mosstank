//! A quick 64-bit hash for telling whether what a file would be written
//! from has changed: thousands of loot rules hash in a few instructions a
//! byte, where the standard library's keyed hash takes several times that.
//! Not for maps that take untrusted keys.

use std::hash::{Hash, Hasher};

const MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

/// The hasher: eight bytes at a time, each folded in with a multiply.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FastHasher {
    state: u64,
}

impl FastHasher {
    fn fold(&mut self, word: u64) {
        self.state = (self.state.rotate_left(5) ^ word).wrapping_mul(MULTIPLIER);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            self.fold(u64::from_le_bytes(word));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.fold(u64::from_le_bytes(word) ^ ((rest.len() as u64) << 59));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.fold(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.fold(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.fold(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.fold(value as u64);
    }

    fn finish(&self) -> u64 {
        // A last mix so near inputs land far apart.
        let mut state = self.state;
        state ^= state >> 33;
        state = state.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        state ^ (state >> 33)
    }
}

/// `value`'s quick hash.
pub(crate) fn hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = FastHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}
