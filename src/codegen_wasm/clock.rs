//! Purpose:
//! The wall clock on `wasm32-wasi`: `time()` and `microtime()`.
//!
//! Called from:
//! - `runtime` declares the WASI import and registers the helpers.
//! - `builtins` lowers `RuntimeFnId::Time` and `RuntimeFnId::Microtime` onto them.
//! - `capability` gates each call against the operand shapes the helpers read.
//!
//! Key details:
//! - WASI Preview 1 has a real clock: `clock_time_get` with id 0 answers nanoseconds since
//!   the Unix epoch. Nothing about the wall clock needed reimplementing — it just needed an
//!   import this target had never declared, which is why `time()` was the single most cited
//!   blocker in the example corpus at nineteen.
//! - A host that refuses the clock answers an error rather than a time. php has no way to say
//!   "no clock", so the helpers answer 0, which is the epoch — the same shape the file helpers
//!   take when the host grants no filesystem.
//! - `microtime()`'s two forms are different TYPES, not different renderings: `true` answers a
//!   float and the default answers `"0.17535200 1789824612"` — the fraction to eight digits, a
//!   space, then whole seconds. Measured on php-src 8.5.10; the fraction is a fixed width, so
//!   `0.00000000` keeps its zeros.

use super::wat::{Global, ValType, WatModule};

/// Offset of the eight bytes `clock_time_get` writes its answer into.
///
/// Carved from the same hole between `parse_url`'s table and the directory state that `scanf`
/// takes the first 32 bytes of.
pub(super) const CLOCK_OUT: u32 = 0x5120;

/// Proves the slot fits the hole, on both sides.
const _: () = assert!(
    CLOCK_OUT >= 0x5120 && CLOCK_OUT + 8 <= 0x5200,
    "the clock scratch overruns the hole before the directory state"
);

/// Adds the clock to `wm`. Requires the `clock_time_get` import and the string runtime.
pub(super) fn emit_clock_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__clock_out".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: CLOCK_OUT as i64,
    });
    wm.add_raw_func(&rt_realtime_ns());
    wm.add_raw_func(RT_TIME);
    wm.add_raw_func(RT_MICROTIME_FLOAT);
    wm.add_raw_func(&rt_microtime_string());
}

/// `__rt_realtime_ns`: nanoseconds since the Unix epoch, or 0 when the host refuses.
///
/// Clock id 0 is `CLOCK_REALTIME`. The precision argument asks for a resolution rather than
/// demanding one, and 1000 nanoseconds is what php's own microsecond granularity needs.
fn rt_realtime_ns() -> String {
    format!(
        r#"(func $__rt_realtime_ns (result i64)
  (if (i32.ne (call $wasi_clock_time_get (i32.const 0) (i64.const 1000)
        (i32.add (global.get $__float_scratch) (i32.const {out})))
      (i32.const 0))
    (then (return (i64.const 0))))                                 ;; no clock: the epoch
  (i64.load (i32.add (global.get $__float_scratch) (i32.const {out}))))
"#,
        out = CLOCK_OUT
    )
}

/// `__rt_time`: php's `time()`, whole seconds since the epoch.
const RT_TIME: &str = r#"(func $__rt_time (result i64)
  (i64.div_u (call $__rt_realtime_ns) (i64.const 1000000000)))
"#;

/// `__rt_microtime_float`: php's `microtime(true)`, seconds as a double.
///
/// The split into whole seconds and a remainder before the division is not ceremony: a double
/// holds 53 bits of mantissa and the nanosecond count passed that in 1970 plus a few months,
/// so converting the whole thing at once would round away the microseconds this function
/// exists to report. The remainder is quantized to whole microseconds, like php's
/// `tv_usec / 1000000.0`.
const RT_MICROTIME_FLOAT: &str = r#"(func $__rt_microtime_float (result f64)
  (local $ns i64)
  (local.set $ns (call $__rt_realtime_ns))
  (f64.add
    (f64.convert_i64_u (i64.div_u (local.get $ns) (i64.const 1000000000)))
    (f64.div (f64.convert_i64_u (i64.div_u (i64.rem_u (local.get $ns) (i64.const 1000000000))
                                           (i64.const 1000)))   ;; whole microseconds, as php
             (f64.const 1000000))))
"#;

/// `__rt_microtime_string`: php's default `microtime()`, `"0.ffffffff ssssssssss"`.
///
/// The fraction is eight digits wide and zero-padded, which is php's `%.8F` of the seconds
/// remainder — so a clock that lands exactly on a second answers `0.00000000`, not `0.0`.
/// php reads microseconds (`gettimeofday`) and prints them times one hundred, so the last two
/// digits are always `00`; the nanoseconds are truncated to whole microseconds first.
fn rt_microtime_string() -> String {
    format!(
        r#"(func $__rt_microtime_string (result i32 i64)
  (local $ns i64) (local $out i32) (local $w i32)
  (local $ptr i32) (local $len i64) (local $sptr i32) (local $slen i64)
  (local.set $ns (call $__rt_realtime_ns))
  (local.set $out (call $__rt_heap_alloc (i32.const 48)))
  (if (i32.eqz (local.get $out)) (then (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  (i32.store8 (local.get $out) (i32.const 48))                     ;; '0'
  (i32.store8 offset=1 (local.get $out) (i32.const 46))            ;; '.'
  (local.set $w (i32.const 2))
  ;; the fraction, eight digits zero-padded — the renderer's own scratch is reused by the
  ;; second call below, so the bytes are copied out before that happens
  (call $__rt_fmt_int (i64.mul (i64.div_u (i64.rem_u (local.get $ns) (i64.const 1000000000))
                                          (i64.const 1000))
                              (i64.const 100))
                      (i32.const 0) (i64.const 8) (i32.const 48) (i32.const 0))
  (local.set $len)
  (local.set $ptr)
  (memory.copy (i32.add (local.get $out) (local.get $w)) (local.get $ptr)
               (i32.wrap_i64 (local.get $len)))
  (local.set $w (i32.add (local.get $w) (i32.wrap_i64 (local.get $len))))
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 32))  ;; ' '
  (local.set $w (i32.add (local.get $w) (i32.const 1)))
  (call $__rt_fmt_int (i64.div_u (local.get $ns) (i64.const 1000000000))
                      (i32.const 0) (i64.const 0) (i32.const 32) (i32.const 0))
  (local.set $len)
  (local.set $ptr)
  (memory.copy (i32.add (local.get $out) (local.get $w)) (local.get $ptr)
               (i32.wrap_i64 (local.get $len)))
  (local.set $w (i32.add (local.get $w) (i32.wrap_i64 (local.get $len))))
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (local.get $w)))
  (local.set $slen)
  (local.set $sptr)
  (call $__rt_heap_free (local.get $out))
  (local.get $sptr) (local.get $slen))
"#
    )
}

#[cfg(test)]
mod tests {
    use crate::ir::RuntimeFnId;

    /// The two clock builtins are served, and the sleeping ones are not.
    ///
    /// `usleep` and `sleep` need `poll_oneoff`, whose subscription records are a different
    /// import and a different shape; they stay refused rather than being turned into a busy
    /// wait, which would burn a host's CPU to imitate a function whose whole purpose is not to.
    #[test]
    fn the_wall_clock_is_served_and_sleeping_is_not() {
        for served in [RuntimeFnId::Time, RuntimeFnId::Microtime] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(served),
                "{served:?} must be supported on this target"
            );
        }
        for absent in [RuntimeFnId::Sleep, RuntimeFnId::Usleep] {
            assert!(
                !crate::codegen_wasm::capability::runtime_function_is_supported(absent),
                "{absent:?} needs poll_oneoff and must stay refused"
            );
        }
    }
}
