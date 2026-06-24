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

/// Base of the dedicated float<->string scratch region. The strtod bignum buffers
/// (`__rt_digits_to_f64` / `__rt_str_to_f64`) and the later ftoa/itoa scratch live
/// here, above the concat buffer, so a parse or format never collides with an
/// in-flight string concatenation whose cursor would otherwise run through 0x4000.
/// Callers reach this base via the immutable `$__float_scratch` global.
pub(super) const FLOAT_SCRATCH_BASE: u32 = RT_SCRATCH_END;

/// Size of the float<->string scratch region. The strtod path uses offsets
/// 0..0x1200 (four 96-limb bignums at +0/+1024/+2048/+3072 and the digit buffer at
/// +4096); the ftoa/itoa scratch lands at +0x2000..+0x3000. 16 KiB bounds both.
pub(super) const FLOAT_SCRATCH_SIZE: u32 = 0x4000;

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
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "args_get".to_string(),
        internal: "wasi_args_get".to_string(),
        // argv_ptr_array, argv_buf -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.add_raw_func(RT_ECHO_I64);
    wm.add_raw_func(RT_ECHO_F64);
    wm.add_raw_func(RT_ECHO_STR);
    wm.add_raw_func(RT_ECHO_BOOL);
    wm.add_raw_func(RT_ARGC);
    wm.add_raw_func(RT_STRLEN_C);
    wm.add_raw_func(RT_ARGV);
    wm.add_raw_func(RT_MIXED_WRITE_STDOUT);
}

/// `__rt_argc`: returns PHP's `$argc` (the process argument count) via WASI
/// `args_sizes_get`, which writes the count to the number-buffer scratch region.
const RT_ARGC: &str = r#"(func $__rt_argc (result i64)
  (drop (call $wasi_args_sizes_get (i32.const 16) (i32.const 20))) ;; argc@16, argv_buf_size@20
  (i64.extend_i32_u (i32.load (i32.const 16))))                    ;; return argc as i64"#;

/// `__rt_strlen_c`: byte length of a NUL-terminated C string (used to measure the
/// WASI argv entries before copying them into PHP strings).
const RT_STRLEN_C: &str = r#"(func $__rt_strlen_c (param $p i32) (result i32)
  (local $n i32)
  (local.set $n (i32.const 0))
  (block $end (loop $scan
    (br_if $end (i32.eqz (i32.load8_u (i32.add (local.get $p) (local.get $n)))))  ;; stop at the NUL terminator
    (local.set $n (i32.add (local.get $n) (i32.const 1)))
    (br $scan)))
  (local.get $n))"#;

/// `__rt_argv`: builds PHP's `$argv` as an indexed string array via WASI
/// `args_sizes_get` + `args_get`. Temporary heap buffers hold the WASI pointer
/// array and argument byte buffer; each argument is copied (persisted) into the
/// array via `__rt_array_push_str`, after which the temporaries are freed.
const RT_ARGV: &str = r#"(func $__rt_argv (result i32)
  (local $argc i32)
  (local $bufsize i32)
  (local $ptrs i32)
  (local $buf i32)
  (local $arr i32)
  (local $i i32)
  (local $argp i32)
  (local $len i32)
  (drop (call $wasi_args_sizes_get (i32.const 16) (i32.const 20)))   ;; argc@16, argv_buf_size@20
  (local.set $argc (i32.load (i32.const 16)))
  (local.set $bufsize (i32.load (i32.const 20)))
  (local.set $ptrs (call $__rt_heap_alloc (i32.mul (local.get $argc) (i32.const 4))))  ;; argc i32 pointers
  (local.set $buf (call $__rt_heap_alloc (local.get $bufsize)))      ;; argv byte buffer
  (drop (call $wasi_args_get (local.get $ptrs) (local.get $buf)))    ;; fill the pointer array + buffer
  (local.set $arr (call $__rt_array_new (i64.extend_i32_u (local.get $argc)) (i64.const 16)))  ;; string array
  (local.set $i (i32.const 0))
  (block $end (loop $loop
    (br_if $end (i32.ge_u (local.get $i) (local.get $argc)))
    (local.set $argp (i32.load (i32.add (local.get $ptrs) (i32.mul (local.get $i) (i32.const 4)))))  ;; argv[i] (C string)
    (local.set $len (call $__rt_strlen_c (local.get $argp)))         ;; its byte length
    (local.set $arr (call $__rt_array_push_str (local.get $arr) (local.get $argp) (i64.extend_i32_u (local.get $len))))  ;; append a persisted copy
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $loop)))
  (call $__rt_heap_free (local.get $ptrs))                          ;; temporaries no longer needed (args were copied)
  (call $__rt_heap_free (local.get $buf))
  (local.get $arr))"#;

/// `__rt_mixed_write_stdout`: echoes a boxed Mixed value by dispatching on its tag:
/// int (0) via `__rt_echo_i64`, float (2) via `__rt_echo_f64` (`%.14G`), string (1)
/// via `__rt_echo_str`, bool (3) via `__rt_echo_bool`; null (8) and non-scalar tags
/// print nothing (PHP semantics).
const RT_MIXED_WRITE_STDOUT: &str = r#"(func $__rt_mixed_write_stdout (param $ptr i32)
  (local $tag i64)
  (if (i32.eqz (local.get $ptr))
    (then (return)))                                                ;; null pointer -> nothing
  (local.set $tag (i64.load (local.get $ptr)))                      ;; tag @ +0
  (if (i64.eqz (local.get $tag))                                    ;; tag 0 = int
    (then
      (call $__rt_echo_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 1))                       ;; tag 1 = string
    (then
      (call $__rt_echo_str
        (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))
        (i64.load (i32.add (local.get $ptr) (i32.const 16))))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 2))                       ;; tag 2 = float
    (then
      (call $__rt_echo_f64 (f64.load (i32.add (local.get $ptr) (i32.const 8)))) ;; %.14G text via __rt_ftoa + fd_write
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 3))                       ;; tag 3 = bool
    (then
      (call $__rt_echo_bool (i64.load (i32.add (local.get $ptr) (i32.const 8))))
      (return))))
"#;

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

/// `__rt_echo_f64`: writes a PHP float to stdout as `%.14G` text. The float arrives
/// as a wasm `f64`; its bits are reinterpreted to an `i64` for `__rt_ftoa`, which
/// renders into the float-scratch output region (scratch+4096) and returns
/// `(ptr, len)`. The iovec at [0, 16) is then pointed at those bytes and `fd_write`
/// flushes them to stdout. Mirrors `__rt_echo_str` once the text is materialized.
const RT_ECHO_F64: &str = r#"(func $__rt_echo_f64 (param $v f64)
  (local $bits i64)                                         ;; f64 bits handed to __rt_ftoa
  (local $ptr i32)                                          ;; formatted text pointer (from __rt_ftoa)
  (local $len i32)                                          ;; formatted text length (from __rt_ftoa)
  (local.set $bits (i64.reinterpret_f64 (local.get $v)))    ;; f64 value -> raw bits for __rt_ftoa
  (call $__rt_ftoa (local.get $bits) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 768) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096 -> (ptr,len)
  (local.set $len)                                          ;; pop ftoa length (result 1, on top)
  (local.set $ptr)                                          ;; pop ftoa pointer (result 0)
  (i32.store (i32.const 0) (local.get $ptr))                ;; iovec.buf_ptr
  (i32.store (i32.const 4) (local.get $len))                ;; iovec.buf_len
  (drop (call $wasi_fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))) ;; write to stdout"#;
