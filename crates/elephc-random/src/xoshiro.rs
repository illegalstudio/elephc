//! Purpose:
//! PHP's Xoshiro256** engine, including `jump()` and `jumpLong()`.
//!
//! Called from:
//! - `crate::Engine` for `Random\Engine\Xoshiro256StarStar`.
//!
//! Key details:
//! - Seeding from an integer uses `splitmix64` four times; a 32-byte string seed is copied
//!   little-endian per word; a null seed retries the CSPRNG until the state is not all zero.
//! - `restore_hex` also rejects the all-zero state, matching GH-21731.

/// The `jump()` distance constants from php-src.
const JUMP: [u64; 4] = [
    0x180e_c6d3_3cfd_0aba,
    0xd5a6_1266_f0c9_392c,
    0xa958_2618_e03f_c9aa,
    0x39ab_dc45_29b1_661c,
];
/// The `jumpLong()` distance constants from php-src.
const JUMP_LONG: [u64; 4] = [
    0x76e1_5d3e_fefd_cbbf,
    0xc500_4e44_1c52_2fb3,
    0x7771_0069_854e_e241,
    0x3910_9bb0_2acb_e635,
];

/// One Xoshiro256** generator.
#[derive(Clone)]
pub struct Xoshiro {
    /// The four 64-bit state words.
    pub state: [u64; 4],
}

impl Xoshiro {
    /// Builds the state from a 64-bit integer seed through four `splitmix64` outputs.
    pub fn seeded_int(seed: u64) -> Self {
        let mut seed = seed;
        let mut state = [0u64; 4];
        for word in state.iter_mut() {
            *word = splitmix64(&mut seed);
        }
        Self { state }
    }

    /// Builds the state from 32 raw seed bytes, little-endian per 8-byte word.
    pub fn seeded_bytes(bytes: [u8; 32]) -> Self {
        let mut state = [0u64; 4];
        for (index, word) in state.iter_mut().enumerate() {
            *word = u64::from_le_bytes(
                bytes[index * 8..index * 8 + 8]
                    .try_into()
                    .expect("8 seed bytes"),
            );
        }
        Self { state }
    }

    /// Produces one 64-bit output and advances the state.
    pub fn generate(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;

        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);

        result
    }

    /// Advances the state by the `jump()` distance.
    pub fn jump(&mut self) {
        self.apply_jump(&JUMP);
    }

    /// Advances the state by the `jumpLong()` distance.
    pub fn jump_long(&mut self) {
        self.apply_jump(&JUMP_LONG);
    }

    /// Applies one of php-src's two jump constant vectors.
    fn apply_jump(&mut self, constants: &[u64; 4]) {
        let mut accumulator = [0u64; 4];
        for constant in constants {
            for bit in 0..64 {
                if constant & (1u64 << bit) != 0 {
                    for index in 0..4 {
                        accumulator[index] ^= self.state[index];
                    }
                }
                self.generate();
            }
        }
        self.state = accumulator;
    }

    /// Serializes the four words in php-src's little-endian hex form.
    pub fn state_hex(&self) -> String {
        let mut text = String::with_capacity(64);
        for word in self.state {
            text.push_str(&crate::hex::encode_u64(word));
        }
        text
    }

    /// Restores the state from four little-endian 64-bit hex fields, rejecting all zero.
    pub fn restore_hex(&mut self, text: &str) -> bool {
        if text.len() != 64 {
            return false;
        }
        let Some(bytes) = crate::hex::decode(text) else {
            return false;
        };
        let mut state = [0u64; 4];
        for (index, word) in state.iter_mut().enumerate() {
            *word = u64::from_le_bytes(
                bytes[index * 8..index * 8 + 8]
                    .try_into()
                    .expect("8 state bytes"),
            );
        }
        if state == [0; 4] {
            return false;
        }
        self.state = state;
        true
    }
}

/// One SplitMix64 step used to expand a 64-bit seed into the xoshiro state.
fn splitmix64(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *seed;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
