//! Purpose:
//! Emits the hand-authored WebAssembly text (WAT) runtime for the wasm32-wasi
//! backend: the WASI imports a command module needs and the `__rt_*` helper
//! routines (currently integer echo). Runtime helpers are added to a `WatModule`
//! as raw `(func ...)` blocks.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for command (main-bearing) modules.
//!
//! Key details:
//! - Low linear memory is reserved as runtime scratch:
//!     [0, 8)        iovec for `fd_write`: { buf_ptr @0 (i32), buf_len @4 (i32) }
//!     [8, 16)       `nwritten` cell for `fd_write` / `args_sizes_get` scratch (i32)
//!     [16, 64)      number-formatting buffer (itoa/ftoa), written back-to-front
//!     [64, 65600)   string-concatenation buffer (`_concat_buf`, 64 KiB)
//!   Compile-time data segments and the heap (later phases) start at `RT_SCRATCH_END`.
//! - The concat buffer + `__rt_concat` + the `$__concat_off` cursor are "common"
//!   runtime (no WASI), emitted for every module so any function can concatenate.
//!   WASI imports and the echo/exit helpers are "command" runtime, emitted only
//!   for main-bearing modules (importing WASI forces `_start`-command semantics).

use super::wat::{FuncImport, Global, ValType, WatModule};

/// Base offset of the string-concatenation buffer in linear memory.
const CONCAT_BASE: u32 = 64;
/// Size of the string-concatenation buffer (matches the native 64 KiB `_concat_buf`).
const CONCAT_SIZE: u32 = 65536;

/// First linear-memory offset available to data segments / the heap; everything
/// below this is reserved runtime scratch (number buffer + concat buffer).
pub(super) const RT_SCRATCH_END: u32 = CONCAT_BASE + CONCAT_SIZE;

/// Adds the import-free runtime every module needs: the concat-buffer cursor
/// global and the `__rt_concat` helper. Safe for reactor modules (no WASI).
pub(super) fn emit_common_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__concat_off".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: CONCAT_BASE as i64,
    });
    wm.add_raw_func(RT_CONCAT);
}

/// Adds the WASI imports and `__rt_*` helpers a command (main-bearing) module needs.
///
/// Imports `proc_exit` and `fd_write` from `wasi_snapshot_preview1` and registers
/// the echo helpers. Must be called before functions that reference these symbols
/// are rendered.
pub(super) fn emit_command_runtime(wm: &mut WatModule) {
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "proc_exit".to_string(),
        internal: "wasi_proc_exit".to_string(),
        params: vec![ValType::I32],
        results: vec![],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_write".to_string(),
        internal: "wasi_fd_write".to_string(),
        // fd, iovs_ptr, iovs_len, nwritten_ptr -> errno
        params: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "args_sizes_get".to_string(),
        internal: "wasi_args_sizes_get".to_string(),
        // argc_ptr, argv_buf_size_ptr -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.add_raw_func(RT_ECHO_I64);
    wm.add_raw_func(RT_ECHO_STR);
    wm.add_raw_func(RT_ECHO_BOOL);
    wm.add_raw_func(RT_ARGC);
}

/// `__rt_argc`: returns PHP's `$argc` (the process argument count) via WASI
/// `args_sizes_get`, which writes the count to the number-buffer scratch region.
const RT_ARGC: &str = r#"(func $__rt_argc (result i64)
  (drop (call $wasi_args_sizes_get (i32.const 16) (i32.const 20))) ;; argc@16, argv_buf_size@20
  (i64.extend_i32_u (i32.load (i32.const 16))))                    ;; return argc as i64"#;

/// `__rt_concat`: appends `a` then `b` into the concat buffer at the current
/// `$__concat_off` cursor and returns the freshly-written region as `(ptr, len)`,
/// advancing the cursor. Copying both operands (rather than only the right one)
/// means the returned pointer always addresses a contiguous copy, so chained
/// `a . b . c` concatenations are correct; the cursor is reset to a per-function
/// baseline at statement boundaries by `ConcatReset`.
const RT_CONCAT: &str = r#"(func $__rt_concat (param $aptr i32) (param $alen i64) (param $bptr i32) (param $blen i64) (result i32) (result i64)
  (local $start i32) (local $dest i32) (local $i i32) (local $al i32) (local $bl i32)
  (local.set $start (global.get $__concat_off))           ;; result begins at the cursor
  (local.set $dest (local.get $start))
  (local.set $al (i32.wrap_i64 (local.get $alen)))
  (local.set $bl (i32.wrap_i64 (local.get $blen)))
  (local.set $i (i32.const 0))
  (block $enda (loop $copya                               ;; copy operand a
    (br_if $enda (i32.ge_u (local.get $i) (local.get $al)))
    (i32.store8 (i32.add (local.get $dest) (local.get $i))
                (i32.load8_u (i32.add (local.get $aptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copya)))
  (local.set $dest (i32.add (local.get $dest) (local.get $al)))
  (local.set $i (i32.const 0))
  (block $endb (loop $copyb                               ;; copy operand b
    (br_if $endb (i32.ge_u (local.get $i) (local.get $bl)))
    (i32.store8 (i32.add (local.get $dest) (local.get $i))
                (i32.load8_u (i32.add (local.get $bptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copyb)))
  (global.set $__concat_off
    (i32.add (i32.add (local.get $start) (local.get $al)) (local.get $bl))) ;; advance cursor
  (local.get $start)                                      ;; result ptr
  (i64.add (local.get $alen) (local.get $blen)))          ;; result len"#;

/// `__rt_echo_bool`: PHP `echo` of a boolean writes "1" for true and nothing for
/// false. The value is the i64 boolean (0 or 1).
const RT_ECHO_BOOL: &str = r#"(func $__rt_echo_bool (param $v i64)
  (if (i64.ne (local.get $v) (i64.const 0))
    (then
      (i32.store8 (i32.const 16) (i32.const 49))            ;; '1' into the number buffer
      (i32.store (i32.const 0) (i32.const 16))              ;; iovec.buf_ptr
      (i32.store (i32.const 4) (i32.const 1))               ;; iovec.buf_len = 1
      (drop (call $wasi_fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))) ;; write "1""#;

/// `__rt_echo_str`: writes a string (a linear-memory pointer + byte length) to
/// stdout via `fd_write`. The length is an i64 (PHP int) wrapped to the i32 the
/// iovec field requires.
const RT_ECHO_STR: &str = r#"(func $__rt_echo_str (param $ptr i32) (param $len i64)
  (i32.store (i32.const 0) (local.get $ptr))                ;; iovec.buf_ptr
  (i32.store (i32.const 4) (i32.wrap_i64 (local.get $len))) ;; iovec.buf_len
  (drop (call $wasi_fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))) ;; write to stdout"#;

/// `__rt_echo_i64`: writes a signed 64-bit integer to stdout as decimal text.
///
/// Formats the value back-to-front into the scratch number buffer [16, 64), then
/// points the iovec at the written bytes and calls `fd_write(1, ...)`. The
/// magnitude is taken as unsigned (`0 - v`), which wraps correctly for `i64::MIN`
/// so `div_u`/`rem_u` produce its true digits.
const RT_ECHO_I64: &str = r#"(func $__rt_echo_i64 (param $v i64)
  (local $ptr i32)   ;; back-to-front write cursor into the number buffer
  (local $neg i32)   ;; 1 if the value is negative
  (local $u i64)     ;; magnitude (unsigned)
  (local $len i32)   ;; number of bytes written
  (local.set $ptr (i32.const 64))                              ;; buffer end (exclusive)
  (if (i64.eqz (local.get $v))
    (then
      (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1)))
      (i32.store8 (local.get $ptr) (i32.const 48)))            ;; '0'
    (else
      (local.set $neg (i64.lt_s (local.get $v) (i64.const 0))) ;; sign
      (if (local.get $neg)
        (then (local.set $u (i64.sub (i64.const 0) (local.get $v)))) ;; magnitude (MIN wraps -> correct unsigned)
        (else (local.set $u (local.get $v))))
      (block $done
        (loop $digit
          (br_if $done (i64.eqz (local.get $u)))               ;; stop when no digits left
          (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1)))
          (i32.store8 (local.get $ptr)
            (i32.add (i32.const 48)
              (i32.wrap_i64 (i64.rem_u (local.get $u) (i64.const 10))))) ;; '0' + (u % 10)
          (local.set $u (i64.div_u (local.get $u) (i64.const 10)))      ;; u /= 10
          (br $digit)))
      (if (local.get $neg)
        (then
          (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1)))
          (i32.store8 (local.get $ptr) (i32.const 45))))))     ;; '-'
  (local.set $len (i32.sub (i32.const 64) (local.get $ptr)))   ;; byte count
  (i32.store (i32.const 0) (local.get $ptr))                   ;; iovec.buf_ptr
  (i32.store (i32.const 4) (local.get $len))                   ;; iovec.buf_len
  (drop (call $wasi_fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))) ;; write to stdout"#;
