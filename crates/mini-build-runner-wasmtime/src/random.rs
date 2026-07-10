//! A deterministic random source for `random:deterministic` steps,
//! seeded from `ExecutionRequest::deterministic_seed` (itself derived by
//! the caller from the execution plan's own digest) -- never OS entropy,
//! so a step declaring only this capability stays byte-for-byte
//! reproducible across independent runners (exit criterion 12).
//!
//! Built on BLAKE3's keyed mode as a counter-based stream construction:
//! composing an already-reviewed primitive (`mini-crypto`'s hash
//! function, here used directly since this crate has no `mini-crypto`
//! dependency) rather than inventing a new PRNG design.

use rand_core::{Error, RngCore};

pub struct DeterministicRng {
    key: [u8; 32],
    counter: u64,
}

impl DeterministicRng {
    pub fn new(seed: [u8; 32]) -> Self {
        DeterministicRng {
            key: seed,
            counter: 0,
        }
    }

    fn next_block(&mut self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.key);
        hasher.update(&self.counter.to_le_bytes());
        self.counter += 1;
        *hasher.finalize().as_bytes()
    }
}

impl RngCore for DeterministicRng {
    fn next_u32(&mut self) -> u32 {
        let block = self.next_block();
        u32::from_le_bytes(block[0..4].try_into().expect("4 bytes"))
    }

    fn next_u64(&mut self) -> u64 {
        let block = self.next_block();
        u64::from_le_bytes(block[0..8].try_into().expect("8 bytes"))
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut filled = 0;
        while filled < dest.len() {
            let block = self.next_block();
            let n = (dest.len() - filled).min(block.len());
            dest[filled..filled + n].copy_from_slice(&block[..n]);
            filled += n;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_produces_the_same_stream() {
        let mut a = DeterministicRng::new([7u8; 32]);
        let mut b = DeterministicRng::new([7u8; 32]);
        let mut buf_a = [0u8; 100];
        let mut buf_b = [0u8; 100];
        a.fill_bytes(&mut buf_a);
        b.fill_bytes(&mut buf_b);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn different_seeds_produce_different_streams() {
        let mut a = DeterministicRng::new([7u8; 32]);
        let mut b = DeterministicRng::new([8u8; 32]);
        let mut buf_a = [0u8; 32];
        let mut buf_b = [0u8; 32];
        a.fill_bytes(&mut buf_a);
        b.fill_bytes(&mut buf_b);
        assert_ne!(buf_a, buf_b);
    }

    #[test]
    fn successive_blocks_from_the_same_rng_differ() {
        let mut rng = DeterministicRng::new([1u8; 32]);
        let first = rng.next_u64();
        let second = rng.next_u64();
        assert_ne!(first, second);
    }
}
