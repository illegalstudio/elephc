//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) file-I/O runtime for the wasm32-wasi
//! backend: `fopen`, `fread`, `fwrite`, `fclose`, `file_get_contents`,
//! `file_put_contents`, `file_exists` and `unlink`.
//!
//! Called from:
//! - `crate::codegen_wasm::runtime::emit_command_runtime()`, after the WASI imports
//!   it shares and before the helpers that call `__rt_heap_alloc`/`__rt_str_persist`.
//!
//! Key details:
//! - WASI Preview 1 is CAPABILITY-based: a path can only be opened relative to a
//!   directory the host preopened. `__rt_wasi_dirfd` finds the first preopen by
//!   probing `fd_prestat_get` from fd 3 upward, and every path call uses it. With no
//!   preopen at all — `node` without `preopens`, `wasmer` without `--dir` — every
//!   open fails, which is the same `false` PHP answers for an unopenable path.
//! - A PHP stream handle is a boxed Mixed cell with the resource tag (9) carrying the
//!   WASI fd as its payload, so it flows through locals, arrays and arguments as any
//!   other boxed value does.
//! - Syscall scratch lives at `$__float_scratch + 0x3000`, in the 4 KiB the strtod
//!   bignums (0..0x1200) and the ftoa/itoa buffers (0x2000..0x3000) leave free.

use super::wat::WatModule;

/// Offset of this module's syscall scratch within the float-scratch region.
pub(super) const IO_SCRATCH: u32 = 0x3000;

/// How many fds the inline per-fd tables cover; fds from here up use the extension block.
pub(super) const FD_TABLE_INLINE: u32 = 256;

/// One past the highest fd this runtime tracks.
///
/// php never limits open files itself — the OS does, and its default soft limit is 1024, where
/// `fopen` fails with `Too many open files` and answers `false`. A descriptor the host hands out
/// at or past this number is closed again and the open answers `false` the same way, so no stream
/// ever exists without its eof flag, closed mark, mode/uri record, chunk size and directory state.
pub(super) const FD_LIMIT: u32 = 1024;

/// The fd value of every `php://output` handle.
///
/// `php://output` is NOT stdout: its writes go through the output buffers, which `php://stdout`'s
/// bypass. It therefore needs an identity of its own, and this is the one fd number a host can
/// never hand out, because `fopen`/`opendir` refuse any host fd at or past `FD_HOST_LIMIT`. It
/// still sits inside the tracked range, so its eof flag, closed mark and metadata record exist.
pub(super) const FD_OUTPUT: u32 = FD_LIMIT - 1;

/// One past the highest fd a HOST may hand out; `FD_OUTPUT` and above are reserved.
pub(super) const FD_HOST_LIMIT: u32 = FD_LIMIT - 1;

/// Scratch word (8 bytes) counting the bytes written through `php://output`, for `ftell`.
pub(super) const OUTPUT_POS: u32 = IO_SCRATCH + 0x18;

/// Offset of `fd_seek`'s 8-byte result (`ftell`, `fseek`, the seekability probe).
pub(super) const SEEK_OUT: u32 = IO_SCRATCH + 0x40;

/// The WAT of `ftell`/`fseek`/`rewind` spells `SEEK_OUT` as the literal 12352, so the two are tied.
const _: () = assert!(SEEK_OUT == 12352, "the fd_seek result literal in the WAT drifted from SEEK_OUT");

/// Scratch word holding the address of the lazily allocated extension block (0 until needed).
pub(super) const FD_EXT_PTR: u32 = IO_SCRATCH + 0x28;

/// Bytes of per-fd state in the extension block: eof flag +0, temp-file index +1, chunk size +4,
/// stream metadata +8 (16 bytes), directory state +24 (16 bytes).
const FD_EXT_STRIDE: u32 = 40;

/// Sixteen bytes that are never written: what a stream with no recorded metadata reads.
pub(super) const ZERO16: u32 = IO_SCRATCH + 0x270;

/// Scratch byte whose bits 0..2 record that `STDIN`/`STDOUT`/`STDERR` were closed by `fclose`.
///
/// Those three constants are bare fd numbers with no per-open record to hold a closed flag, and
/// php really closes them (every copy then reads as a closed stream). The host fds themselves stay
/// open on purpose — `echo` and the uncaught-error path keep writing — so only this mark changes.
pub(super) const STD_CLOSED: u32 = IO_SCRATCH + 0x300;

const _: () = assert!(STD_CLOSED < FD_STREAM_META, "the std-closed byte overruns the io scratch region");

/// Offset of the 64-byte `filestat` result. It has a region of its own: it used to sit at
/// `IO_SCRATCH + 0x40`, on top of the first 48 fds' eof flags, so a `file_exists()` cleared them.
pub(super) const FILESTAT: u32 = IO_SCRATCH + 0x280;

/// Bytes of the `filestat` result.
pub(super) const FILESTAT_BYTES: u32 = 64;

const _: () = assert!(
    FILESTAT + FILESTAT_BYTES <= FD_STREAM_META,
    "the filestat result overruns the io scratch region"
);

/// Offset of the per-fd EOF flag table: one byte per real WASI fd, 256 fds.
///
/// PHP's `feof` is a FLAG a read sets when it finds nothing, never a position
/// comparison — reading exactly the last byte leaves it false, and only the next
/// read flips it (measured on php-src 8.5.6, same rule `__rt_memstream_eof`
/// documents). A real fd carries no descriptor block to hold that flag, so it
/// lives here, indexed by fd. Linear memory starts zeroed, so every fd begins
/// "not at EOF"; `fclose` clears the byte because WASI recycles fd numbers.
pub(super) const FD_EOF_FLAGS: u32 = IO_SCRATCH + 0x50;

/// Offset of the per-fd temp-file table: one byte per real WASI fd, 256 fds.
///
/// `tmpfile()` opens a file that PHP deletes when the handle closes. WASI preview1 cannot
/// unlink an open file, so the deletion happens at `fclose` instead — which means `fclose` has
/// to know the NAME. It is not stored: the name is a function of the index this table holds
/// (0 means "not a temp file", n means index n-1), so one byte per fd suffices.
pub(super) const FD_TMP_INDEX: u32 = IO_SCRATCH + 0x150;

/// Scratch for the generated temp-file name, rebuilt on open and again on close.
pub(super) const TMP_NAME: u32 = IO_SCRATCH + 0x250;

/// Length of the generated temp-file name: `.elephc_tmpNNN`.
pub(super) const TMP_NAME_LEN: u32 = 14;

/// Offset of the per-fd stream-metadata table: 16 bytes per fd, 256 fds.
///
/// `stream_get_meta_data` reports the MODE and URI `fopen` was called with, and a
/// bare WASI fd remembers neither — so `fopen` records them here as persisted-string
/// (ptr, len) pairs: mode at +0/+4, uri at +8/+12. `fclose` releases both strings
/// and zeroes the record, because WASI recycles fd numbers. A never-recorded fd
/// (a stream this runtime did not open) reads as two empty strings.
pub(super) const FD_STREAM_META: u32 = 0x4000;

/// Adds the file-I/O runtime to `wm`. Requires the WASI path imports and the heap
/// and string runtimes, all of which the command runtime emits alongside it.
pub(super) fn emit_file_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_FOPEN_FAILED);
    wm.add_raw_func(RT_STD_STREAM_FD);
    wm.add_raw_func(&rt_wasi_dirfd());
    wm.add_raw_func(&rt_fopen());
    wm.add_raw_func(&rt_stream_fd());
    wm.add_raw_func(&rt_fwrite());
    wm.add_raw_func(RT_FWRITE_BOXED);
    wm.add_raw_func(&rt_fread());
    wm.add_raw_func(&rt_fgetc());
    wm.add_raw_func(&rt_tmp_name());
    wm.add_raw_func(&rt_tmpfile());
    wm.add_raw_func(&rt_fclose());
    wm.add_raw_func(RT_FLOCK);
    wm.add_raw_func(&rt_file_exists());
    wm.add_raw_func(&rt_unlink());
    wm.add_raw_func(&rt_readfile());
    wm.add_raw_func(&rt_file_size());
    wm.add_raw_func(&rt_file_get_contents());
    wm.add_raw_func(&rt_file_put_contents());
    wm.add_raw_func(RT_IS_MEMSTREAM_PATH);
    wm.add_raw_func(RT_IS_DATA_URI);
    wm.add_raw_func(RT_DATA_URI_OPEN);
    wm.add_raw_func(&rt_data_uri_contents());
    wm.add_raw_func(&rt_memstream_new());
    wm.add_raw_func(RT_MEMSTREAM_GROW);
    wm.add_raw_func(&rt_memstream_write());
    wm.add_raw_func(&rt_memstream_read());
    wm.add_raw_func(RT_MEMSTREAM_TELL);
    wm.add_raw_func(RT_MEMSTREAM_SEEK);
    wm.add_raw_func(RT_MEMSTREAM_EOF);
    wm.add_raw_func(RT_MEMSTREAM_CLOSE);
    wm.add_raw_func(&rt_handle_closed());
    wm.add_raw_func(RT_BOX_CLOSED);
    wm.add_raw_func(&rt_fd_eof_get());
    wm.add_raw_func(&rt_fd_eof_set());
    wm.add_raw_func(&rt_fd_eof_clear());
    wm.add_raw_func(&rt_fd_addr());
    wm.add_raw_func(RT_STREAM_NEW);
    wm.add_raw_func(RT_HANDLE_FD);
    wm.add_raw_func(&rt_stream_record_close());
    wm.add_raw_func(RT_STREAM_CELL_CLOSE);
    wm.add_raw_func(RT_STREAM_CELL_RELEASE);
    wm.add_raw_func(&rt_fd_meta_record());
    wm.add_raw_func(&rt_fd_meta_clear());
    wm.add_raw_func(RT_FEOF);
    wm.add_raw_func(&rt_ftell());
    wm.add_raw_func(RT_FSEEK);
    wm.add_raw_func(&rt_stream_get_contents());
    wm.add_raw_func(&rt_stream_read_unsized());
    wm.add_raw_func(RT_STREAM_COPY_TO_STREAM);
    wm.add_raw_func(RT_REWIND);
    wm.add_raw_func(&rt_stream_get_line());
}

/// `__rt_fd_addr`: the address of one per-fd record, or 0 for an fd this runtime does not track.
///
/// `kind` picks the table: 0 the eof flag byte (bit 0 eof), 1 the temp-file index
/// byte, 2 the 16-byte mode/uri record, 3 the 4-byte chunk size, 4 the 16-byte directory state.
/// An fd below 256 lives in the inline table; one up to `FD_LIMIT` lives in an extension block
/// allocated (zeroed) on first use, so every table covers the same fds and none of them can fall
/// out of step — an fd past the inline tables used to lose its eof flag and its
/// metadata, which then read slot 0.
fn rt_fd_addr() -> String {
    // (kind, inline base, shift, extension offset)
    let kinds = [
        (0, FD_EOF_FLAGS, 0, 0),
        (1, FD_TMP_INDEX, 0, 1),
        (2, FD_STREAM_META, 4, 8),
        (3, super::streamtune::FD_CHUNK_SIZE, 2, 4),
        (4, super::dirs::DIR_STATE, 4, 24),
    ];
    let mut pick = String::new();
    for (kind, base, shift, off) in kinds {
        pick.push_str(&format!(
            "  (if (i32.eq (local.get $kind) (i32.const {kind}))\n    (then (local.set $base (i32.const {base})) (local.set $shift (i32.const {shift})) (local.set $off (i32.const {off}))))\n"
        ));
    }
    format!(
        r#"(func $__rt_fd_addr (param $fd i32) (param $kind i32) (result i32)
  (local $base i32) (local $shift i32) (local $off i32) (local $ext i32)
{pick}  (if (i32.lt_u (local.get $fd) (i32.const {inline}))
    (then (return (i32.add (i32.add (global.get $__float_scratch) (local.get $base))
                           (i32.shl (local.get $fd) (local.get $shift))))))
  (if (i32.ge_u (local.get $fd) (i32.const {limit}))              ;; also every negative fd
    (then (return (i32.const 0))))
  (local.set $ext (i32.load (i32.add (global.get $__float_scratch) (i32.const {ptr}))))
  (if (i32.eqz (local.get $ext))
    (then
      (local.set $ext (call $__rt_heap_alloc (i32.const {bytes})))
      (memory.fill (local.get $ext) (i32.const 0) (i32.const {bytes}))
      (i32.store (i32.add (global.get $__float_scratch) (i32.const {ptr})) (local.get $ext))))
  (i32.add (i32.add (local.get $ext)
                    (i32.mul (i32.sub (local.get $fd) (i32.const {inline})) (i32.const {stride})))
           (local.get $off)))
"#,
        pick = pick,
        inline = FD_TABLE_INLINE,
        limit = FD_LIMIT,
        ptr = FD_EXT_PTR,
        stride = FD_EXT_STRIDE,
        bytes = FD_EXT_STRIDE * (FD_LIMIT - FD_TABLE_INLINE)
    )
}

/// `__rt_fd_meta_record`: remembers a real fd's fopen MODE and URI for
/// `stream_get_meta_data`, releasing any stale record the recycled fd left behind.
fn rt_fd_meta_record() -> String {
    r#"(func $__rt_fd_meta_record (param $fd i32) (param $mode i32) (param $mode_len i64) (param $uri i32) (param $uri_len i64)
  (local $slot i32) (local $p i32) (local $l i64)
  (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 2)))
  (if (i32.eqz (local.get $slot))
    (then (return)))
  (call $__rt_fd_meta_clear (local.get $fd))
  (call $__rt_fd_eof_clear (local.get $fd))                       ;; a reused fd number starts open
  (call $__rt_str_persist (local.get $mode) (local.get $mode_len))
  (local.set $l)
  (local.set $p)
  (i32.store (local.get $slot) (local.get $p))
  (i32.store (i32.add (local.get $slot) (i32.const 4)) (i32.wrap_i64 (local.get $l)))
  (call $__rt_str_persist (local.get $uri) (local.get $uri_len))
  (local.set $l)
  (local.set $p)
  (i32.store (i32.add (local.get $slot) (i32.const 8)) (local.get $p))
  (i32.store (i32.add (local.get $slot) (i32.const 12)) (i32.wrap_i64 (local.get $l))))
"#
    .to_string()
}

/// `__rt_fd_meta_clear`: releases a closed fd's recorded mode/uri strings and zeroes
/// the record — the next `fopen` may be handed the same fd number back.
fn rt_fd_meta_clear() -> String {
    r#"(func $__rt_fd_meta_clear (param $fd i32)
  (local $slot i32)
  (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 2)))
  (if (i32.eqz (local.get $slot))
    (then (return)))
  (call $__rt_decref_any (i32.load (local.get $slot)))
  (call $__rt_decref_any (i32.load (i32.add (local.get $slot) (i32.const 8))))
  (i64.store (local.get $slot) (i64.const 0))
  (i64.store (i32.add (local.get $slot) (i32.const 8)) (i64.const 0))
  (call $__rt_fd_chunk_clear (local.get $fd)))
"#
    .to_string()
}

/// `__rt_fd_eof_get`: the EOF flag a real fd's reads have set, 0 for an untracked fd.
fn rt_fd_eof_get() -> String {
    r#"(func $__rt_fd_eof_get (param $fd i32) (result i64)
  (local $a i32)
  (local.set $a (call $__rt_fd_addr (local.get $fd) (i32.const 0)))
  (if (i32.eqz (local.get $a))
    (then (return (i64.const 0))))
  (i64.extend_i32_u (i32.and (i32.load8_u (local.get $a)) (i32.const 1))))
"#
    .to_string()
}

/// `__rt_fd_eof_set`: records that a read on this fd found nothing.
fn rt_fd_eof_set() -> String {
    r#"(func $__rt_fd_eof_set (param $fd i32)
  (local $a i32)
  (local.set $a (call $__rt_fd_addr (local.get $fd) (i32.const 0)))
  (if (i32.eqz (local.get $a))
    (then (return)))
  (i32.store8 (local.get $a) (i32.const 1)))
"#
    .to_string()
}

/// `__rt_fd_eof_clear`: forgets the flag — a successful seek does, and `fclose` must,
/// because the next `fopen` may be handed the same fd number back.
fn rt_fd_eof_clear() -> String {
    r#"(func $__rt_fd_eof_clear (param $fd i32)
  (local $a i32)
  (local.set $a (call $__rt_fd_addr (local.get $fd) (i32.const 0)))
  (if (i32.eqz (local.get $a))
    (then (return)))
  (i32.store8 (local.get $a) (i32.const 0)))
"#
    .to_string()
}

/// `__rt_is_memstream_path`: whether a path names an in-memory stream.
///
/// `php://memory` is 13 bytes and `php://temp` 12. php-src also accepts a `php://temp/maxmemory:N`
/// suffix, which only chooses when the stream spills to a real file — something this
/// implementation never does — so the prefix decides and the suffix is ignored.
const RT_IS_MEMSTREAM_PATH: &str = r#"(func $__rt_is_memstream_path (param $p i32) (param $len i64) (result i32)
  (if (i64.lt_u (local.get $len) (i64.const 10))
    (then (return (i32.const 0))))
  (if (i32.eqz (i32.and (i32.and
        (i32.and (i32.eq (i32.load8_u (local.get $p)) (i32.const 112))
                 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 104)))
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 2))) (i32.const 112))
                 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 3))) (i32.const 58))))
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 4))) (i32.const 47))
                 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 5))) (i32.const 47)))))
    (then (return (i32.const 0))))
  (if (i32.and                                                    ;; "temp"
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 6))) (i32.const 116))
                 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 7))) (i32.const 101)))
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 8))) (i32.const 109))
                 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 9))) (i32.const 112))))
    (then (return (i32.const 1))))
  (if (i64.ne (local.get $len) (i64.const 12))                    ;; "memory" is exactly 12 bytes
    (then (return (i32.const 0))))
  (i32.and
    (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 6))) (i32.const 109))
             (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 7))) (i32.const 101)))
    (i32.and
      (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 8))) (i32.const 109))
               (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 9))) (i32.const 111)))
      (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 10))) (i32.const 114))
               (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 11))) (i32.const 121))))))
"#;

/// `__rt_memstream_write`: writes at the current position, OVERWRITING what is there.
///
/// Measured on php-src 8.5.6: a write mid-stream replaces bytes rather than inserting, and only
/// a write that runs past the end extends the length. `"abcdef"` rewound and written `"XY"`
/// reads back `"XYcdef"`, which is what the length update below preserves.
fn rt_memstream_write() -> String {
    r#"(func $__rt_memstream_write (param $h i32) (param $ptr i32) (param $len i64) (result i64)
  (local $d i32) (local $pos i64) (local $end i64) (local $buf i32) (local $i i64) (local $newpos i64)
  (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
  ;; A read-only stream refuses every write, an empty one included: php answers `false`, which
  ;; this helper spells -1 for its callers to box.
  (if (i32.eq (i32.load (i32.add (local.get $d) (i32.const 60))) (i32.const 2))
    (then (return (i64.const -1))))
  (if (i64.le_s (local.get $len) (i64.const 0))
    (then (return (i64.const 0))))
  ;; A closed stream (capacity -1, no buffer) reached through a copy of its handle writes
  ;; nothing: growing would read -1 as a huge capacity and copy to address 0.
  (if (i64.eq (i64.load (i32.add (local.get $d) (i32.const 8))) (i64.const -1))
    (then (return (i64.const 0))))
  (local.set $pos (i64.load (i32.add (local.get $d) (i32.const 16))))
  (local.set $newpos (i64.add (local.get $pos) (local.get $len)))
  ;; Append mode writes the BYTES at the end whatever the position is, but php's stream layer
  ;; still advances its own position from where it was (measured: read one byte of "abc" in
  ;; `a+`, append "X", and `ftell` is 2), so the position moves by `len` from the old one.
  (if (i32.eq (i32.load (i32.add (local.get $d) (i32.const 60))) (i32.const 1))
    (then (local.set $pos (i64.load (local.get $d)))))
  (local.set $end (i64.add (local.get $pos) (local.get $len)))
  (call $__rt_memstream_grow (local.get $d) (local.get $end))
  (local.set $buf (i32.load (i32.add (local.get $d) (i32.const 32))))
  ;; A write past the end (after an `fseek` beyond it) leaves a gap php reads back as zeros.
  ;; The buffer may be a reused block, and growing copies only the live bytes, so the gap is
  ;; whatever that block held before unless it is cleared here.
  (if (i64.gt_u (local.get $pos) (i64.load (local.get $d)))
    (then (memory.fill
      (i32.add (local.get $buf) (i32.wrap_i64 (i64.load (local.get $d))))
      (i32.const 0)
      (i32.wrap_i64 (i64.sub (local.get $pos) (i64.load (local.get $d)))))))
  (memory.copy
    (i32.add (local.get $buf) (i32.wrap_i64 (local.get $pos)))
    (local.get $ptr)
    (i32.wrap_i64 (local.get $len)))
  (i64.store (i32.add (local.get $d) (i32.const 16))
    (select (local.get $newpos) (local.get $end)
      (i32.eq (i32.load (i32.add (local.get $d) (i32.const 60))) (i32.const 1))))
  ;; Only a write PAST the end extends the stream; one inside it leaves the length alone.
  (if (i64.gt_u (local.get $end) (i64.load (local.get $d)))
    (then (i64.store (local.get $d) (local.get $end))))
  (local.get $len))
"#
    .to_string()
}

/// `__rt_memstream_read`: reads at most `count` bytes from the current position.
///
/// What sets the end-of-file flag is a read that ASKED for more than was there, not one that
/// merely finished at the end. Measured on php-src 8.5.6 across four cases: requesting 5 of 5
/// leaves `feof` FALSE, requesting 100 of 6 sets it TRUE even though 6 bytes came back, and
/// requesting anything at all of 0 sets it. So the test is `count > available`, and the flag is
/// stored rather than derived from the position, which could not tell those cases apart.
fn rt_memstream_read() -> String {
    r#"(func $__rt_memstream_read (param $h i32) (param $count i64) (result i32 i64)
  (local $d i32) (local $pos i64) (local $len i64) (local $avail i64) (local $take i64)
  (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
  (local.set $pos (i64.load (i32.add (local.get $d) (i32.const 16))))
  (local.set $len (i64.load (local.get $d)))
  (local.set $avail (select
    (i64.sub (local.get $len) (local.get $pos))
    (i64.const 0)
    (i64.lt_u (local.get $pos) (local.get $len))))
  (local.set $take (select (local.get $count) (local.get $avail)
    (i64.lt_u (local.get $count) (local.get $avail))))
  (if (i64.le_s (local.get $count) (i64.const 0))
    (then (local.set $take (i64.const 0))))
  (if (i64.gt_u (local.get $count) (local.get $avail))             ;; asked past the end
    (then (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 1))))
  (if (i64.eqz (local.get $take))
    (then (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  (i64.store (i32.add (local.get $d) (i32.const 16)) (i64.add (local.get $pos) (local.get $take)))
  (call $__rt_str_persist
    (i32.add (i32.load (i32.add (local.get $d) (i32.const 32))) (i32.wrap_i64 (local.get $pos)))
    (local.get $take)))
"#
    .to_string()
}

/// The bit that marks a stream handle as an IN-MEMORY stream rather than a WASI fd.
///
/// `php://memory` and `php://temp` have no host file behind them, so their handles cannot be
/// fds — but they flow through the same resource-tagged cell every other stream does, and the
/// same `fread`/`fwrite`/`fclose` call sites. Encoding the descriptor's ADDRESS with this bit
/// set makes the two spaces disjoint without a side table: a WASI fd is a small non-negative
/// integer. A heap address CAN reach 2^30 — the allocator grows towards 4 GiB — so
/// `__rt_memstream_new` refuses a descriptor placed there rather than hand out a handle that
/// decodes to another address.
const MEMSTREAM_FLAG: u32 = 0x4000_0000;

/// `__rt_memstream_new`: opens an empty in-memory stream and answers its handle.
///
/// The descriptor is a fixed 56-byte block whose ADDRESS is the handle, so it never moves; the
/// bytes live in a separate block it points at, which is what lets a write grow the stream
/// without invalidating the handle the script is holding.
///
/// Layout: +0 length, +8 capacity, +16 position, +24 eof, +32 buffer pointer, then the
/// `stream_get_meta_data` record — +40/+44 the fopen MODE as a persisted-string (ptr, len)
/// and +48/+52 the URI the stream was opened as, both released by `__rt_memstream_close` —
/// and finally +56 the chunk size `stream_set_chunk_size()` records (a 32-bit field) and +60
/// the access mode: 0 read/write, 1 append, 2 read-only. An in-memory stream has
/// no fd, so it cannot use the per-fd table `streamtune` keeps for real ones.
fn rt_memstream_new() -> String {
    format!(
        r#"(func $__rt_memstream_new (param $mode i32) (param $mode_len i64) (param $uri i32) (param $uri_len i64) (result i32)
  (local $d i32) (local $p i32) (local $l i64) (local $i i32) (local $c i32) (local $access i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 64)))
  ;; The handle is the address with bit 30 set, and every reader masks that bit off, so a
  ;; descriptor placed at or above 1 GiB would decode to another address. Past that point
  ;; the stream cannot be represented, which is an out-of-memory condition, not a wrong one.
  (if (i32.ge_u (local.get $d) (i32.const {flag}))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:memstream-descriptor-address
  (i64.store (local.get $d) (i64.const 0))                        ;; length
  (i64.store (i32.add (local.get $d) (i32.const 8)) (i64.const 0)) ;; capacity
  (i64.store (i32.add (local.get $d) (i32.const 16)) (i64.const 0)) ;; position
  (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 0)) ;; eof
  (i64.store (i32.add (local.get $d) (i32.const 56)) (i64.const 0)) ;; chunk size: 0 = php's default
  ;; The access mode, by php's own rule (`php_stream_mode_from_str`): any `a` appends, else any
  ;; `w` or `+` is plain read/write, and what is left (`r`, `x`, `c` alone) is READ-ONLY.
  ;; Stored in the high half of the chunk-size word, which only its low 32 bits use.
  (local.set $access (i32.const 2))
  (block $scanned (loop $scan
    (br_if $scanned (i64.ge_u (i64.extend_i32_u (local.get $i)) (local.get $mode_len)))
    (local.set $c (i32.load8_u (i32.add (local.get $mode) (local.get $i))))
    (if (i32.eq (local.get $c) (i32.const 97))
      (then (local.set $access (i32.const 1)) (br $scanned)))
    (if (i32.or (i32.eq (local.get $c) (i32.const 119)) (i32.eq (local.get $c) (i32.const 43)))
      (then (local.set $access (i32.const 0))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (i32.store (i32.add (local.get $d) (i32.const 60)) (local.get $access))
  (i32.store (i32.add (local.get $d) (i32.const 32)) (i32.const 0)) ;; no buffer yet
  (call $__rt_str_persist (local.get $mode) (local.get $mode_len))
  (local.set $l)
  (local.set $p)
  (i32.store (i32.add (local.get $d) (i32.const 40)) (local.get $p))               ;; mode ptr
  (i32.store (i32.add (local.get $d) (i32.const 44)) (i32.wrap_i64 (local.get $l))) ;; mode len
  (call $__rt_str_persist (local.get $uri) (local.get $uri_len))
  (local.set $l)
  (local.set $p)
  (i32.store (i32.add (local.get $d) (i32.const 48)) (local.get $p))               ;; uri ptr
  (i32.store (i32.add (local.get $d) (i32.const 52)) (i32.wrap_i64 (local.get $l))) ;; uri len
  (i32.or (local.get $d) (i32.const {flag})))
"#,
        flag = MEMSTREAM_FLAG
    )
}

/// `__rt_memstream_grow`: ensures the stream's buffer holds at least `want` bytes.
///
/// Capacity doubles from a 32-byte floor, so a write loop costs a logarithmic number of copies
/// rather than one per call. The old bytes are copied and the old block freed; only the buffer
/// pointer in the descriptor changes, which is why the handle stays valid.
///
/// Every size is validated in widened (i64) arithmetic before it is narrowed to the allocator's
/// i32: a request past the ceiling is the named "Allowed memory size exhausted" fatal, never a
/// wrapped, tiny allocation that the copy loop then writes past.
const RT_MEMSTREAM_GROW: &str = r#"(func $__rt_memstream_grow (param $d i32) (param $want i64)
  (local $cap i64) (local $old i32) (local $new i32)
  (local.set $cap (i64.load (i32.add (local.get $d) (i32.const 8))))
  (if (i64.le_u (local.get $want) (local.get $cap))
    (then (return)))
  ;; `want` is position + length and a seek can park the position anywhere in i64, so the sum
  ;; may have wrapped negative; read UNSIGNED, a wrapped sum is far above the ceiling too. The
  ;; ceiling is the allocator's largest payload, and past it the stream is out of memory.
  (if (i64.gt_u (local.get $want) (i64.const 4294900720))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:memstream-want-overflow
  (if (i64.lt_u (local.get $cap) (i64.const 32))
    (then (local.set $cap (i64.const 32))))
  (block $sized (loop $double
    (br_if $sized (i64.ge_u (local.get $cap) (local.get $want)))
    (local.set $cap (i64.shl (local.get $cap) (i64.const 1)))
    (br $double)))
  ;; the doubled capacity may overshoot the ceiling even though `want` fits under it
  (if (i64.gt_u (local.get $cap) (i64.const 4294900720))
    (then (local.set $cap (i64.const 4294900720))))
  (local.set $old (i32.load (i32.add (local.get $d) (i32.const 32))))
  (local.set $new (call $__rt_heap_alloc
    (call $__rt_checked_layout (local.get $cap) (i64.const 1) (i64.const 0))))
  (if (local.get $old)
    (then (memory.copy (local.get $new) (local.get $old)
      (i32.wrap_i64 (i64.load (local.get $d))))))                        ;; only the LIVE bytes
  (if (local.get $old)
    (then (call $__rt_heap_free (local.get $old))))
  (i32.store (i32.add (local.get $d) (i32.const 32)) (local.get $new))
  (i64.store (i32.add (local.get $d) (i32.const 8)) (local.get $cap)))
"#;

/// `__rt_memstream_tell`: the stream's current position.
const RT_MEMSTREAM_TELL: &str = r#"(func $__rt_memstream_tell (param $h i32) (result i64)
  (i64.load (i32.add (i32.and (local.get $h) (i32.const 1073741823)) (i32.const 16))))
"#;

/// `__rt_memstream_seek`: moves the position and clears the end-of-file flag.
///
/// `rewind` is this with offset 0. Clearing `eof` is what php-src does: after `rewind`, `feof`
/// answers false again even though a read had previously hit the end.
const RT_MEMSTREAM_SEEK: &str = r#"(func $__rt_memstream_seek (param $h i32) (param $off i64)
  (local $d i32)
  (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
  (i64.store (i32.add (local.get $d) (i32.const 16)) (local.get $off))
  (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 0)))
"#;

/// `__rt_memstream_eof`: whether a read has already found nothing.
///
/// Measured on php-src 8.5.6, this is NOT "the position is at the end": reading exactly the last
/// byte leaves `feof` FALSE, and only the next read — the one that finds nothing — sets it. So
/// the flag is written by the read, never derived from the position here.
const RT_MEMSTREAM_EOF: &str = r#"(func $__rt_memstream_eof (param $h i32) (result i64)
  (i64.load (i32.add (i32.and (local.get $h) (i32.const 1073741823)) (i32.const 24))))
"#;

/// `__rt_memstream_close`: frees the buffer and the metadata strings, answering PHP's true.
///
/// The DESCRIPTOR is kept, emptied and marked closed with a capacity of -1: the handle is its
/// address, and copies of it outlive the box `fclose` was given — an array holds the raw handle
/// and boxes it afresh on every read. Freeing it let a second `fclose` through one of those
/// copies free the block again, and any other call read whatever reused it. Its 64 bytes are
/// the price of every copy answering "closed".
///
/// MEASURED COST: 64 bytes of descriptor plus the allocator header, about 80 bytes per
/// `fopen("php://temp")`/`fclose` pair (80000 pairs grow memory by 97 pages), so a loop that opens
/// and closes memory streams grows linearly and slowly instead of staying flat. Freeing the
/// descriptor needs a refcount shared by every holder of the handle — the boxes, the raw copies an
/// array keeps — and those copies are made and dropped by the generic Mixed runtime, which does
/// not count resources; the leak is the deliberate trade against the use-after-free it replaced.
const RT_MEMSTREAM_CLOSE: &str = r#"(func $__rt_memstream_close (param $h i32) (result i64)
  (local $d i32) (local $buf i32)
  (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
  (local.set $buf (i32.load (i32.add (local.get $d) (i32.const 32))))
  (if (local.get $buf)
    (then (call $__rt_heap_free (local.get $buf))))
  (call $__rt_decref_any (i32.load (i32.add (local.get $d) (i32.const 40))))  ;; mode string (null-safe)
  (call $__rt_decref_any (i32.load (i32.add (local.get $d) (i32.const 48))))  ;; uri string (null-safe)
  (i32.store (i32.add (local.get $d) (i32.const 32)) (i32.const 0))           ;; no buffer
  (i32.store (i32.add (local.get $d) (i32.const 40)) (i32.const 0))           ;; no mode
  (i32.store (i32.add (local.get $d) (i32.const 48)) (i32.const 0))           ;; no uri
  (i32.store (i32.add (local.get $d) (i32.const 44)) (i32.const 0))           ;; and no mode/uri LENGTHS:
  (i32.store (i32.add (local.get $d) (i32.const 52)) (i32.const 0))           ;; a stale length over a null pointer read garbage
  (i64.store (local.get $d) (i64.const 0))                                    ;; empty
  (i64.store (i32.add (local.get $d) (i32.const 8)) (i64.const -1))           ;; closed
  (i64.const 1))
"#;

/// `__rt_stream_new`: gives one `fopen`/`tmpfile`/`opendir`/standard-stream open its own record.
///
/// The record is eight bytes — the host fd at +0 (`FD_OUTPUT` for `php://output`, 0-2 for the
/// standard streams) and the closed flag at +4 — and the resource payload is its address with
/// a high word of 1 (bits 32 and up; memory-stream handles never reach bit 32). The identity is per OPEN, not per fd: WASI recycles fd numbers
/// and `php://output` has only one pseudo-fd, so a handle that was just the fd let a stale copy of
/// a closed stream come back to life when a later open reused the number, and let closing one
/// `php://output` stream close every other.
///
/// The record is never freed by `fclose`: copies of the handle outlive the box `fclose` was given
/// (an array keeps the raw payload and boxes it afresh on every read), and the generic Mixed
/// runtime does not count resources, so it stays behind as the closed tombstone every copy reads.
/// That is the memory-stream trade, at 8 bytes plus the allocator header per open.
const RT_STREAM_NEW: &str = r#"(func $__rt_stream_new (param $fd i32) (result i64)
  (local $r i32)
  (local.set $r (call $__rt_heap_alloc (i32.const 8)))
  (i32.store (local.get $r) (local.get $fd))
  (i32.store offset=4 (local.get $r) (i32.const 0))               ;; open
  (i64.or (i64.shl (i64.const 1) (i64.const 32)) (i64.extend_i32_u (local.get $r))))
"#;

/// `__rt_handle_fd`: the host fd (or memory-stream handle) a resource payload stands for.
///
/// A record handle answers its fd, or -1 once its record is closed, so a stale copy never reaches
/// a number the host has since handed to another stream. Any other payload — a memory-stream
/// handle, a predefined `STDIN`/`STDOUT`/`STDERR` fd, the -1 closed stamp — is its own fd word.
const RT_HANDLE_FD: &str = r#"(func $__rt_handle_fd (param $p i64) (result i32)
  (local $r i32)
  (if (i64.eq (i64.shr_u (local.get $p) (i64.const 32)) (i64.const 1))   ;; high word 1: a record
    (then
      (local.set $r (i32.wrap_i64 (local.get $p)))
      (if (i32.load offset=4 (local.get $r)) (then (return (i32.const -1))))
      (return (i32.load (local.get $r)))))
  (i32.wrap_i64 (local.get $p)))
"#;

/// `__rt_stream_record_close`: marks the stream behind a resource payload closed, for every copy.
///
/// A record handle gets its closed flag set; a predefined `STDIN`/`STDOUT`/`STDERR` payload gets
/// its bit in `STD_CLOSED`. Anything else (a memory stream, which closes through its own
/// descriptor, or a payload already stamped -1) is left alone. Host fds are not touched here.
fn rt_stream_record_close() -> String {
    format!(
        r#"(func $__rt_stream_record_close (param $p i64)
  (if (i64.eq (i64.shr_u (local.get $p) (i64.const 32)) (i64.const 1))
    (then
      (i32.store offset=4 (i32.wrap_i64 (local.get $p)) (i32.const 1))
      (return)))
  (if (i64.lt_u (local.get $p) (i64.const 3))                     ;; STDIN, STDOUT, STDERR
    (then
      (i32.store8 (i32.add (global.get $__float_scratch) (i32.const {std}))
        (i32.or
          (i32.load8_u (i32.add (global.get $__float_scratch) (i32.const {std})))
          (i32.shl (i32.const 1) (i32.wrap_i64 (local.get $p))))))))
"#,
        std = STD_CLOSED
    )
}

/// `__rt_stream_cell_close`: `__rt_stream_record_close` of a boxed resource cell's payload.
const RT_STREAM_CELL_CLOSE: &str = r#"(func $__rt_stream_cell_close (param $cell i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (if (i32.eqz (local.get $cell)) (then (return)))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 9))
    (then (call $__rt_stream_record_close (local.get $lo)))))
"#;

/// `__rt_stream_cell_release`: releases a handle cell the runtime opened for its own use.
///
/// `file_get_contents`, `file_put_contents` and a `tmpfile` name probe open a stream nobody else
/// can hold a copy of, so its record is freed together with the cell, where a script-visible
/// handle keeps its record as the closed tombstone.
const RT_STREAM_CELL_RELEASE: &str = r#"(func $__rt_stream_cell_release (param $cell i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (if (i32.eqz (local.get $cell)) (then (return)))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i32.and (i64.eq (local.get $tag) (i64.const 9))
               (i64.eq (i64.shr_u (local.get $lo) (i64.const 32)) (i64.const 1)))
    (then (call $__rt_heap_free (i32.wrap_i64 (local.get $lo)))))
  (call $__rt_decref_any (local.get $cell)))
"#;

/// `__rt_handle_closed`: whether a resource payload names a stream that has been closed.
///
/// The box `fclose` received carries -1, but a copy of the handle (re-boxed from an array, or a
/// raw one) does not, so the state is also kept with the STREAM: a record handle's closed flag
/// (one record per open, so a host that recycles the fd number cannot revive a stale copy), a
/// memory-stream descriptor's capacity of -1, or a predefined `STDIN`/`STDOUT`/`STDERR`'s bit in
/// `STD_CLOSED`, which `fclose` sets for the constant as php closes it.
fn rt_handle_closed() -> String {
    format!(
        r#"(func $__rt_handle_closed (param $p i64) (result i32)
  (local $h i32)
  (if (i64.eq (local.get $p) (i64.const -1))
    (then (return (i32.const 1))))
  (if (i64.eq (i64.shr_u (local.get $p) (i64.const 32)) (i64.const 1))   ;; a per-open record
    (then (return (i32.ne (i32.load offset=4 (i32.wrap_i64 (local.get $p))) (i32.const 0)))))
  (if (i64.lt_u (local.get $p) (i64.const 3))                     ;; STDIN, STDOUT, STDERR
    (then (return (i32.ne (i32.and
      (i32.load8_u (i32.add (global.get $__float_scratch) (i32.const {std})))
      (i32.shl (i32.const 1) (i32.wrap_i64 (local.get $p)))) (i32.const 0)))))
  (local.set $h (i32.wrap_i64 (local.get $p)))
  (if (i32.and (local.get $h) (i32.const {flag}))
    (then (return (i64.eq
      (i64.load offset=8 (i32.and (local.get $h) (i32.const 1073741823)))
      (i64.const -1)))))
  (i32.const 0))
"#,
        flag = MEMSTREAM_FLAG,
        std = STD_CLOSED
    )
}

/// `__rt_box_closed`: whether a boxed cell is a resource handle that has been closed.
///
/// A cell that is not a resource (the `false` a failed `fopen` answers, a stray int) is NOT
/// "closed" — php reports that as a different `TypeError`, which this target does not model — so
/// only tag 9 is asked, and its payload goes through `__rt_handle_closed`.
const RT_BOX_CLOSED: &str = r#"(func $__rt_box_closed (param $cell i32) (result i32)
  (if (i32.eqz (local.get $cell))
    (then (return (i32.const 0))))
  (if (i64.ne (i64.load (local.get $cell)) (i64.const 9))         ;; not a resource
    (then (return (i32.const 0))))
  (call $__rt_handle_closed (i64.load offset=8 (local.get $cell))))
"#;

/// `__rt_feof`: PHP's `feof` for either kind of stream.
///
/// An in-memory stream carries the flag its own reads set; a real fd carries the same
/// read-set flag in the `FD_EOF_FLAGS` table. Neither is a position comparison: the
/// stream-lines corpus example needs the read AFTER the last line to run — it answers
/// `false` and only then does `feof` turn true, exactly php's fourth `event:` line.
const RT_FEOF: &str = r#"(func $__rt_feof (param $h i32) (result i64)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (i64.const 1))))                                ;; not a stream at all
  (if (i32.and (local.get $h) (i32.const 1073741824))
    (then (return (call $__rt_memstream_eof (local.get $h)))))
  (call $__rt_fd_eof_get (local.get $h)))
"#;

/// `__rt_ftell`: PHP's `ftell`, for either stream kind.
///
/// WASI has no `fd_tell`, but a ZERO-length seek from the current position answers the same
/// question and is what the position is defined as. Measured on php-src 8.5.6: after writing
/// ten bytes, `ftell` is 10 — so answering a constant 0 for a real fd, as this used to, was
/// wrong for every file that had been read or written.
///
/// A `php://output` handle has no host position at all; php counts the bytes written through it
/// (measured: 5 after `fwrite($h, "12345")`), and so does the counter `__rt_fwrite` keeps. That
/// counter is shared by every `php://output` handle, where php keeps one per handle; `fopen`
/// resets it, so the single-handle case is exact.
fn rt_ftell() -> String {
    format!(
        r#"(func $__rt_ftell (param $h i32) (result i64)
  (if (i32.and (local.get $h) (i32.const 1073741824))
    (then (return (call $__rt_memstream_tell (local.get $h)))))
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.eq (local.get $h) (i32.const {output}))
    (then (return (i64.load (i32.add (global.get $__float_scratch) (i32.const {pos}))))))
  (if (i32.ne (call $wasi_fd_seek (local.get $h) (i64.const 0) (i32.const 1)
        (i32.add (global.get $__float_scratch) (i32.const 12352)))
      (i32.const 0))
    (then (return (i64.const 0))))
  (i64.load (i32.add (global.get $__float_scratch) (i32.const 12352))))
"#,
        output = FD_OUTPUT,
        pos = OUTPUT_POS
    )
}

/// `__rt_fseek`: PHP's `fseek`, answering 0 on success and -1 on failure.
///
/// PHP's `SEEK_SET`/`SEEK_CUR`/`SEEK_END` are 0/1/2, which are the same numbers WASI's
/// `fd_seek` uses, so the whence passes straight through for a real fd.
///
/// Measured on php-src 8.5.6, for the two cases that are not obvious:
///   * a NEGATIVE absolute position fails (`-1`) and leaves the position UNTOUCHED;
///   * seeking PAST the end succeeds — `fseek($m, 50)` answers 0 and `ftell` is then 50 —
///     and the read there yields `""`.
/// A successful seek also CLEARS eof, which `__rt_memstream_seek` already does.
/// The whence arrives as an `i64` because the lowering passes PHP ints as i64; `fd_seek`
/// takes it as an i32, so it is narrowed at the call rather than in the signature.
const RT_FSEEK: &str = r#"(func $__rt_fseek (param $h i32) (param $off i64) (param $whence i64) (result i64)
  (local $d i32) (local $target i64)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (i64.const -1))))                                 ;; not a stream at all
  (if (i32.and (local.get $h) (i32.const 1073741824))
    (then
      ;; php refuses a whence that is none of SEEK_SET/CUR/END and leaves the position alone.
      (if (i64.gt_u (local.get $whence) (i64.const 2))
        (then (return (i64.const -1))))
      (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
      (local.set $target
        (if (result i64) (i64.eq (local.get $whence) (i64.const 1))
          (then (i64.add (i64.load (i32.add (local.get $d) (i32.const 16))) (local.get $off)))
          (else (if (result i64) (i64.eq (local.get $whence) (i64.const 2))
            (then (i64.add (i64.load (local.get $d)) (local.get $off)))
            (else (local.get $off))))))
      (if (i64.lt_s (local.get $target) (i64.const 0))
        (then (return (i64.const -1))))                             ;; position left untouched
      (call $__rt_memstream_seek (local.get $h) (local.get $target))
      (return (i64.const 0))))
  (if (i32.ne (call $wasi_fd_seek (local.get $h) (local.get $off)
        (i32.wrap_i64 (local.get $whence))
        (i32.add (global.get $__float_scratch) (i32.const 12352)))
      (i32.const 0))
    (then (return (i64.const -1))))
  (call $__rt_fd_eof_clear (local.get $h))                          ;; a successful seek clears eof
  (i64.const 0))
"#;

/// `__rt_rewind`: PHP's `rewind`, seeking either kind of stream back to the start.
const RT_REWIND: &str = r#"(func $__rt_rewind (param $h i32) (result i64)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.and (local.get $h) (i32.const 1073741824))
    (then
      (call $__rt_memstream_seek (local.get $h) (i64.const 0))
      (return (i64.const 1))))
  (if (i32.ne (call $wasi_fd_seek (local.get $h) (i64.const 0) (i32.const 0)
        (i32.add (global.get $__float_scratch) (i32.const 12352)))
      (i32.const 0))
    (then (return (i64.const 0))))
  (call $__rt_fd_eof_clear (local.get $h))                          ;; a successful seek clears eof
  (i64.const 1))
"#;

/// `__rt_fopen_failed`: the boxed `false` every unopenable path answers, warning first.
///
/// The flag exists because `file_get_contents` opens through `__rt_fopen` and must emit its OWN
/// message rather than `fopen`'s — php-src and the native backend both name the function the
/// script actually called.
const RT_FOPEN_FAILED: &str = r#"(func $__rt_fopen_failed (param $warn i32) (result i32)
  (if (local.get $warn)
    (then (call $__rt_warn_fopen_failed)))
  (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))
"#;

/// `__rt_std_stream_fd`: the fd behind `php://stdout`, `php://stderr`, `php://stdin` or
/// `php://output`, and -1 for anything else.
///
/// These four are the part of the `php://` wrapper that is not a stream implementation at all —
/// they name the three fds the process already has, which is why they work here while
/// `php://memory` and `php://temp` do not. `php://output` is the exception: it is NOT stdout but
/// the output buffer, so it answers `FD_OUTPUT`, a number no host fd can take, and `__rt_fwrite`
/// routes that handle through `__rt_stdout_write` while `php://stdout` keeps writing directly.
///
/// The comparison is open-coded rather than done against a data segment so this stays
/// independent of the module's static-data layout, the same reason `__rt_echo_array_word`
/// builds its five bytes in scratch.
const RT_STD_STREAM_FD: &str = r#"(func $__rt_std_stream_fd (param $path i32) (param $len i64) (result i32)
  (local $c i32)
  (if (i32.and (i64.ne (local.get $len) (i64.const 11)) (i64.ne (local.get $len) (i64.const 12)))
    (then (return (i32.const -1))))                              ;; no php:// name is another length
  (if (i32.eqz (i32.and (i32.and
        (i32.and (i32.eq (i32.load8_u (local.get $path)) (i32.const 112))                       ;; 'p'
                 (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 1))) (i32.const 104))) ;; 'h'
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 2))) (i32.const 112))  ;; 'p'
                 (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 3))) (i32.const 58)))) ;; ':'
        (i32.and (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 4))) (i32.const 47))   ;; '/'
                 (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 5))) (i32.const 47))))) ;; '/'
    (then (return (i32.const -1))))
  (local.set $c (i32.load8_u (i32.add (local.get $path) (i32.const 6))))
  (if (i32.eq (local.get $c) (i32.const 111))                    ;; "output": the output buffer
    (then (return (select (i32.const 1023) (i32.const -1)           ;; FD_OUTPUT, not stdout's fd
      (i32.and (i64.eq (local.get $len) (i64.const 12))
               (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 7))) (i32.const 117)))))))
  (if (i32.ne (local.get $c) (i32.const 115))                    ;; every other name starts "std"
    (then (return (i32.const -1))))
  (if (i32.eqz (i32.and
        (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 7))) (i32.const 116))   ;; 't'
        (i32.eq (i32.load8_u (i32.add (local.get $path) (i32.const 8))) (i32.const 100)))) ;; 'd'
    (then (return (i32.const -1))))
  (local.set $c (i32.load8_u (i32.add (local.get $path) (i32.const 9))))
  (if (i64.eq (local.get $len) (i64.const 11))
    (then (return (select (i32.const 0) (i32.const -1)           ;; "stdin"
      (i32.eq (local.get $c) (i32.const 105))))))                ;; 'i'
  (if (i32.eq (local.get $c) (i32.const 111))                    ;; "stdout"
    (then (return (i32.const 1))))
  (if (i32.eq (local.get $c) (i32.const 101))                    ;; "stderr"
    (then (return (i32.const 2))))
  (i32.const -1))
"#;

/// `__rt_wasi_dirfd`: the fd of the first directory the host preopened, or -1.
///
/// WASI hands a module its filesystem authority as preopened fds starting at 3, and a
/// path is only meaningful relative to one of them. Probing `fd_prestat_get` upward
/// finds the first; a host that preopened nothing answers `EBADF` for every probe and
/// this answers -1, which every caller turns into PHP's `false`.
///
/// The scan stops at 32: a host that preopened more than that has directories this
/// cannot reach, which is a coverage limit rather than a wrong answer.
fn rt_wasi_dirfd() -> String {
    format!(
        r#"(func $__rt_wasi_dirfd (result i32)
  (local $fd i32)
  (local.set $fd (i32.const 3))                                  ;; preopens start at fd 3
  (block $done (loop $next
    (br_if $done (i32.ge_u (local.get $fd) (i32.const 32)))
    (if (i32.eqz (call $wasi_fd_prestat_get
          (local.get $fd)
          (i32.add (global.get $__float_scratch) (i32.const {prestat}))))
      (then (return (local.get $fd))))                           ;; first preopen wins
    (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
    (br $next)))
  (i32.const -1))
"#,
        prestat = IO_SCRATCH + 0x20
    )
}

/// `__rt_fopen`: opens a path in a PHP mode and boxes the fd as a resource.
///
/// The mode's FIRST byte picks the open flags — `r` opens an existing file, `w`
/// creates and truncates, `a` creates and appends, `x` creates and fails if the file
/// exists, `c` creates without truncating — and a `+` anywhere after it adds the
/// other direction. The `b` and `t` suffixes name a line-ending translation this
/// target does not perform, so they are read and ignored, which is what they mean on
/// every platform PHP treats as binary-safe.
///
/// Rights are requested per direction rather than generously: `path_open` fails with
/// `ENOTCAPABLE` when a caller asks for more than the preopen holds, so asking for
/// what the mode actually needs is what makes a read-only preopen still serve `r`.
///
/// Any failure answers boxed `false`, which is what PHP answers for a path it cannot open,
/// after the warning the native backend emits there. `warn` is zero for the internal opens
/// `file_get_contents` and `file_put_contents` perform, which name themselves instead.
fn rt_fopen() -> String {
    format!(
        r#"(func $__rt_fopen (param $path i32) (param $path_len i64) (param $mode i32) (param $mode_len i64) (param $warn i32) (result i32)
  (local $dirfd i32)
  (local $first i32)
  (local $plus i32)
  (local $i i64)
  (local $oflags i32)
  (local $rights i64)
  (local $fdflags i32)
  (local $readable i32)
  (local $writable i32)
  (local.set $first (call $__rt_std_stream_fd (local.get $path) (local.get $path_len)))
  (if (i32.ge_s (local.get $first) (i32.const 0))                ;; a standard stream needs no path
    (then
      (if (i32.eq (local.get $first) (i32.const {output}))       ;; php://output: php reports mode "wb"
        (then
          (i32.store16 (i32.add (global.get $__float_scratch) (i32.const {modebuf})) (i32.const 25207)) ;; "wb"
          (i64.store (i32.add (global.get $__float_scratch) (i32.const {pos})) (i64.const 0))             ;; a fresh handle starts at position 0
          (call $__rt_fd_meta_record (local.get $first)
            (i32.add (global.get $__float_scratch) (i32.const {modebuf})) (i64.const 2)
            (local.get $path) (local.get $path_len))
          (return (call $__rt_mixed_from_value (i64.const 9)
            (call $__rt_stream_new (local.get $first)) (i64.const 0)))))
      (call $__rt_fd_meta_record (local.get $first)
        (local.get $mode) (local.get $mode_len)
        (local.get $path) (local.get $path_len))
      (return (call $__rt_mixed_from_value (i64.const 9)
        (call $__rt_stream_new (local.get $first)) (i64.const 0)))))
  ;; `php://memory` and `php://temp` have no host file behind them at all, so they are opened
  ;; before the preopen probe: they work with no filesystem authority whatsoever.
  (if (call $__rt_is_memstream_path (local.get $path) (local.get $path_len))
    (then (return (call $__rt_mixed_from_value (i64.const 9)
      (i64.extend_i32_u (call $__rt_memstream_new
        (local.get $mode) (local.get $mode_len)
        (local.get $path) (local.get $path_len))) (i64.const 0)))))
  ;; A `data:` URI carries its own content, so it is decoded here rather than opened: no
  ;; filesystem authority is involved at all. A malformed one answers php's `false`.
  (if (call $__rt_is_data_uri (local.get $path) (local.get $path_len))
    (then
      (local.set $first (call $__rt_data_uri_open (local.get $path) (local.get $path_len)
                                                  (local.get $mode) (local.get $mode_len)))
      (if (i32.lt_s (local.get $first) (i32.const 0))
        (then (return (call $__rt_fopen_failed (local.get $warn)))))
      (return (call $__rt_mixed_from_value (i64.const 9)
        (i64.extend_i32_u (local.get $first)) (i64.const 0)))))
  (local.set $first (i32.const 0))
  (local.set $dirfd (call $__rt_wasi_dirfd))
  (if (i32.lt_s (local.get $dirfd) (i32.const 0))                ;; no preopen -> no filesystem
    (then (return (call $__rt_fopen_failed (local.get $warn)))))
  (if (i64.eqz (local.get $mode_len))                            ;; an empty mode opens nothing
    (then (return (call $__rt_fopen_failed (local.get $warn)))))
  (local.set $first (i32.load8_u (local.get $mode)))
  (block $scanned (loop $scan                                    ;; is there a '+' after the first byte?
    (br_if $scanned (i64.ge_u (local.get $i) (local.get $mode_len)))
    (if (i32.eq (i32.load8_u (i32.add (local.get $mode) (i32.wrap_i64 (local.get $i)))) (i32.const 43))
      (then (local.set $plus (i32.const 1)) (br $scanned)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan)))
  (if (i32.eq (local.get $first) (i32.const 114))                ;; 'r': read an existing file
    (then (local.set $readable (i32.const 1)) (local.set $writable (local.get $plus)))
    (else (if (i32.eq (local.get $first) (i32.const 119))         ;; 'w': create, truncate
      (then
        (local.set $writable (i32.const 1)) (local.set $readable (local.get $plus))
        (local.set $oflags (i32.const 9)))                        ;; O_CREAT | O_TRUNC
      (else (if (i32.eq (local.get $first) (i32.const 97))         ;; 'a': create, append
        (then
          (local.set $writable (i32.const 1)) (local.set $readable (local.get $plus))
          (local.set $oflags (i32.const 1))                        ;; O_CREAT
          (local.set $fdflags (i32.const 1)))                      ;; FDFLAG_APPEND
        (else (if (i32.eq (local.get $first) (i32.const 120))       ;; 'x': create, fail if present
          (then
            (local.set $writable (i32.const 1)) (local.set $readable (local.get $plus))
            (local.set $oflags (i32.const 5)))                      ;; O_CREAT | O_EXCL
          (else (if (i32.eq (local.get $first) (i32.const 99))       ;; 'c': create, keep contents
            (then
              (local.set $writable (i32.const 1)) (local.set $readable (local.get $plus))
              (local.set $oflags (i32.const 1)))                     ;; O_CREAT
            (else                                                    ;; anything else is not a PHP mode
              (return (call $__rt_fopen_failed (local.get $warn)))))))))))))
  (local.set $rights (i64.const 2097188))                         ;; FD_SEEK | FD_TELL | FD_FILESTAT_GET
  (if (local.get $readable)
    (then (local.set $rights (i64.or (local.get $rights) (i64.const 2)))))       ;; FD_READ
  (if (local.get $writable)
    (then (local.set $rights (i64.or (local.get $rights) (i64.const 4194369))))) ;; FD_WRITE | FD_DATASYNC | FD_FILESTAT_SET_SIZE
  (if (i32.ne (call $wasi_path_open
        (local.get $dirfd)
        (i32.const 1)                                             ;; LOOKUP_SYMLINK_FOLLOW
        (local.get $path)
        (i32.wrap_i64 (local.get $path_len))
        (local.get $oflags)
        (local.get $rights)
        (local.get $rights)                                       ;; inheriting: same rights
        (local.get $fdflags)
        (i32.add (global.get $__float_scratch) (i32.const {opened})))
      (i32.const 0))
    (then (return (call $__rt_fopen_failed (local.get $warn)))))
  ;; An fd past the tracked range is php's EMFILE: close it and fail like any unopenable path.
  (if (i32.ge_u (i32.load (i32.add (global.get $__float_scratch) (i32.const {opened}))) (i32.const {limit}))
    (then
      (drop (call $wasi_fd_close (i32.load (i32.add (global.get $__float_scratch) (i32.const {opened})))))
      (return (call $__rt_fopen_failed (local.get $warn)))))
  (call $__rt_fd_meta_record
    (i32.load (i32.add (global.get $__float_scratch) (i32.const {opened})))
    (local.get $mode) (local.get $mode_len)
    (local.get $path) (local.get $path_len))
  (call $__rt_mixed_from_value (i64.const 9)                      ;; tag 9 = PHP resource
    (call $__rt_stream_new (i32.load (i32.add (global.get $__float_scratch) (i32.const {opened}))))
    (i64.const 0)))
"#,
        opened = IO_SCRATCH + 0x10,
        modebuf = IO_SCRATCH + 0x30,
        pos = OUTPUT_POS,
        output = FD_OUTPUT,
        limit = FD_HOST_LIMIT
    )
}

/// `__rt_stream_fd`: the WASI fd inside a stream handle, or -1 for anything else.
///
/// `fwrite($notAStream, ...)` is a `TypeError` in php-src; here a non-resource cell —
/// including the `false` a failed `fopen` answers — reads as -1 and every caller
/// answers PHP's failure value. That is the native backend's behaviour too.
fn rt_stream_fd() -> String {
    r#"(func $__rt_stream_fd (param $cell i32) (result i32)
  (local $tag i64)
  (local $lo i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (drop)                                                          ;; discard the high word
  (local.set $lo)
  (local.set $tag)
  (if (i64.ne (local.get $tag) (i64.const 9))                     ;; not a resource
    (then (return (i32.const -1))))
  (call $__rt_handle_fd (local.get $lo)))
"#
    .to_string()
}

/// `__rt_fwrite`: writes a string to a stream and answers the byte count.
/// `__rt_fwrite_boxed`: `fwrite`'s byte count boxed as PHP's `int|false` cell.
///
/// The EIR types the result `mixed` since upstream widened the contract. A failed WASI write
/// reports 0 bytes, as the raw helper always did; a write to a READ-ONLY in-memory stream is
/// php's `false`, which the raw helper spells -1. The cell is what the consumer releases.
const RT_FWRITE_BOXED: &str = r#"(func $__rt_fwrite_boxed (param $fd i32) (param $ptr i32) (param $len i64) (result i32)
  (local $n i64)
  (local.set $n (call $__rt_fwrite (local.get $fd) (local.get $ptr) (local.get $len)))
  (if (i64.lt_s (local.get $n) (i64.const 0))                     ;; a read-only stream: php's false
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_mixed_from_value (i64.const 0) (local.get $n) (i64.const 0)))
"#;

/// Builds `__rt_fwrite`: writes `len` bytes at `ptr` to a descriptor and answers the number
/// written. A negative fd writes nothing, an in-memory stream goes to its buffer, and anything
/// else goes through WASI `fd_write` with the iovec staged in the float scratch.
fn rt_fwrite() -> String {
    format!(
        r#"(func $__rt_fwrite (param $fd i32) (param $ptr i32) (param $len i64) (result i64)
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.and (local.get $fd) (i32.const {flag}))
    (then (return (call $__rt_memstream_write (local.get $fd) (local.get $ptr) (local.get $len)))))
  (if (i32.eq (local.get $fd) (i32.const {output}))               ;; php://output: through the buffers
    (then
      (call $__rt_stdout_write (local.get $ptr) (i32.wrap_i64 (local.get $len)))
      (i64.store (i32.add (global.get $__float_scratch) (i32.const {pos}))
        (i64.add (i64.load (i32.add (global.get $__float_scratch) (i32.const {pos}))) (local.get $len)))
      (return (local.get $len))))
  (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov})) (local.get $ptr))
  (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len})) (i32.wrap_i64 (local.get $len)))
  (if (i32.ne (call $wasi_fd_write
        (local.get $fd)
        (i32.add (global.get $__float_scratch) (i32.const {iov}))
        (i32.const 1)
        (i32.add (global.get $__float_scratch) (i32.const {written})))
      (i32.const 0))
    (then (return (i64.const 0))))
  (i64.extend_i32_u (i32.load (i32.add (global.get $__float_scratch) (i32.const {written})))))
"#,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        written = IO_SCRATCH + 8,
        flag = MEMSTREAM_FLAG,
        output = FD_OUTPUT,
        pos = OUTPUT_POS
    )
}

/// `__rt_fread`: reads at most `count` bytes and answers them as an owned string.
///
/// The bytes land in a heap block sized to the request and are persisted at the length
/// actually read, so a short read — the normal end-of-file case — answers a string of
/// the right length rather than one padded with whatever the block held. The block is
/// freed once its bytes are copied out, so a read loop costs one live string, not one
/// live block per iteration.
fn rt_fread() -> String {
    format!(
        r#"(func $__rt_fread (param $fd i32) (param $count i64) (result i32 i64)
  (local $buf i32)
  (local $read i32)
  (local $out i32)
  (local $out_len i64)
  (if (i32.or (i32.lt_s (local.get $fd) (i32.const 0)) (i64.le_s (local.get $count) (i64.const 0)))
    (then (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  (if (i32.and (local.get $fd) (i32.const {flag}))
    (then (return (call $__rt_memstream_read (local.get $fd) (local.get $count)))))
  ;; php allocates the requested length up front, so a count past what this module can
  ;; allocate is php's memory-limit fatal; narrowing it first read 1 byte for 2^32 + 1.
  (if (i64.gt_u (local.get $count) (i64.const 4294900720))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:fread-count-overflow
  (local.set $buf (call $__rt_heap_alloc (i32.wrap_i64 (local.get $count))))
  (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov})) (local.get $buf))
  (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len})) (i32.wrap_i64 (local.get $count)))
  (if (i32.eqz (call $wasi_fd_read
        (local.get $fd)
        (i32.add (global.get $__float_scratch) (i32.const {iov}))
        (i32.const 1)
        (i32.add (global.get $__float_scratch) (i32.const {nread}))))
    (then (local.set $read (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread}))))))
  ;; php's plain-file stream marks EOF as soon as a read answers fewer bytes than asked for,
  ;; so `fread($h, 10)` over the last 2 bytes already makes `feof()` true. A pipe does not:
  ;; a short read there only means the writer has not sent more yet, and the standard streams
  ;; are the only ones here that can be a pipe, so they keep the read-that-finds-nothing rule.
  (if (i32.or
        (i32.eqz (local.get $read))
        (i32.and (i32.gt_s (local.get $fd) (i32.const 2))
                 (i64.lt_u (i64.extend_i32_u (local.get $read)) (local.get $count))))
    (then (call $__rt_fd_eof_set (local.get $fd))))
  (call $__rt_str_persist (local.get $buf) (i64.extend_i32_u (local.get $read)))
  (local.set $out_len)                                            ;; persisted length (on top)
  (local.set $out)                                                ;; persisted pointer
  (call $__rt_heap_free (local.get $buf))                         ;; the bytes are copied now
  (local.get $out)
  (local.get $out_len))
"#,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        nread = IO_SCRATCH + 8,
        flag = MEMSTREAM_FLAG
    )
}

/// `__rt_stream_get_contents`: the remainder of a stream, boxed as PHP's `string|false`.
///
/// The bytes are gathered into ONE raw block and boxed once. Reusing `__rt_fread` would be
/// shorter but would persist the string twice — once in the read and once in the boxing — and
/// leak the first copy on every call.
///
/// The length is resolved BEFORE reading rather than by concatenating in a loop: an in-memory
/// stream knows its own remaining count, and a real fd answers it with a seek to the end and
/// back. Measured on php-src 8.5.6 against `examples/stream-get-contents`: a whole-file read,
/// a read that resumes from the current position after a partial `fread`, a capped read, and a
/// read from an explicit offset all agree.
fn rt_stream_get_contents() -> String {
    format!(
        r#"(func $__rt_stream_get_contents (param $h i32) (param $maxlen i64) (param $offset i64) (result i32)
  (local $d i32) (local $pos i64) (local $avail i64) (local $count i64)
  (local $cur i64) (local $end i64) (local $buf i32) (local $filled i32) (local $chunk i32)
  (local $out i32)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (if (i64.ge_s (local.get $offset) (i64.const 0))                ;; -1 means "read from here"
    (then (drop (call $__rt_fseek (local.get $h) (local.get $offset) (i64.const 0)))))
  (if (i32.and (local.get $h) (i32.const {flag}))
    (then
      (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
      (local.set $pos (i64.load (i32.add (local.get $d) (i32.const 16))))
      (local.set $avail (i64.sub (i64.load (local.get $d)) (local.get $pos))))
    (else
      ;; A pipe cannot seek, so it has no "remaining length" to ask for: `__rt_ftell` answers 0
      ;; for it and the measured span came out empty. Read such a stream until it ends instead.
      (if (i32.ne (call $wasi_fd_seek (local.get $h) (i64.const 0) (i32.const 1)
            (i32.add (global.get $__float_scratch) (i32.const {seek})))
          (i32.const 0))
        (then (return (call $__rt_stream_read_unsized (local.get $h) (local.get $maxlen)))))
      (local.set $cur (call $__rt_ftell (local.get $h)))
      (drop (call $__rt_fseek (local.get $h) (i64.const 0) (i64.const 2)))
      (local.set $end (call $__rt_ftell (local.get $h)))
      (drop (call $__rt_fseek (local.get $h) (local.get $cur) (i64.const 0)))
      (local.set $avail (i64.sub (local.get $end) (local.get $cur)))))
  (if (i64.lt_s (local.get $avail) (i64.const 0))                 ;; seeked past the end
    (then (local.set $avail (i64.const 0))))
  (local.set $count (local.get $avail))
  (if (i32.and (i64.ge_s (local.get $maxlen) (i64.const 0))
               (i64.lt_s (local.get $maxlen) (local.get $avail)))
    (then (local.set $count (local.get $maxlen))))
  ;; php sets the end-of-file flag whenever the read ran into the end of the stream: an
  ;; unbounded read always does, a bounded one only when it asked for MORE than was left.
  ;; (`maxlen` equal to the remainder, or a zero `maxlen`, stops short and leaves it false.)
  (if (i32.or (i64.lt_s (local.get $maxlen) (i64.const 0))
              (i64.gt_s (local.get $maxlen) (local.get $avail)))
    (then
      (if (i32.and (local.get $h) (i32.const {flag}))
        (then (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 1)))
        (else (call $__rt_fd_eof_set (local.get $h))))))
  (if (i64.le_s (local.get $count) (i64.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 1) (i64.const 0) (i64.const 0)))))
  (if (i32.and (local.get $h) (i32.const {flag}))
    (then
      (local.set $buf (i32.add (i32.load (i32.add (local.get $d) (i32.const 32)))
                               (i32.wrap_i64 (local.get $pos))))
      (local.set $out (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (local.get $buf)) (local.get $count)))
      (i64.store (i32.add (local.get $d) (i32.const 16))          ;; the read advances the stream
        (i64.add (local.get $pos) (local.get $count)))
      (return (local.get $out))))
  ;; Same narrowing as `file_get_contents`: a remaining span past the allocator ceiling is the
  ;; memory-limit fatal rather than a wrapped count.
  (if (i64.gt_u (local.get $count) (i64.const 4294900720))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:stream-get-contents-size
  (local.set $buf (call $__rt_heap_alloc (i32.wrap_i64 (local.get $count))))
  (block $done (loop $more
    (br_if $done (i32.ge_u (local.get $filled) (i32.wrap_i64 (local.get $count))))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.add (local.get $buf) (local.get $filled)))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len}))
      (i32.sub (i32.wrap_i64 (local.get $count)) (local.get $filled)))
    (br_if $done (i32.ne (call $wasi_fd_read
      (local.get $h)
      (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.const 1)
      (i32.add (global.get $__float_scratch) (i32.const {nread}))) (i32.const 0)))
    (local.set $chunk (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread}))))
    (if (i32.eqz (local.get $chunk))                              ;; a short read means the end
      (then
        (call $__rt_fd_eof_set (local.get $h))                    ;; and the read that found it sets eof
        (br $done)))
    (local.set $filled (i32.add (local.get $filled) (local.get $chunk)))
    (br $more)))
  (local.set $out (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $filled))))
  (call $__rt_heap_free (local.get $buf))                         ;; the bytes are copied now
  (local.get $out))
"#,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        nread = IO_SCRATCH + 8,
        flag = MEMSTREAM_FLAG,
        seek = IO_SCRATCH + 16
    )
}

/// `__rt_stream_read_unsized`: the remainder of a stream that cannot seek, boxed as a string.
///
/// Reads into a block that doubles from 4 KiB until the stream answers a read with nothing (its
/// end) or `$maxlen` bytes are in (`-1` for no limit). Only the read that finds nothing sets
/// EOF: on a pipe a short read just means the writer has not sent more yet.
fn rt_stream_read_unsized() -> String {
    format!(
        r#"(func $__rt_stream_read_unsized (param $h i32) (param $maxlen i64) (result i32)
  (local $buf i32) (local $cap i32) (local $filled i32) (local $want i32) (local $chunk i32)
  (local $grown i32) (local $out i32)
  (local.set $cap (i32.const 4096))
  (local.set $buf (call $__rt_heap_alloc (local.get $cap)))
  (block $done (loop $more
    (if (i64.ge_s (local.get $maxlen) (i64.const 0))
      (then (br_if $done (i64.ge_u (i64.extend_i32_u (local.get $filled)) (local.get $maxlen)))))
    (if (i32.eq (local.get $filled) (local.get $cap))
      (then
        (if (i32.ge_u (local.get $cap) (i32.const 1073741824))
          (then (call $__rt_oom) unreachable))                     ;; elephc-trap:deterministic-oom:unsized-read-ceiling
        (local.set $grown (call $__rt_heap_alloc (i32.shl (local.get $cap) (i32.const 1))))
        (memory.copy (local.get $grown) (local.get $buf) (local.get $filled))
        (call $__rt_heap_free (local.get $buf))
        (local.set $buf (local.get $grown))
        (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))))
    (local.set $want (i32.sub (local.get $cap) (local.get $filled)))
    (if (i64.ge_s (local.get $maxlen) (i64.const 0))
      (then
        (if (i64.lt_u (i64.sub (local.get $maxlen) (i64.extend_i32_u (local.get $filled)))
                      (i64.extend_i32_u (local.get $want)))
          (then (local.set $want (i32.wrap_i64
            (i64.sub (local.get $maxlen) (i64.extend_i32_u (local.get $filled)))))))))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.add (local.get $buf) (local.get $filled)))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len})) (local.get $want))
    (br_if $done (i32.ne (call $wasi_fd_read
      (local.get $h)
      (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.const 1)
      (i32.add (global.get $__float_scratch) (i32.const {nread}))) (i32.const 0)))
    (local.set $chunk (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread}))))
    (if (i32.eqz (local.get $chunk))
      (then
        (call $__rt_fd_eof_set (local.get $h))
        (br $done)))
    (local.set $filled (i32.add (local.get $filled) (local.get $chunk)))
    (br $more)))
  (local.set $out (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $filled))))
  (call $__rt_heap_free (local.get $buf))                         ;; the bytes are copied now
  (local.get $out))
"#,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        nread = IO_SCRATCH + 8
    )
}

/// `__rt_stream_copy_to_stream`: pipes a stream's remainder into another, boxed as `int|false`.
///
/// Composed from the two helpers rather than reimplementing a read loop: the source read is
/// exactly `stream_get_contents`, including how it resolves "everything remaining" and honours
/// an explicit offset, and the destination write is `fwrite`. Reading the whole span at once
/// matches what `stream_get_contents` already does and keeps one contract for both stream kinds.
///
/// The intermediate cell is released here — it is this function's own reference, and the string
/// bytes have been written out by the time it goes.
const RT_STREAM_COPY_TO_STREAM: &str = r#"(func $__rt_stream_copy_to_stream (param $from i32) (param $to i32) (param $len i64) (param $offset i64) (result i32)
  (local $cell i32) (local $ptr i32) (local $n i64) (local $written i64)
  (local.set $cell (call $__rt_stream_get_contents (local.get $from) (local.get $len) (local.get $offset)))
  (if (i64.ne (i64.load (local.get $cell)) (i64.const 1))         ;; tag 1 = string; anything else failed
    (then
      (call $__rt_decref_any (local.get $cell))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $ptr (i32.wrap_i64 (i64.load (i32.add (local.get $cell) (i32.const 8)))))
  (local.set $n (i64.load (i32.add (local.get $cell) (i32.const 16))))
  (local.set $written (call $__rt_fwrite (local.get $to) (local.get $ptr) (local.get $n)))
  (call $__rt_decref_any (local.get $cell))
  (if (i64.lt_s (local.get $written) (i64.const 0))               ;; the target refused the write
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_mixed_from_value (i64.const 0) (local.get $written) (i64.const 0)))
"#;

/// `__rt_fclose`: closes a stream's host fd, answering PHP's true/false — but never fd 0, 1 or 2.
///
/// The helper takes the HOST fd the lowering decoded from the handle; the stream's per-open record
/// (`__rt_stream_new`) is closed by the lowering right after, so every copy reads closed and the
/// fd is closed exactly once even if the host hands the number to a new stream afterwards.
///
/// Measured on php-src 8.5.6: `fclose(fopen("php://stdout", "w"))` answers true and leaves
/// stdout working, because that stream wraps the fd without owning it; the closed handle reads as
/// closed through its copies. Here the record is what closes, and the process fd is skipped.
/// `php://output` has no host fd at all, so there is nothing to close beyond its record.
///
/// `fclose(STDOUT)` itself is really closed by php-src: the script loses its output and, measured,
/// dies at the next write. This target marks the constant closed (`STD_CLOSED`, so `fwrite(STDERR)`
/// after `fclose(STDERR)` raises php's `TypeError` through every copy) but keeps host fds 0-2 open
/// on purpose: the alternative failure mode, silently swallowing all remaining output, is far worse.
fn rt_fclose() -> String {
    format!(
        r#"(func $__rt_fclose (param $fd i32) (result i64)
  (local $tmp i32) (local $tmpslot i32)
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.and (local.get $fd) (i32.const 1073741824))
    (then (return (call $__rt_memstream_close (local.get $fd)))))
  (if (i32.eq (local.get $fd) (i32.const {output}))               ;; php://output: no host fd, only the
    (then (return (i64.const 1))))                                ;; record, which the lowering closes
  (call $__rt_fd_eof_clear (local.get $fd))                       ;; the next fopen may reuse this fd number
  (call $__rt_fd_meta_clear (local.get $fd))                      ;; and must not inherit this stream's mode/uri
  (if (i32.lt_s (local.get $fd) (i32.const 3))                    ;; stdin/stdout/stderr survive
    (then (return (i64.const 1))))
  ;; A `tmpfile()` handle is deleted when it closes. The name is rebuilt from the index this fd
  ;; carries rather than stored, and the entry is cleared because WASI recycles fd numbers.
  (local.set $tmpslot (call $__rt_fd_addr (local.get $fd) (i32.const 1)))
  (if (local.get $tmpslot)
    (then
      (local.set $tmp (i32.load8_u (local.get $tmpslot)))
      (if (local.get $tmp)
        (then
          (i32.store8 (local.get $tmpslot) (i32.const 0))
          (drop (call $wasi_fd_close (local.get $fd)))
          (call $__rt_tmp_name (i32.sub (local.get $tmp) (i32.const 1)))
          (drop (call $__rt_unlink
            (i32.add (global.get $__float_scratch) (i32.const {name}))
            (i64.const {name_len})))
          (return (i64.const 1))))))
  (if (i32.ne (call $wasi_fd_close (local.get $fd)) (i32.const 0))
    (then (return (i64.const 0))))
  (i64.const 1))
"#,
        name = TMP_NAME,
        name_len = TMP_NAME_LEN,
        output = FD_OUTPUT
    )
}

/// `__rt_fgetc`: PHP's one-byte read, answering `string|false`.
///
/// A one-byte `fread` would answer the empty string at end of file, which PHP distinguishes
/// from a NUL byte — so the boxed result is built here: a read of zero bytes is `false`, and a
/// read of one is a one-character string, NUL included.
fn rt_fgetc() -> String {
    r#"(func $__rt_fgetc (param $fd i32) (result i32)
  (local $ptr i32) (local $len i64) (local $cell i32)
  (call $__rt_fread (local.get $fd) (i64.const 1))
  (local.set $len)
  (local.set $ptr)
  ;; Boxing persists its OWN copy, so the string `__rt_fread` answered is released here, on the
  ;; end-of-file arm as well: it used to leak one block per call.
  (if (i64.eqz (local.get $len))
    (then
      (call $__rt_decref_any (local.get $ptr))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $cell (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $ptr)) (local.get $len)))
  (call $__rt_decref_any (local.get $ptr))
  (local.get $cell))
"#
    .to_string()
}

/// `__rt_tmp_name`: writes `.elephc_tmpNNN` for `index` into the name scratch.
///
/// Three decimal digits, zero-padded, so the name has a FIXED length: `fclose` rebuilds it from
/// the same index and must produce the same bytes without storing a length.
fn rt_tmp_name() -> String {
    format!(
        r#"(func $__rt_tmp_name (param $index i32)
  (local $base i32)
  (local.set $base (i32.add (global.get $__float_scratch) (i32.const {name})))
  (i32.store8 offset=0 (local.get $base) (i32.const 46))           ;; '.'
  (i32.store8 offset=1 (local.get $base) (i32.const 101))          ;; 'e'
  (i32.store8 offset=2 (local.get $base) (i32.const 108))          ;; 'l'
  (i32.store8 offset=3 (local.get $base) (i32.const 101))          ;; 'e'
  (i32.store8 offset=4 (local.get $base) (i32.const 112))          ;; 'p'
  (i32.store8 offset=5 (local.get $base) (i32.const 104))          ;; 'h'
  (i32.store8 offset=6 (local.get $base) (i32.const 99))           ;; 'c'
  (i32.store8 offset=7 (local.get $base) (i32.const 95))           ;; '_'
  (i32.store8 offset=8 (local.get $base) (i32.const 116))          ;; 't'
  (i32.store8 offset=9 (local.get $base) (i32.const 109))          ;; 'm'
  (i32.store8 offset=10 (local.get $base) (i32.const 112))         ;; 'p'
  (i32.store8 offset=11 (local.get $base)
    (i32.add (i32.const 48) (i32.rem_u (i32.div_u (local.get $index) (i32.const 100)) (i32.const 10))))
  (i32.store8 offset=12 (local.get $base)
    (i32.add (i32.const 48) (i32.rem_u (i32.div_u (local.get $index) (i32.const 10)) (i32.const 10))))
  (i32.store8 offset=13 (local.get $base)
    (i32.add (i32.const 48) (i32.rem_u (local.get $index) (i32.const 10)))))
"#,
        name = TMP_NAME
    )
}

/// `__rt_tmpfile`: PHP's self-deleting temporary handle.
///
/// KNOWN DIVERGENCE: php-src creates the file in the system temp directory and, on POSIX,
/// unlinks it immediately so it is never visible. WASI preview1 has neither a temp directory nor
/// a way to unlink an open file, so the file lives in the PREOPENED directory under a generated
/// name until `fclose` deletes it. A program that lists that directory while the handle is open
/// sees an entry php would not show; everything read or written through the handle matches.
///
/// Each candidate name is opened EXCLUSIVELY (`x+`: `O_CREAT | O_EXCL`), and the first one that
/// opens wins. An existence probe followed by a `w+` open would leave a window in which another
/// module sharing the preopened directory takes the same name, and the truncating open would then
/// destroy its file; the exclusive open is the probe and the creation in one step, so a name
/// that is taken is skipped and never touched. 255 names, not 256: the fd's table byte stores
/// index + 1 and keeps 0 for "not a temp file", so index 255 would wrap to 0 and its file would
/// survive `fclose`.
fn rt_tmpfile() -> String {
    format!(
        r#"(func $__rt_tmpfile (result i32)
  (local $i i32) (local $cell i32) (local $fd i32) (local $slot i32)
  (if (i32.lt_s (call $__rt_wasi_dirfd) (i32.const 0))              ;; no preopen: nothing could ever open
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  ;; The mode is `x+`, written into the tail of the name scratch: a temp file must be both
  ;; readable and writable, and the exclusive create is what makes the name ours.
  (i32.store8 offset=16 (i32.add (global.get $__float_scratch) (i32.const {name})) (i32.const 120))
  (i32.store8 offset=17 (i32.add (global.get $__float_scratch) (i32.const {name})) (i32.const 43))
  (block $opened (loop $scan
    (br_if $opened (i32.ge_u (local.get $i) (i32.const 255)))
    (call $__rt_tmp_name (local.get $i))
    (local.set $cell (call $__rt_fopen
      (i32.add (global.get $__float_scratch) (i32.const {name}))
      (i64.const {name_len})
      (i32.add (i32.add (global.get $__float_scratch) (i32.const {name})) (i32.const 16))
      (i64.const 2)
      (i32.const 0)))
    (br_if $opened (i32.ge_s (call $__rt_stream_fd (local.get $cell)) (i32.const 0)))
    (call $__rt_decref_any (local.get $cell))                        ;; the name is taken: try the next
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (if (i32.ge_u (local.get $i) (i32.const 255))                    ;; every name is taken
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $fd (call $__rt_stream_fd (local.get $cell)))
  (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 1)))
  (if (local.get $slot)
    (then (i32.store8 (local.get $slot)
      (i32.add (local.get $i) (i32.const 1)))))                    ;; 0 means "not a temp file"
  (local.get $cell))
"#,
        name = TMP_NAME,
        name_len = TMP_NAME_LEN
    )
}

/// `__rt_flock`: PHP's advisory lock.
///
/// KNOWN DIVERGENCE: WASI preview1 exposes no advisory-locking call at all, and a WASI command
/// is one single-threaded process, so there is no second locker inside the module to contend
/// with. Every operation therefore succeeds, which is what php answers whenever the lock is
/// uncontended — the case a wasm module can reach on its own. What it cannot reproduce is
/// contention with a HOST process sharing the preopened directory: php would block, or answer
/// false under `LOCK_NB`, where this answers true.
const RT_FLOCK: &str = r#"(func $__rt_flock (param $fd i32) (param $op i64) (result i64)
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then (return (i64.const 0))))                                 ;; php answers false for a bad handle
  (i64.const 1))
"#;

/// `__rt_readfile`: writes a whole file to stdout and answers the byte count.
///
/// Reuses `file_get_contents` rather than streaming in chunks: the same bytes, one boxed result
/// to release, and the count php answers is that string's length. The bytes go through
/// `__rt_stdout_write`, the output-buffer interception point, so `ob_start()` captures them.
fn rt_readfile() -> String {
    r#"(func $__rt_readfile (param $path i32) (param $path_len i64) (result i32)
  (local $cell i32) (local $ptr i32) (local $len i64)
  (local.set $cell (call $__rt_file_get_contents (local.get $path) (local.get $path_len)))
  (if (i64.ne (i64.load (local.get $cell)) (i64.const 1))          ;; tag 1 = string; anything else is false
    (then (return (local.get $cell))))
  (local.set $ptr (i32.wrap_i64 (i64.load offset=8 (local.get $cell))))
  (local.set $len (i64.load offset=16 (local.get $cell)))
  (call $__rt_stdout_write (local.get $ptr) (i32.wrap_i64 (local.get $len)))  ;; through the output buffers
  (call $__rt_decref_any (local.get $cell))
  (call $__rt_mixed_from_value (i64.const 0) (local.get $len) (i64.const 0)))
"#
    .to_string()
}

/// `__rt_file_exists`: whether a path resolves, following symlinks.
fn rt_file_exists() -> String {
    format!(
        r#"(func $__rt_file_exists (param $path i32) (param $path_len i64) (result i64)
  (local $dirfd i32)
  (local.set $dirfd (call $__rt_wasi_dirfd))
  (if (i32.lt_s (local.get $dirfd) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.ne (call $wasi_path_filestat_get
        (local.get $dirfd)
        (i32.const 1)                                             ;; LOOKUP_SYMLINK_FOLLOW
        (local.get $path)
        (i32.wrap_i64 (local.get $path_len))
        (i32.add (global.get $__float_scratch) (i32.const {stat})))
      (i32.const 0))
    (then (return (i64.const 0))))
  (i64.const 1))
"#,
        stat = FILESTAT
    )
}

/// `__rt_unlink`: removes a file, answering PHP's true/false.
fn rt_unlink() -> String {
    r#"(func $__rt_unlink (param $path i32) (param $path_len i64) (result i64)
  (local $dirfd i32)
  (local.set $dirfd (call $__rt_wasi_dirfd))
  (if (i32.lt_s (local.get $dirfd) (i32.const 0))
    (then (return (i64.const 0))))
  (if (i32.ne (call $wasi_path_unlink_file
        (local.get $dirfd)
        (local.get $path)
        (i32.wrap_i64 (local.get $path_len)))
      (i32.const 0))
    (then (return (i64.const 0))))
  (i64.const 1))
"#
    .to_string()
}

/// `__rt_file_size`: a path's size in bytes, or -1 when it does not resolve.
///
/// Read from `path_filestat_get` rather than from the open fd so that sizing a file
/// needs no right beyond the one the preopen already grants for the lookup.
fn rt_file_size() -> String {
    format!(
        r#"(func $__rt_file_size (param $path i32) (param $path_len i64) (result i64)
  (local $dirfd i32)
  (local.set $dirfd (call $__rt_wasi_dirfd))
  (if (i32.lt_s (local.get $dirfd) (i32.const 0))
    (then (return (i64.const -1))))
  (if (i32.ne (call $wasi_path_filestat_get
        (local.get $dirfd)
        (i32.const 1)
        (local.get $path)
        (i32.wrap_i64 (local.get $path_len))
        (i32.add (global.get $__float_scratch) (i32.const {stat})))
      (i32.const 0))
    (then (return (i64.const -1))))
  (i64.load (i32.add (global.get $__float_scratch) (i32.const {size}))))
"#,
        stat = FILESTAT,
        size = FILESTAT + 32
    )
}

/// `__rt_file_get_contents`: the whole file as an owned string.
///
/// Sized from the directory stat and then read in a loop, because a single `fd_read`
/// is allowed to answer short and a file read that stops early would silently truncate.
/// A path that does not resolve answers boxed `false`, exactly as PHP does — the EIR
/// types this result `string|false`, so the failure value has somewhere to go.
fn rt_file_get_contents() -> String {
    format!(
        r#"(func $__rt_file_get_contents (param $path i32) (param $path_len i64) (result i32)
  (local $size i64)
  (local $handle i32)
  (local $fd i32)
  (local $buf i32)
  (local $filled i32)
  (local $chunk i32)
  (local $out i32)
  ;; A `data:` URI carries its own bytes: it is decoded, never statted or opened.
  (if (call $__rt_is_data_uri (local.get $path) (local.get $path_len))
    (then (return (call $__rt_data_uri_contents (local.get $path) (local.get $path_len)))))
  (local.set $size (call $__rt_file_size (local.get $path) (local.get $path_len)))
  (if (i64.lt_s (local.get $size) (i64.const 0))
    (then
      (call $__rt_warn_file_get_contents_failed)                   ;; names ITSELF, not fopen
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  ;; The size is narrowed to the allocator's i32 below; a file past what this module can hold
  ;; is php's memory-limit fatal, never a wrapped allocation that reads 0 or 1 bytes of it.
  (if (i64.gt_u (local.get $size) (i64.const 4294900719))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:file-get-contents-size
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const {mode})) (i32.const 114))  ;; 'r'
  (local.set $handle (call $__rt_fopen (local.get $path) (local.get $path_len)
    (i32.add (global.get $__float_scratch) (i32.const {mode})) (i64.const 1) (i32.const 0)))
  (local.set $fd (call $__rt_stream_fd (local.get $handle)))
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then
      (call $__rt_stream_cell_release (local.get $handle))
      (call $__rt_warn_file_get_contents_failed)
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $buf (call $__rt_heap_alloc (i32.wrap_i64 (i64.add (local.get $size) (i64.const 1)))))
  (block $done (loop $more
    (br_if $done (i32.ge_u (local.get $filled) (i32.wrap_i64 (local.get $size))))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.add (local.get $buf) (local.get $filled)))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len}))
      (i32.sub (i32.wrap_i64 (local.get $size)) (local.get $filled)))
    (br_if $done (i32.ne (call $wasi_fd_read
      (local.get $fd)
      (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.const 1)
      (i32.add (global.get $__float_scratch) (i32.const {nread}))) (i32.const 0)))
    (local.set $chunk (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread}))))
    (br_if $done (i32.eqz (local.get $chunk)))                    ;; end of file
    (local.set $filled (i32.add (local.get $filled) (local.get $chunk)))
    (br $more)))
  (drop (call $wasi_fd_close (local.get $fd)))
  (call $__rt_stream_cell_release (local.get $handle))
  (local.set $out (call $__rt_mixed_from_value                    ;; tag 1 persists its own copy
    (i64.const 1) (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $filled))))
  (call $__rt_heap_free (local.get $buf))                         ;; the bytes are copied now
  (local.get $out))
"#,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        nread = IO_SCRATCH + 8,
        mode = IO_SCRATCH + 0x30
    )
}

/// `__rt_data_uri_contents`: the payload of a `data:` URI as an owned string, boxed as
/// `string|false` — what `file_get_contents` and `readfile` answer for one.
///
/// Decodes through `__rt_data_uri_open` (so every measured acceptance rule lives in one place),
/// copies the bytes out, then releases the temporary stream: its buffer, its metadata strings and
/// the descriptor itself, which no other holder can reach. A malformed URI is php's warning and
/// `false`.
fn rt_data_uri_contents() -> String {
    format!(
        r#"(func $__rt_data_uri_contents (param $path i32) (param $path_len i64) (result i32)
  (local $h i32) (local $d i32) (local $out i32)
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const {mode})) (i32.const 114))  ;; 'r'
  (local.set $h (call $__rt_data_uri_open (local.get $path) (local.get $path_len)
    (i32.add (global.get $__float_scratch) (i32.const {mode})) (i64.const 1)))
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then
      (call $__rt_warn_file_get_contents_failed)
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
  (local.set $out (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (i32.load offset=32 (local.get $d))) (i64.load (local.get $d))))
  (drop (call $__rt_memstream_close (local.get $h)))
  (call $__rt_heap_free (local.get $d))                           ;; no copy of this handle exists
  (local.get $out))
"#,
        mode = IO_SCRATCH + 0x30
    )
}

/// `__rt_file_put_contents`: writes a string to a path, answering the byte count.
///
/// Opens in PHP's `w` mode — create, truncate — which is what `file_put_contents`
/// means without `FILE_APPEND`. A path that cannot be opened answers 0, where PHP
/// answers `false` after a warning.
fn rt_file_put_contents() -> String {
    format!(
        r#"(func $__rt_file_put_contents (param $path i32) (param $path_len i64) (param $data i32) (param $data_len i64) (result i64)
  (local $handle i32)
  (local $written i64)
  ;; A `data:` URI is read-only (php: "Stream is not writable", answering false): nothing may be
  ;; created under that name.
  (if (call $__rt_is_data_uri (local.get $path) (local.get $path_len))
    (then (return (i64.const 0))))
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const {mode})) (i32.const 119))  ;; 'w'
  (local.set $handle (call $__rt_fopen (local.get $path) (local.get $path_len)
    (i32.add (global.get $__float_scratch) (i32.const {mode})) (i64.const 1) (i32.const 0)))
  (if (i32.lt_s (call $__rt_stream_fd (local.get $handle)) (i32.const 0))
    (then
      (call $__rt_stream_cell_release (local.get $handle))
      (return (i64.const 0))))
  (local.set $written (call $__rt_fwrite (call $__rt_stream_fd (local.get $handle)) (local.get $data) (local.get $data_len)))
  (drop (call $__rt_fclose (call $__rt_stream_fd (local.get $handle))))
  (call $__rt_stream_cell_release (local.get $handle))
  (local.get $written))
"#,
        mode = IO_SCRATCH + 0x30
    )
}

/// Emits `__rt_stream_get_meta_data`, whose nine keys and four constant values live in
/// the command data region — which is why it is emitted from the failure runtime's
/// tail, where their offsets are known, and not from `emit_file_runtime`.
///
/// `offsets` is the 15-entry (offset, len) slice laid out by `emit_failure_runtime`:
/// the nine keys in php's own insertion order (timed_out, blocked, eof, wrapper_type,
/// stream_type, mode, unread_bytes, seekable, uri) then plainfile, STDIO, PHP, MEMORY,
/// TEMP.
///
/// Measured on php-src 8.5.6:
///   * a real file answers wrapper_type "plainfile" / stream_type "STDIO", the MODE
///     exactly as `fopen` received it ("rb" stays "rb"), and the URI as given;
///   * `php://memory` and `php://temp` answer "PHP" with "MEMORY"/"TEMP", and their
///     mode is NORMALIZED to binary — "r" reports "rb", "w+" reports "w+b";
///   * `php://output` answers "PHP" / "Output" with mode "wb" whatever mode was asked;
///   * `blocked` is true and `timed_out` false for both; `unread_bytes` is 0;
///   * `seekable` is asked of the fd itself here — a zero-length SEEK_CUR succeeds
///     only on a seekable fd, so pipes and ttys answer false the way php does.
///
/// A stream this runtime never recorded (a bare std fd) reads as two empty strings
/// for mode/uri rather than fabricated values.
pub(super) fn emit_stream_meta_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    let [k_to, k_bl, k_eof, k_wt, k_st, k_mode, k_ub, k_seek, k_uri, v_plain, v_stdio, v_php, v_mem, v_temp, v_out] =
        offsets
    else {
        unreachable!("emit_failure_runtime hands exactly the fifteen meta fragments");
    };
    wm.add_raw_func(&format!(
        r#"(func $__rt_stream_get_meta_data (param $h i32) (result i32)
  (local $hash i32) (local $d i32) (local $slot i32) (local $eof i64)
  (local $mode_p i32) (local $mode_l i64) (local $uri_p i32) (local $uri_l i64)
  (local $wrap_p i32) (local $wrap_l i64) (local $st_p i32) (local $st_l i64)
  (local $seek i64) (local $mb i32) (local $i i64) (local $hasb i32) (local $out i32)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (if (i32.and (local.get $h) (i32.const {flag}))
    (then
      ;; In-memory stream: the descriptor carries everything.
      (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
      (local.set $eof (i64.extend_i32_u
        (i64.ne (i64.load (i32.add (local.get $d) (i32.const 24))) (i64.const 0))))
      (local.set $mode_p (i32.load (i32.add (local.get $d) (i32.const 40))))
      (local.set $mode_l (i64.extend_i32_u (i32.load (i32.add (local.get $d) (i32.const 44)))))
      (local.set $uri_p (i32.load (i32.add (local.get $d) (i32.const 48))))
      (local.set $uri_l (i64.extend_i32_u (i32.load (i32.add (local.get $d) (i32.const 52)))))
      (local.set $wrap_p (i32.const {php_p}))
      (local.set $wrap_l (i64.const {php_l}))
      ;; The two memstream uris differ in the letter after "php://": `t` for every temp
      ;; spelling (including "php://temp/maxmemory:N"), `m` for "php://memory". Their LENGTHS
      ;; told the bare forms apart and called every maxmemory temp stream MEMORY.
      (if (i32.and
            (i64.gt_u (local.get $uri_l) (i64.const 6))
            (i32.eq (i32.or (i32.load8_u offset=6 (local.get $uri_p)) (i32.const 32))
                    (i32.const 116)))
        (then
          (local.set $st_p (i32.const {temp_p}))
          (local.set $st_l (i64.const {temp_l})))
        (else
          (local.set $st_p (i32.const {mem_p}))
          (local.set $st_l (i64.const {mem_l}))))
      (local.set $seek (i64.const 1))
      ;; php reports a memory stream's mode in BINARY: append 'b' when absent.
      (local.set $hasb (i32.const 0))
      (local.set $i (i64.const 0))
      (block $scanned (loop $scan
        (br_if $scanned (i64.ge_u (local.get $i) (local.get $mode_l)))
        (if (i32.eq (i32.load8_u (i32.add (local.get $mode_p) (i32.wrap_i64 (local.get $i)))) (i32.const 98))
          (then
            (local.set $hasb (i32.const 1))
            (br $scanned)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $scan)))
      (if (i32.eqz (local.get $hasb))
        (then
          (local.set $mb (call $__rt_heap_alloc (i32.add (i32.wrap_i64 (local.get $mode_l)) (i32.const 1))))
          (memory.copy (local.get $mb) (local.get $mode_p) (i32.wrap_i64 (local.get $mode_l)))
          (i32.store8 (i32.add (local.get $mb) (i32.wrap_i64 (local.get $mode_l))) (i32.const 98))
          (local.set $mode_p (local.get $mb))
          (local.set $mode_l (i64.add (local.get $mode_l) (i64.const 1))))))
    (else
      ;; Real fd: the fopen record plus the live eof flag and a seekability probe.
      (local.set $slot (call $__rt_fd_addr (local.get $h) (i32.const 2)))
      (if (i32.eqz (local.get $slot))                                  ;; an untracked fd was never recorded
        (then (local.set $slot (i32.add (global.get $__float_scratch) (i32.const {zero})))))
      (local.set $mode_p (i32.load (local.get $slot)))
      (local.set $mode_l (i64.extend_i32_u (i32.load (i32.add (local.get $slot) (i32.const 4)))))
      (local.set $uri_p (i32.load (i32.add (local.get $slot) (i32.const 8))))
      (local.set $uri_l (i64.extend_i32_u (i32.load (i32.add (local.get $slot) (i32.const 12)))))
      (local.set $eof (call $__rt_fd_eof_get (local.get $h)))
      (local.set $wrap_p (i32.const {plain_p}))
      (local.set $wrap_l (i64.const {plain_l}))
      (local.set $st_p (i32.const {stdio_p}))
      (local.set $st_l (i64.const {stdio_l}))
      (if (i32.eq (local.get $h) (i32.const {output}))                 ;; php://output is a PHP stream of type Output
        (then
          (local.set $wrap_p (i32.const {php_p}))
          (local.set $wrap_l (i64.const {php_l}))
          (local.set $st_p (i32.const {out_p}))
          (local.set $st_l (i64.const {out_l}))))
      (local.set $seek (i64.extend_i32_u (i32.eqz (call $wasi_fd_seek
        (local.get $h) (i64.const 0) (i32.const 1)
        (i32.add (global.get $__float_scratch) (i32.const 12352))))))))
  (local.set $hash (call $__rt_hash_new (i64.const 32) (i64.const 7)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_to_p}) (i64.const {k_to_l}) (i64.const 0) (i64.const 0) (i64.const 3)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_bl_p}) (i64.const {k_bl_l}) (i64.const 1) (i64.const 0) (i64.const 3)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_eof_p}) (i64.const {k_eof_l}) (local.get $eof) (i64.const 0) (i64.const 3)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_wt_p}) (i64.const {k_wt_l})
    (i64.extend_i32_u (local.get $wrap_p)) (local.get $wrap_l) (i64.const 1)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_st_p}) (i64.const {k_st_l})
    (i64.extend_i32_u (local.get $st_p)) (local.get $st_l) (i64.const 1)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_mode_p}) (i64.const {k_mode_l})
    (i64.extend_i32_u (local.get $mode_p)) (local.get $mode_l) (i64.const 1)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_ub_p}) (i64.const {k_ub_l}) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_seek_p}) (i64.const {k_seek_l}) (local.get $seek) (i64.const 0) (i64.const 3)))
  (local.set $hash (call $__rt_hash_set (local.get $hash)
    (i64.const {k_uri_p}) (i64.const {k_uri_l})
    (i64.extend_i32_u (local.get $uri_p)) (local.get $uri_l) (i64.const 1)))
  (if (local.get $mb)
    (then (call $__rt_heap_free (local.get $mb))))                ;; hash_set copied the normalized mode
  (local.set $out (call $__rt_mixed_from_value (i64.const 5)
    (i64.extend_i32_u (local.get $hash)) (i64.const 0)))
  (call $__rt_decref_any (local.get $hash))                       ;; the cell is the hash's sole owner
  (local.get $out))
"#,
        flag = MEMSTREAM_FLAG,
        zero = ZERO16,
        php_p = v_php.0, php_l = v_php.1,
        temp_p = v_temp.0, temp_l = v_temp.1,
        mem_p = v_mem.0, mem_l = v_mem.1,
        plain_p = v_plain.0, plain_l = v_plain.1,
        stdio_p = v_stdio.0, stdio_l = v_stdio.1,
        out_p = v_out.0, out_l = v_out.1,
        output = FD_OUTPUT,
        k_to_p = k_to.0, k_to_l = k_to.1,
        k_bl_p = k_bl.0, k_bl_l = k_bl.1,
        k_eof_p = k_eof.0, k_eof_l = k_eof.1,
        k_wt_p = k_wt.0, k_wt_l = k_wt.1,
        k_st_p = k_st.0, k_st_l = k_st.1,
        k_mode_p = k_mode.0, k_mode_l = k_mode.1,
        k_ub_p = k_ub.0, k_ub_l = k_ub.1,
        k_seek_p = k_seek.0, k_seek_l = k_seek.1,
        k_uri_p = k_uri.0, k_uri_l = k_uri.1,
    ));
}

/// `__rt_is_data_uri`: whether a path is an RFC 2397 `data:` URI.
///
/// The `//` is OPTIONAL — measured on php-src 8.5.6, `data:,x` opens exactly like
/// `data://,x` — so only the five-byte `data:` prefix is tested here.
const RT_IS_DATA_URI: &str = r#"(func $__rt_is_data_uri (param $p i32) (param $len i64) (result i32)
  (if (i64.lt_u (local.get $len) (i64.const 5))
    (then (return (i32.const 0))))
  (i32.and
    (i32.and (i32.eq (i32.load8_u (local.get $p)) (i32.const 100))                     ;; 'd'
             (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 97)))  ;; 'a'
    (i32.and
      (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 2))) (i32.const 116))   ;; 't'
               (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 3))) (i32.const 97)))   ;; 'a'
      (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 4))) (i32.const 58)))))          ;; ':'
"#;

/// `__rt_data_uri_open`: decodes an RFC 2397 `data:` URI into an in-memory stream.
///
/// Answers the memstream handle, or -1 when the URI cannot be opened — which php reports as
/// `fopen`'s `false`. Measured on php-src 8.5.6, every rule below came from a probe rather
/// than from the RFC:
///   * the `//` after `data:` is optional;
///   * NO comma at all fails (`data://text/plain` is false);
///   * the payload is base64 only when the media type ends with the LOWERCASE `;base64` —
///     `;BASE64` fails, because php compares that literal case-sensitively;
///   * base64 decoding is STRICT here, unlike the `base64_decode` builtin: whitespace and `=`
///     are skipped but any other stray byte fails the open (`;base64,####` is false);
///     padding is optional (`SGVsbG8` decodes like `SGVsbG8=`);
///   * a plain payload is `urldecode`, NOT `rawurldecode`: `+` becomes a space. A `%` without
///     two hex digits after it stays LITERAL (`100%` is `100%`, `%4` is `%4`, `%zz` is `%zz`);
///   * an empty payload is a valid empty stream.
const RT_DATA_URI_OPEN: &str = r#"(func $__rt_data_uri_open (param $p i32) (param $len i64) (param $mode i32) (param $mode_len i64) (result i32)
  (local $i i32) (local $n i32) (local $comma i32) (local $meta i32)
  (local $b64 i32) (local $buf i32) (local $w i32) (local $c i32)
  (local $v i32) (local $acc i32) (local $bits i32) (local $hi i32) (local $lo i32)
  (local $h i32) (local $mend i32) (local $seg i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (local.set $i (i32.const 5))                                    ;; past "data:"
  (if (i32.and (i32.gt_s (i32.sub (local.get $n) (local.get $i)) (i32.const 1))
       (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 47))
                (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 1))))
                        (i32.const 47))))
    (then (local.set $i (i32.add (local.get $i) (i32.const 2)))))  ;; the optional "//"
  (local.set $meta (local.get $i))
  (local.set $comma (i32.const -1))
  (block $found (loop $scan                                        ;; the FIRST comma splits it
    (br_if $found (i32.ge_s (local.get $i) (local.get $n)))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 44))
      (then
        (local.set $comma (local.get $i))
        (br $found)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (if (i32.lt_s (local.get $comma) (i32.const 0))
    (then (return (i32.const -1))))                                ;; no comma: php answers false
  ;; base64 when the media type ends with the lowercase ";base64"
  (local.set $b64 (i32.const 0))
  (if (i32.ge_s (i32.sub (local.get $comma) (local.get $meta)) (i32.const 7))
    (then
      (local.set $i (i32.sub (local.get $comma) (i32.const 7)))
      (if (i32.and
            (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 59))
                     (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 1)))) (i32.const 98)))
            (i32.and
              (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 2)))) (i32.const 97))
                       (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 3)))) (i32.const 115)))
              (i32.and
                (i32.and (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 4)))) (i32.const 101))
                         (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 5)))) (i32.const 54)))
                (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 6)))) (i32.const 52)))))
        (then (local.set $b64 (i32.const 1))))))
  ;; Every remaining `;` parameter must be `key=value`. Measured: `;charset=utf-8`,
  ;; `;CHARSET=utf-8` and `;foo=bar` all open, while `;foo`, `;base64x`, `;BASE64` and a bare
  ;; `;` all answer FALSE — so the lowercase `base64` above is the ONLY parameter allowed to
  ;; carry no `=`, and an uppercase spelling is not a base64 marker NOR a valid parameter.
  (local.set $mend (local.get $comma))
  (if (local.get $b64)
    (then (local.set $mend (i32.sub (local.get $comma) (i32.const 7)))))
  (local.set $i (local.get $meta))
  (local.set $seg (i32.const -1))                                  ;; -1 until the first ';'
  (block $mdone (loop $mscan
    (if (i32.ge_s (local.get $i) (local.get $mend))
      (then
        (if (i32.eq (local.get $seg) (i32.const 0))                ;; a parameter closed with no '='
          (then (return (i32.const -1))))                          ;; nothing allocated yet
        (br $mdone)))
    (local.set $c (i32.load8_u (i32.add (local.get $p) (local.get $i))))
    (if (i32.eq (local.get $c) (i32.const 59))                     ;; ';' closes a segment
      (then
        (if (i32.eq (local.get $seg) (i32.const 0))
          (then (return (i32.const -1))))
        (local.set $seg (i32.const 0)))                            ;; the next one needs an '='
      (else
        (if (i32.and (i32.eq (local.get $c) (i32.const 61))        ;; '=' satisfies it
                     (i32.eq (local.get $seg) (i32.const 0)))
          (then (local.set $seg (i32.const 1))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $mscan)))
  (local.set $i (i32.add (local.get $comma) (i32.const 1)))        ;; the payload starts here
  ;; Neither decoding ever grows the input, so the payload length bounds both.
  (local.set $buf (call $__rt_heap_alloc (i32.add (i32.sub (local.get $n) (local.get $i)) (i32.const 1))))
  (local.set $w (i32.const 0))
  (if (local.get $b64)
    (then
      (block $bend (loop $b64loop
        (br_if $bend (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $c (i32.load8_u (i32.add (local.get $p) (local.get $i))))
        (local.set $v (call $__rt_b64_value (local.get $c)))
        (if (i32.ge_s (local.get $v) (i32.const 0))
          (then
            (local.set $acc (i32.or (i32.shl (local.get $acc) (i32.const 6)) (local.get $v)))
            (local.set $bits (i32.add (local.get $bits) (i32.const 6)))
            (if (i32.ge_u (local.get $bits) (i32.const 8))
              (then
                (local.set $bits (i32.sub (local.get $bits) (i32.const 8)))
                (i32.store8 (i32.add (local.get $buf) (local.get $w))
                  (i32.and (i32.shr_u (local.get $acc) (local.get $bits)) (i32.const 255)))
                (local.set $w (i32.add (local.get $w) (i32.const 1))))))
          (else
            ;; STRICT: only padding and whitespace may sit outside the alphabet.
            (if (i32.eqz (i32.or
                  (i32.eq (local.get $c) (i32.const 61))                       ;; '='
                  (i32.or
                    (i32.eq (local.get $c) (i32.const 32))                     ;; ' '
                    (i32.and (i32.ge_u (local.get $c) (i32.const 9))
                             (i32.le_u (local.get $c) (i32.const 13))))))      ;; \t \n \v \f \r
              (then
                (call $__rt_heap_free (local.get $buf))
                (return (i32.const -1))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $b64loop))))
    (else
      (block $pend (loop $pctloop
        (br_if $pend (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $c (i32.load8_u (i32.add (local.get $p) (local.get $i))))
        (if (i32.eq (local.get $c) (i32.const 43))                             ;; '+' is a space
          (then
            (i32.store8 (i32.add (local.get $buf) (local.get $w)) (i32.const 32))
            (local.set $w (i32.add (local.get $w) (i32.const 1))))
          (else
            (local.set $hi (i32.const -1))
            (if (i32.and (i32.eq (local.get $c) (i32.const 37))                ;; '%'
                         (i32.lt_s (i32.add (local.get $i) (i32.const 2)) (local.get $n)))
              (then
                (local.set $hi (call $__rt_hex_digit_value
                  (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 1))))))
                (local.set $lo (call $__rt_hex_digit_value
                  (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 2))))))
                (if (i32.lt_s (local.get $lo) (i32.const 0))
                  (then (local.set $hi (i32.const -1))))))
            (if (i32.ge_s (local.get $hi) (i32.const 0))
              (then                                                            ;; a complete %HH
                (i32.store8 (i32.add (local.get $buf) (local.get $w))
                  (i32.add (i32.mul (local.get $hi) (i32.const 16)) (local.get $lo)))
                (local.set $i (i32.add (local.get $i) (i32.const 2))))
              (else                                                            ;; anything else is literal
                (i32.store8 (i32.add (local.get $buf) (local.get $w)) (local.get $c))))
            (local.set $w (i32.add (local.get $w) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $pctloop)))))
  (local.set $h (call $__rt_memstream_new (local.get $mode) (local.get $mode_len)
                                          (local.get $p) (local.get $len)))
  (i32.store offset=60 (i32.and (local.get $h) (i32.const 1073741823)) (i32.const 0)) ;; writable while it is filled
  (drop (call $__rt_memstream_write (local.get $h) (local.get $buf) (i64.extend_i32_u (local.get $w))))
  (call $__rt_heap_free (local.get $buf))
  (call $__rt_memstream_seek (local.get $h) (i64.const 0))         ;; reads start at the beginning
  (i32.store offset=60 (i32.and (local.get $h) (i32.const 1073741823)) (i32.const 2)) ;; a data: stream is read-only
  (local.get $h))
"#;

/// `__rt_stream_get_line`: PHP's `stream_get_line`, boxed as `string|false`.
///
/// Measured on php-src 8.5.6, the rules the two branches implement:
///   * the delimiter is STRIPPED from the answer and consumed from the stream;
///   * the length cap wins over the delimiter and consumes NOTHING past the cap —
///     `stream_get_line($f, 3, "#")` on `abc#rest` answers `abc` with `ftell` at 3;
///   * a length of 0 (PHP's default) reads up to the 8192-byte default chunk;
///   * partial delimiter matches are data (`ab##` with `###` answers `ab##` whole);
///   * EOF with no bytes answers `false`; either EOF sets the feof flag, but a read
///     that stops at the cap — even on the last byte — does NOT.
///
/// The real-fd branch reads one byte at a time: WASI has no ungetc, so a byte read
/// past the delimiter could not be handed back, and byte-wise reads make the
/// stop-at-delimiter position exact by construction.
fn rt_stream_get_line() -> String {
    format!(
        r#"(func $__rt_stream_get_line (param $h i32) (param $len i64) (param $dptr i32) (param $dlen i64) (result i32)
  (local $eff i32) (local $d i32) (local $size i64) (local $pos i64) (local $buf i32)
  (local $avail i64) (local $limit i32) (local $k i32) (local $j i32) (local $m i32)
  (local $hit i32) (local $acc i32) (local $n i32) (local $out i32) (local $eof i32)
  (local $acap i32) (local $newcap i32) (local $grown i32)
  (if (i32.lt_s (local.get $h) (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $eff (i32.const 8192))                               ;; PHP's default chunk for length 0
  ;; The cap is narrowed to i32 only after it is range-checked: a length of 2^32 + 1 used to wrap
  ;; to 1 and cut the line short. Past i32::MAX a cap just means "no practical cap": the line
  ;; can never be longer than the stream, and the real-fd accumulator below grows on demand
  ;; instead of reserving the cap up front, so a huge cap costs nothing.
  (if (i64.gt_s (local.get $len) (i64.const 0))
    (then
      (if (i64.gt_u (local.get $len) (i64.const 2147483647))
        (then (local.set $eff (i32.const 2147483647)))
        (else (local.set $eff (i32.wrap_i64 (local.get $len)))))))
  (if (i32.and (local.get $h) (i32.const {flag}))
    (then
      ;; In-memory stream: the bytes are already here, scan them in place.
      (local.set $d (i32.and (local.get $h) (i32.const 1073741823)))
      (local.set $size (i64.load (local.get $d)))
      (local.set $pos (i64.load (i32.add (local.get $d) (i32.const 16))))
      (local.set $avail (i64.sub (local.get $size) (local.get $pos)))
      (if (i64.le_s (local.get $avail) (i64.const 0))
        (then
          (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 1))  ;; the read found nothing
          (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
      (local.set $buf (i32.add (i32.load (i32.add (local.get $d) (i32.const 32)))
                               (i32.wrap_i64 (local.get $pos))))
      (local.set $limit (i32.wrap_i64 (local.get $avail)))
      (if (i32.gt_u (local.get $limit) (local.get $eff))
        (then (local.set $limit (local.get $eff))))
      (local.set $hit (i32.const -1))
      (if (i64.gt_s (local.get $dlen) (i64.const 0))
        (then
          (local.set $k (i32.const 0))
          (block $found (loop $scan
            (br_if $found (i32.ge_u (local.get $k) (local.get $limit)))
            ;; the delimiter must fit inside the PERMITTED window (the cap and what is left), as it
            ;; does on a real fd: a cap of 3 and a two-byte delimiter over `ab` + delimiter + `rest` is `ab` + one delimiter byte
            (if (i64.le_s (i64.add (i64.extend_i32_u (local.get $k)) (local.get $dlen))
                          (i64.extend_i32_u (local.get $limit)))
              (then
                (local.set $j (i32.const 0))
                (local.set $m (i32.const 1))
                (block $cmp_done (loop $cmp
                  (br_if $cmp_done (i32.ge_u (local.get $j) (i32.wrap_i64 (local.get $dlen))))
                  (if (i32.ne
                        (i32.load8_u (i32.add (i32.add (local.get $buf) (local.get $k)) (local.get $j)))
                        (i32.load8_u (i32.add (local.get $dptr) (local.get $j))))
                    (then
                      (local.set $m (i32.const 0))
                      (br $cmp_done)))
                  (local.set $j (i32.add (local.get $j) (i32.const 1)))
                  (br $cmp)))
                (if (local.get $m)
                  (then
                    (local.set $hit (local.get $k))
                    (br $found)))))
            (local.set $k (i32.add (local.get $k) (i32.const 1)))
            (br $scan)))))
      (if (i32.ne (local.get $hit) (i32.const -1))
        (then
          (local.set $out (call $__rt_mixed_from_value (i64.const 1)
            (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $hit))))
          (i64.store (i32.add (local.get $d) (i32.const 16))      ;; consume data AND delimiter
            (i64.add (local.get $pos)
              (i64.add (i64.extend_i32_u (local.get $hit)) (local.get $dlen))))
          (return (local.get $out))))
      (local.set $out (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $limit))))
      (i64.store (i32.add (local.get $d) (i32.const 16))          ;; cap or end: consume only the data
        (i64.add (local.get $pos) (i64.extend_i32_u (local.get $limit))))
      ;; No delimiter and the DATA ran out before the cap: the fd branch would have
      ;; attempted one more read and found nothing, so this stop is an EOF stop. A stop
      ;; AT the cap never attempts that read — even when the cap lands on the last byte.
      (if (i32.lt_u (local.get $limit) (local.get $eff))
        (then (i64.store (i32.add (local.get $d) (i32.const 24)) (i64.const 1))))
      (return (local.get $out))))
  ;; Real fd: one byte at a time, stopping on cap, delimiter, or the read that finds nothing.
  (local.set $acap (select (local.get $eff) (i32.const 4096) (i32.lt_u (local.get $eff) (i32.const 4096))))
  (local.set $acc (call $__rt_heap_alloc (local.get $acap)))
  (block $stop (loop $byte
    (br_if $stop (i32.ge_u (local.get $n) (local.get $eff)))      ;; cap: eof stays untouched
    (if (i32.ge_u (local.get $n) (local.get $acap))               ;; the accumulator is full: double it
      (then
        (local.set $newcap (i32.wrap_i64 (select
          (i64.extend_i32_u (local.get $eff))
          (i64.shl (i64.extend_i32_u (local.get $acap)) (i64.const 1))
          (i64.lt_u (i64.extend_i32_u (local.get $eff))
                    (i64.shl (i64.extend_i32_u (local.get $acap)) (i64.const 1))))))
        (local.set $grown (call $__rt_heap_alloc (local.get $newcap)))
        (memory.copy (local.get $grown) (local.get $acc) (local.get $n))
        (call $__rt_heap_free (local.get $acc))
        (local.set $acc (local.get $grown))
        (local.set $acap (local.get $newcap))))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov}))
      (i32.add (local.get $acc) (local.get $n)))
    (i32.store (i32.add (global.get $__float_scratch) (i32.const {iov_len})) (i32.const 1))
    (if (i32.ne (call $wasi_fd_read
          (local.get $h)
          (i32.add (global.get $__float_scratch) (i32.const {iov}))
          (i32.const 1)
          (i32.add (global.get $__float_scratch) (i32.const {nread}))) (i32.const 0))
      (then
        (local.set $eof (i32.const 1))
        (br $stop)))
    (if (i32.eqz (i32.load (i32.add (global.get $__float_scratch) (i32.const {nread}))))
      (then
        (local.set $eof (i32.const 1))
        (br $stop)))
    (local.set $n (i32.add (local.get $n) (i32.const 1)))
    (if (i32.and (i64.gt_s (local.get $dlen) (i64.const 0))
                 (i32.ge_u (local.get $n) (i32.wrap_i64 (local.get $dlen))))
      (then
        ;; does the accumulator now END WITH the delimiter?
        (local.set $j (i32.const 0))
        (local.set $m (i32.const 1))
        (block $cmp_done (loop $cmp
          (br_if $cmp_done (i32.ge_u (local.get $j) (i32.wrap_i64 (local.get $dlen))))
          (if (i32.ne
                (i32.load8_u (i32.add
                  (i32.add (local.get $acc) (i32.sub (local.get $n) (i32.wrap_i64 (local.get $dlen))))
                  (local.get $j)))
                (i32.load8_u (i32.add (local.get $dptr) (local.get $j))))
            (then
              (local.set $m (i32.const 0))
              (br $cmp_done)))
          (local.set $j (i32.add (local.get $j) (i32.const 1)))
          (br $cmp)))
        (if (local.get $m)
          (then
            (local.set $out (call $__rt_mixed_from_value (i64.const 1)
              (i64.extend_i32_u (local.get $acc))
              (i64.extend_i32_u (i32.sub (local.get $n) (i32.wrap_i64 (local.get $dlen))))))
            (call $__rt_heap_free (local.get $acc))
            (return (local.get $out))))))
    (br $byte)))
  (if (local.get $eof)
    (then (call $__rt_fd_eof_set (local.get $h))))
  (if (i32.and (local.get $eof) (i32.eqz (local.get $n)))
    (then
      (call $__rt_heap_free (local.get $acc))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $out (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $acc)) (i64.extend_i32_u (local.get $n))))
  (call $__rt_heap_free (local.get $acc))
  (local.get $out))
"#,
        flag = MEMSTREAM_FLAG,
        iov = IO_SCRATCH,
        iov_len = IO_SCRATCH + 4,
        nread = IO_SCRATCH + 8
    )
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the address guard the in-memory stream handle encoding depends on.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::{rt_memstream_new, MEMSTREAM_FLAG};

    /// Verifies a descriptor allocated at or above the handle bit is refused, never encoded.
    ///
    /// Every reader recovers the descriptor by masking `MEMSTREAM_FLAG` off the handle, so an
    /// address that already has that bit decodes to a different block. No PHP program reaches
    /// that placement reliably — the first-fit allocator reuses low holes first — so the guard
    /// is pinned on the emitted helper instead.
    #[test]
    fn memstream_new_refuses_a_descriptor_at_the_handle_bit() {
        let wat = rt_memstream_new();
        let guard = format!("(if (i32.ge_u (local.get $d) (i32.const {MEMSTREAM_FLAG}))");
        let encode = format!("(i32.or (local.get $d) (i32.const {MEMSTREAM_FLAG}))");
        let alloc = wat
            .find("(call $__rt_heap_alloc (i32.const 64))")
            .expect("descriptor allocation");
        let check = wat.find(&guard).expect("the address guard");
        let encoded = wat.find(&encode).expect("the encoding");
        assert!(alloc < check && check < encoded, "{wat}");
        assert!(wat[check..encoded].contains("call $__rt_oom"), "{wat}");
    }
}
