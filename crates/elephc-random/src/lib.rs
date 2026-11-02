//! Purpose:
//! Stable panic-free C ABI for the PHP `random` extension: the four engines, the global
//! Mt19937 state behind `mt_srand`/`mt_rand`/`array_rand`/`shuffle`, the combined LCG behind
//! `lcg_value`, and the CSPRNG behind `random_bytes`/`Secure`.
//!
//! Called from:
//! - elephc-generated code through the `extern "elephc_random"` block in
//!   `src/random_prelude`, and through direct calls emitted by the `mt_rand`/`rand` builtins.
//!
//! Key details:
//! - Engine handles are boxed `Engine` values returned as `u64`; the prelude owns exactly one
//!   handle per engine object and releases it from `__destruct`.
//! - Returned strings are hex or otherwise ASCII so the C-string ABI never truncates them.
//!   They live in a thread-local `CString` that is valid until the next call on the same
//!   thread; the extern call boundary copies the bytes into a PHP string immediately.
//! - `u128` arithmetic (PCG, xoshiro) stays in Rust, so generated programs never need
//!   128-bit PHP arithmetic.

mod csprng;
mod engine;
mod hex;
mod mt;
mod pcg;
mod xoshiro;

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use engine::Engine;
use mt::{Mt19937, MODE_MT19937};

use crate::mt::MODE_PHP;

thread_local! {
    /// Backing store for the most recent ASCII string returned to generated code.
    static RETURN_TEXT: RefCell<CString> = RefCell::new(CString::default());
    /// The global Mt19937 instance behind the procedural `mt_*` functions.
    static GLOBAL_MT: RefCell<Option<Mt19937>> = const { RefCell::new(None) };
    /// The combined LCG behind `lcg_value()`.
    static GLOBAL_LCG: RefCell<Option<CombinedLcg>> = const { RefCell::new(None) };
}

/// Copies `text` into the thread-local return buffer and hands out its pointer.
fn return_text(text: String) -> *const c_char {
    RETURN_TEXT.with(|cell| {
        let owned = CString::new(text).unwrap_or_default();
        *cell.borrow_mut() = owned;
        cell.borrow().as_ptr()
    })
}

/// Converts an incoming NUL-terminated C string, returning an empty slice for a null pointer.
///
/// # Safety
/// `pointer` must be null or point at a NUL-terminated byte sequence valid for the call.
unsafe fn borrow_text<'a>(pointer: *const c_char) -> Option<&'a str> {
    if pointer.is_null() {
        return None;
    }
    CStr::from_ptr(pointer).to_str().ok()
}

/// Runs `action` against the global Mt19937, seeding it from the CSPRNG on first use.
fn with_global_mt<R>(action: impl FnOnce(&mut Mt19937) -> R) -> R {
    GLOBAL_MT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Mt19937::seeded_random(MODE_MT19937));
        }
        action(slot.as_mut().expect("global Mt19937 initialized"))
    })
}

/// Reconstructs an engine handle, returning `None` for an invalid (zero) handle.
///
/// # Safety
/// `handle` must originate from `elephc_random_engine_new` and not have been freed yet.
unsafe fn engine_mut<'a>(handle: u64) -> Option<&'a mut Engine> {
    if handle == 0 {
        return None;
    }
    let pointer = handle as usize as *mut Engine;
    Some(&mut *pointer)
}

/// Allocates an engine and returns its handle, or zero when the inputs are unsupported.
///
/// # Safety
/// `seed_hex` must be null or a NUL-terminated hex string.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_new(
    kind: i64,
    seed_kind: i64,
    seed_int: i64,
    seed_hex: *const c_char,
    mode: i64,
) -> u64 {
    let seed_bytes = if seed_kind == 2 {
        borrow_text(seed_hex).and_then(hex::decode)
    } else {
        None
    };
    let engine = Engine::build(kind, seed_kind, seed_int, seed_bytes.as_deref(), mode as u32);
    match engine {
        Some(value) => Box::into_raw(Box::new(value)) as usize as u64,
        None => 0,
    }
}

/// Releases an engine handle previously returned by `elephc_random_engine_new`.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate; the caller must not reuse it.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_free(handle: u64) {
    if handle == 0 {
        return;
    }
    drop(Box::from_raw(handle as usize as *mut Engine));
}

/// Deep-copies an engine and returns an independent handle, or zero for an invalid handle.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_clone(handle: u64) -> u64 {
    match engine_mut(handle) {
        Some(engine) => Box::into_raw(Box::new(engine.clone())) as usize as u64,
        None => 0,
    }
}

/// Produces the next raw engine value as an unsigned 64-bit bit pattern.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_generate(handle: u64) -> i64 {
    match engine_mut(handle) {
        Some(engine) => engine.generate() as i64,
        None => 0,
    }
}

/// Reports how many bytes the engine's raw value contributes.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_size(handle: u64) -> i64 {
    match engine_mut(handle) {
        Some(engine) => engine.size() as i64,
        None => 0,
    }
}

/// Reports the Mt19937 draw cursor (zero for other engines).
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_count(handle: u64) -> i64 {
    match engine_mut(handle) {
        Some(engine) => engine.count(),
        None => 0,
    }
}

/// Reports the Mt19937 mode (zero for other engines).
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_mode(handle: u64) -> i64 {
    match engine_mut(handle) {
        Some(engine) => engine.mode(),
        None => 0,
    }
}

/// Advances a PCG engine by `advance` steps.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_jump(handle: u64, advance: i64) {
    if let Some(engine) = engine_mut(handle) {
        engine.jump(advance as u64);
    }
}

/// Advances a xoshiro engine by the `jump()` distance.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_jump_short(handle: u64) {
    if let Some(engine) = engine_mut(handle) {
        engine.jump_short();
    }
}

/// Advances a xoshiro engine by the `jumpLong()` distance.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_jump_long(handle: u64) {
    if let Some(engine) = engine_mut(handle) {
        engine.jump_long();
    }
}

/// Returns the engine state as concatenated little-endian word hex.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_state_hex(handle: u64) -> *const c_char {
    let text = match engine_mut(handle) {
        Some(engine) => engine.state_hex(),
        None => String::new(),
    };
    return_text(text)
}

/// Restores an engine state from serialized fields, returning 1 on success and 0 on failure.
///
/// # Safety
/// `handle` must be zero or a live handle from this crate, and `state_hex` must be a
/// NUL-terminated hex string.
#[no_mangle]
pub unsafe extern "C" fn elephc_random_engine_restore_hex(
    handle: u64,
    state_hex: *const c_char,
    count: i64,
    mode: i64,
) -> i64 {
    let Some(text) = borrow_text(state_hex) else {
        return 0;
    };
    match engine_mut(handle) {
        Some(engine) => i64::from(engine.restore_hex(text, count, mode)),
        None => 0,
    }
}

/// Reseeds the global Mt19937 state, clamping unknown modes to the standard variant.
#[no_mangle]
pub extern "C" fn elephc_random_mt_seed(seed: i64, seed_is_null: i64, mode: i64) {
    let resolved = if mode == MODE_PHP as i64 {
        MODE_PHP
    } else {
        MODE_MT19937
    };
    let seed_value = if seed_is_null != 0 {
        let mut bytes = [0u8; 4];
        csprng::fill_or_fallback(&mut bytes);
        u32::from_le_bytes(bytes)
    } else {
        seed as u32
    };
    GLOBAL_MT.with(|cell| {
        *cell.borrow_mut() = Some(Mt19937::seeded(seed_value, resolved));
    });
}

/// Draws one 31-bit value from the global Mt19937 state.
#[no_mangle]
pub extern "C" fn elephc_random_mt_rand31() -> i64 {
    with_global_mt(|generator| generator.generate31() as i64)
}

/// Draws an inclusive range from the global Mt19937 state using php-src's mode rules.
#[no_mangle]
pub extern "C" fn elephc_random_mt_rand_range(min: i64, max: i64) -> i64 {
    with_global_mt(|generator| generator.rand_common(min, max))
}

/// Draws `[0, bound)` from the global Mt19937 state for `array_rand()`/`shuffle()`.
#[no_mangle]
pub extern "C" fn elephc_random_mt_uniform(bound: u64) -> u64 {
    if bound == 0 {
        return 0;
    }
    // A one-element range still consumes one Mt19937 draw, matching php_random_range32(0).
    with_global_mt(|generator| generator.range(0, bound as i64 - 1) as u64)
}

/// Advances the combined LCG and returns a value in `(0, 1)`.
#[no_mangle]
pub extern "C" fn elephc_random_lcg_value() -> f64 {
    GLOBAL_LCG.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(CombinedLcg::seeded());
        }
        slot.as_mut().expect("combined LCG initialized").next()
    })
}

/// Returns `length` CSPRNG bytes encoded as lowercase hex.
#[no_mangle]
pub extern "C" fn elephc_random_bytes_hex(length: i64) -> *const c_char {
    if length <= 0 {
        return return_text(String::new());
    }
    let mut bytes = vec![0u8; length as usize];
    if !csprng::fill(&mut bytes) {
        return return_text(String::new());
    }
    return_text(hex::encode(&bytes))
}

/// Reports and clears whether a strict CSPRNG read has failed since the last query.
#[no_mangle]
pub extern "C" fn elephc_random_csprng_failed() -> i64 {
    i64::from(csprng::take_failure())
}

/// Returns the next representable `f64` above `value`.
#[no_mangle]
pub extern "C" fn elephc_random_next_up(value: f64) -> f64 {
    value.next_up()
}

/// Returns the next representable `f64` below `value`.
#[no_mangle]
pub extern "C" fn elephc_random_next_down(value: f64) -> f64 {
    value.next_down()
}

/// PHP's combined LCG (`php_combined_lcg`), kept as two `int32` states.
struct CombinedLcg {
    /// First LCG state.
    left: i32,
    /// Second LCG state.
    right: i32,
}

impl CombinedLcg {
    /// Seeds both LCG states from eight CSPRNG bytes.
    fn seeded() -> Self {
        let mut bytes = [0u8; 8];
        csprng::fill_or_fallback(&mut bytes);
        let seed = u64::from_le_bytes(bytes);
        Self {
            left: seed as u32 as i32,
            right: (seed >> 32) as u32 as i32,
        }
    }

    /// Advances both LCGs and returns the normalized combined value.
    fn next(&mut self) -> f64 {
        self.left = modmult(self.left, 53668, 40014, 12211, 2_147_483_563);
        self.right = modmult(self.right, 52774, 40692, 3791, 2_147_483_399);
        let mut z = self.left.wrapping_sub(self.right);
        if z < 1 {
            z = z.wrapping_add(2_147_483_562);
        }
        (z as u64) as f64 * 4.656_613e-10
    }
}

/// Applies one `PHP_COMBINED_LCG_MODMULT` step.
fn modmult(state: i32, divisor: i32, multiplier: i32, subtractor: i32, modulus: i32) -> i32 {
    let quotient = state / divisor;
    let mut value =
        (multiplier as i64) * ((state as i64) - (divisor as i64) * (quotient as i64))
            - (subtractor as i64) * (quotient as i64);
    if value < 0 {
        value += modulus as i64;
    }
    value as i32
}

#[cfg(test)]
mod tests {
    //! Reference-vector tests transcribed from php-src's `ext/random/tests` expectations.

    use super::*;
    use crate::pcg::Pcg;
    use crate::xoshiro::Xoshiro;

    /// Reads a `u64` engine value as its little-endian byte sequence.
    fn le_bytes(value: u64) -> [u8; 8] {
        value.to_le_bytes()
    }

    /// Mt19937 seed 1234 reproduces php-src's ten-thousand-and-first draw.
    #[test]
    fn mt19937_reference_value() {
        let mut generator = Mt19937::seeded(1234, MODE_MT19937);
        for _ in 0..10_000 {
            generator.generate();
        }
        assert_eq!(generator.generate().to_le_bytes(), [0x60, 0xfe, 0x95, 0xd9]);
    }

    /// Mt19937 seed 1234 serializes php-src's twisted state with count 0.
    #[test]
    fn mt19937_reference_state() {
        let generator = Mt19937::seeded(1234, MODE_MT19937);
        let state = generator.state_hex();
        assert!(state.starts_with("5aa6b986"));
        assert_eq!(state.len(), 624 * 8);
        assert_eq!(generator.count, 0);
    }

    /// The PHP variant reproduces the historical biased stream.
    #[test]
    fn mt19937_php_mode_reference_value() {
        let mut generator = Mt19937::seeded(12_345_678, MODE_PHP);
        assert_eq!(generator.generate31(), 1_614_640_687);
    }

    /// The PHP variant's ranged draws use the double bad-scaling formula.
    #[test]
    fn mt19937_php_mode_bad_scaling() {
        let mut generator = Mt19937::seeded(0, MODE_PHP);
        assert_eq!(generator.rand_common(0, 999_999_999), 448_865_905);
        assert_eq!(generator.rand_common(0, 999), 592);
    }

    /// PCG seed 1234 serializes php-src's state and matches its post-jump draw.
    #[test]
    fn pcg_reference_value_and_state() {
        let mut generator = Pcg::seeded_int(1234);
        assert_eq!(
            generator.state_hex(),
            "c6d571c37c41a8d1345e7e82265d6e27"
        );
        for _ in 0..10_000 {
            generator.generate();
        }
        generator.jump(1_234_567);
        assert_eq!(
            hex::encode(&le_bytes(generator.generate())),
            "b88e2a0f23720a1d"
        );
    }

    /// PCG's 16-byte string seed matches php-src after one thousand draws.
    #[test]
    fn pcg_string_seed_reference_value() {
        let mut bytes = [0u8; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index % 8 + 1) as u8;
        }
        let mut generator = Pcg::seeded_bytes(bytes);
        for _ in 0..1000 {
            generator.generate();
        }
        assert_eq!(hex::encode(&le_bytes(generator.generate())), "c42016cd9005ef2e");
    }

    /// Xoshiro seed 1234 serializes php-src's state and matches its post-jump draws.
    #[test]
    fn xoshiro_reference_value_and_state() {
        let mut generator = Xoshiro::seeded_int(1234);
        assert_eq!(
            generator.state_hex(),
            "db1c182f1bf60cbb2465f04d36a1c797da25c09be4fabe3333a0d052f241624e"
        );
        for _ in 0..10_000 {
            generator.generate();
        }
        generator.jump();
        generator.jump_long();
        assert_eq!(hex::encode(&le_bytes(generator.generate())), "1f197e9ca7969123");
    }

    /// Xoshiro's 32-byte string seed matches php-src after one thousand draws.
    #[test]
    fn xoshiro_string_seed_reference_value() {
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index % 8 + 1) as u8;
        }
        let mut generator = Xoshiro::seeded_bytes(bytes);
        for _ in 0..1000 {
            generator.generate();
        }
        assert_eq!(hex::encode(&le_bytes(generator.generate())), "90a025df9300cfd1");
    }

    /// The xoshiro restore path rejects an all-zero state.
    #[test]
    fn xoshiro_rejects_zero_restore() {
        let mut generator = Xoshiro::seeded_int(1234);
        assert!(!generator.restore_hex(&"0".repeat(64)));
    }
}
