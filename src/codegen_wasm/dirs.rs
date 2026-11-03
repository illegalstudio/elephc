//! Purpose:
//! Directory iteration on `wasm32-wasi`: `opendir`, `readdir`, `rewinddir` and `closedir`.
//!
//! Called from:
//! - `plan` registers the runtime for a module that opens a directory.
//! - `inst` lowers the four `RuntimeFnId` identities onto these helpers.
//! - `capability` gates each call against the shapes below.
//!
//! Key details:
//! - A directory handle is the same boxed resource a stream handle is — `__rt_mixed_from_value`
//!   under tag 9, carrying a per-open record (`files::__rt_stream_new`) that names the WASI fd — so
//!   it lives in locals and arrays like any other, and `closedir` closes it through the same
//!   `fd_close` and marks the record closed, so a stale copy stays closed when the fd is reused.
//! - The walk does NOT position itself with WASI cookies. That contract is not honoured
//!   uniformly: node's WASI ignores the cookie entirely and writes each entry's OWN index into
//!   `d_next` rather than the next entry's, so a cookie-driven cursor re-reads the first entry
//!   forever there. The whole listing is snapshotted into one heap block per handle instead and
//!   walked by byte offset, which is also closer to what php's `opendir` is — a directory stream
//!   opened once and read down.

use super::wat::{DataSegment, WatModule};

/// Offset of the per-fd directory state: 16 bytes per real WASI fd, 256 fds.
///
/// Snapshot pointer at +0 (zero until the first read), its byte length at +4, the cursor's byte
/// offset into it at +8, and the cursor phase at +12. Linear memory starts zeroed, so an fd that
/// was never opened as a directory reads as "nothing snapshotted, start from the beginning";
/// `closedir` re-zeroes the record and frees the block, because WASI recycles fd numbers exactly
/// as the EOF-flag and stream-metadata tables have to account for.
pub(super) const DIR_STATE: u32 = 0x5200;

/// Bytes of per-fd state.
const DIR_STATE_BYTES: u32 = 16;

/// Offset of the two syscall out-parameters: the opened fd at +0, `fd_readdir`'s bufused at +4.
const DIR_OUT: u32 = DIR_STATE + DIR_STATE_BYTES * 256;

/// Offset of the two bytes `.` and `..` are read from: a one-byte read is `.`, a two-byte read is
/// `..`, so one `".."` in memory serves both.
pub(super) const DIR_DOTS: u32 = DIR_OUT + 8;

/// First snapshot size tried, doubled until the listing fits.
const DIR_SNAPSHOT_START: u32 = 4096;

/// Capacity past which a snapshot cannot double inside an i32 byte count.
///
/// The listing keeps growing up to here, so a large directory is read whole. A listing that would
/// need more stops on the allocator's named out-of-memory fatal; it is never treated as complete
/// when it was cut off, which would silently drop entries.
const DIR_SNAPSHOT_MAX: u32 = 1 << 30;

/// Byte length of a WASI Preview 1 `dirent` header: `d_next`, `d_ino`, `d_namlen`, `d_type`.
const DIRENT_HEADER: u32 = 24;

/// Proves the tables above fit the scratch region they are carved from.
///
/// Nothing at runtime checks this: a table that runs past the reserved region simply writes into
/// the module's static data, and the walk then reads back whatever was written over it. Adding a
/// table without growing `FLOAT_SCRATCH_SIZE` is the mistake, so the bound is asserted here.
const _: () = assert!(
    DIR_DOTS + 4 <= super::runtime::FLOAT_SCRATCH_SIZE,
    "the directory tables overrun the float-scratch region"
);

/// Adds the directory-iteration runtime to `wm`.
///
/// Requires the WASI `fd_readdir`, `fd_close`, `path_open` and `fd_prestat_get` imports, the heap
/// and mixed-cell runtimes, and `files`'s `__rt_wasi_dirfd`.
pub(super) fn emit_dir_runtime(wm: &mut WatModule) {
    // The two names `readdir` reports itself. A one-byte read of this is `.` and a two-byte read
    // is `..`, so one pair of bytes serves both and no runtime store is needed.
    wm.add_data(DataSegment {
        offset: super::runtime::FLOAT_SCRATCH_BASE + DIR_DOTS,
        bytes: b"..".to_vec(),
    });
    wm.add_raw_func(&rt_dir_state());
    wm.add_raw_func(&rt_opendir());
    wm.add_raw_func(&rt_dir_snapshot());
    wm.add_raw_func(&rt_readdir());
    wm.add_raw_func(&rt_rewinddir());
    wm.add_raw_func(&rt_closedir());
}

/// The per-fd state accessors, and the fd a directory handle carries.
///
/// `__rt_dir_fd` mirrors `__rt_stream_fd`: a cell that is not a resource — including the `false`
/// a failed `opendir` answers — reads as -1, and every caller answers PHP's own failure value
/// rather than operating on a fd it does not have. An fd outside the table has no record, so the
/// walk ends rather than addressing past it.
fn rt_dir_state() -> String {
    r#"(func $__rt_dir_fd (param $cell i32) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (if (i32.eqz (local.get $cell)) (then (return (i32.const -1))))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.ne (local.get $tag) (i64.const 9)) (then (return (i32.const -1))))  ;; tag 9 = resource
  (call $__rt_handle_fd (local.get $lo)))

(func $__rt_dir_record (param $fd i32) (result i32)
  (call $__rt_fd_addr (local.get $fd) (i32.const 4)))

(func $__rt_dir_reset (param $fd i32)
  (local $record i32)
  (local.set $record (call $__rt_dir_record (local.get $fd)))
  (if (i32.eqz (local.get $record)) (then (return)))
  (if (i32.load (local.get $record))
    (then (call $__rt_heap_free (i32.load (local.get $record)))))
  (i64.store (local.get $record) (i64.const 0))
  (i64.store offset=8 (local.get $record) (i64.const 0)))
"#
    .to_string()
}

/// `__rt_opendir`: opens a directory and boxes its fd as a PHP resource.
///
/// `O_DIRECTORY` is what makes this refuse a regular file the way php's `opendir` does, and
/// `FD_READDIR` is the one right the handle needs — asking for more than the preopen holds is how
/// `path_open` comes back `ENOTCAPABLE` on a read-only mount.
///
/// Any failure answers boxed `false`, which is php's answer for a directory it cannot open. No
/// WARNING is emitted with it — php-src prints `opendir(x): Failed to open directory: ...` and the
/// native backend prints nothing, so staying silent is what keeps the two Elephc targets in
/// agreement, the same trade `__rt_warn_fopen_failed` documents for its own wording.
///
/// The record is reset rather than trusted: WASI recycles fd numbers, and a stale snapshot would
/// hand this handle the previous one's directory.
fn rt_opendir() -> String {
    format!(
        r#"(func $__rt_opendir (param $path i32) (param $path_len i64) (result i32)
  (local $dirfd i32) (local $fd i32)
  (local.set $dirfd (call $__rt_wasi_dirfd))
  (if (i32.lt_s (local.get $dirfd) (i32.const 0))                 ;; no preopen -> no filesystem
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (if (i32.ne (call $wasi_path_open
        (local.get $dirfd)
        (i32.const 1)                                             ;; LOOKUP_SYMLINK_FOLLOW
        (local.get $path)
        (i32.wrap_i64 (local.get $path_len))
        (i32.const 2)                                             ;; O_DIRECTORY
        (i64.const 16384)                                         ;; FD_READDIR
        (i64.const 16384)                                         ;; inheriting: the same
        (i32.const 0)
        (i32.add (global.get $__float_scratch) (i32.const {opened})))
      (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $fd (i32.load (i32.add (global.get $__float_scratch) (i32.const {opened}))))
  ;; An fd past the tracked range is php's EMFILE: close it and fail like any unopenable path.
  (if (i32.ge_u (local.get $fd) (i32.const {limit}))
    (then
      (drop (call $wasi_fd_close (local.get $fd)))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_dir_reset (local.get $fd))                          ;; a fresh handle starts empty
  (call $__rt_fd_eof_clear (local.get $fd))                       ;; and not at end: a reused number drops the old flag
  (call $__rt_mixed_from_value (i64.const 9)                      ;; tag 9 = PHP resource
    (call $__rt_stream_new (local.get $fd)) (i64.const 0)))
"#,
        opened = DIR_OUT,
        limit = super::files::FD_HOST_LIMIT
    )
}

/// `__rt_dir_snapshot`: reads `fd`'s whole listing into one heap block, once.
///
/// Answers the record address with the snapshot populated, or 0 when the directory cannot be
/// read. An already-snapshotted record is returned untouched, which is what makes the cursor
/// stable across `readdir` calls.
///
/// The buffer is GROWN rather than positioned. `fd_readdir` fills as much as fits and reports how
/// much it wrote, so `used == capacity` means the listing may have been cut off and the read is
/// retried at double the size, without a size cap short of the allocator's own. That is the only portable way to get a whole listing: the cookie
/// that is supposed to resume a truncated read is ignored by node's WASI, which returns the same
/// first entry whatever it is handed.
fn rt_dir_snapshot() -> String {
    format!(
        r#"(func $__rt_dir_snapshot (param $fd i32) (result i32)
  (local $record i32) (local $cap i32) (local $buf i32) (local $used i32)
  (local.set $record (call $__rt_dir_record (local.get $fd)))
  (if (i32.eqz (local.get $record)) (then (return (i32.const 0))))
  (if (i32.load (local.get $record)) (then (return (local.get $record))))  ;; already read
  (local.set $cap (i32.const {start}))
  (block $done (loop $grow
    (local.set $buf (call $__rt_heap_alloc (local.get $cap)))
    (if (i32.eqz (local.get $buf)) (then (return (i32.const 0))))
    (if (i32.ne (call $wasi_fd_readdir
          (local.get $fd) (local.get $buf) (local.get $cap) (i64.const 0)
          (i32.add (global.get $__float_scratch) (i32.const {used_out})))
        (i32.const 0))
      (then
        (call $__rt_heap_free (local.get $buf))
        (return (i32.const 0))))
    (local.set $used (i32.load (i32.add (global.get $__float_scratch) (i32.const {used_out}))))
    (br_if $done (i32.lt_u (local.get $used) (local.get $cap)))   ;; the whole listing fit
    (if (i32.ge_u (local.get $cap) (i32.const {max}))              ;; doubling would no longer fit i32
      (then (call $__rt_heap_free (local.get $buf)) (call $__rt_oom) unreachable))  ;; elephc-trap:deterministic-oom:dir-snapshot-ceiling
    (call $__rt_heap_free (local.get $buf))
    (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))
    (br $grow)))
  (i32.store (local.get $record) (local.get $buf))
  (i32.store offset=4 (local.get $record) (local.get $used))
  (local.get $record))
"#,
        start = DIR_SNAPSHOT_START,
        max = DIR_SNAPSHOT_MAX,
        used_out = DIR_OUT + 4
    )
}

/// `__rt_readdir`: the next entry's name, or boxed `false` once the directory is exhausted.
///
/// php yields ONE name per call, including `.` and `..`, in the order the host enumerates them.
///
/// `.` and `..` are emitted by this runtime rather than taken from the host, because whether
/// `fd_readdir` enumerates them is the HOST's choice and not WASI's: wasmtime returns them and
/// node's WASI does not, so a module that simply forwarded what it was given would answer a
/// different directory listing depending on who ran it. php's contract is the POSIX one — both
/// are present — so they are reported first and then SKIPPED wherever the host also produces
/// them, which is what makes the answer the same everywhere.
///
/// Everything after them is read out of the snapshot by byte offset. Remaining bytes too few for
/// one header, or a `d_namlen` running past what was read, both end the walk — the second is a
/// listing the snapshot could not grow to hold.
fn rt_readdir() -> String {
    format!(
        r#"(func $__rt_readdir (param $handle i32) (result i32)
  (local $fd i32) (local $record i32) (local $buf i32) (local $used i32)
  (local $at i32) (local $name i32) (local $namlen i32) (local $phase i32)
  (local.set $fd (call $__rt_dir_fd (local.get $handle)))
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $record (call $__rt_dir_snapshot (local.get $fd)))
  (if (i32.eqz (local.get $record))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $phase (i32.load offset=12 (local.get $record)))
  (if (i32.lt_u (local.get $phase) (i32.const 2))                 ;; the two entries php always reports
    (then
      (i32.store offset=12 (local.get $record) (i32.add (local.get $phase) (i32.const 1)))
      (return (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (i32.add (global.get $__float_scratch) (i32.const {dots})))
        (i64.extend_i32_u (i32.add (local.get $phase) (i32.const 1)))))))
  (local.set $buf (i32.load (local.get $record)))
  (local.set $used (i32.load offset=4 (local.get $record)))
  (local.set $at (i32.load offset=8 (local.get $record)))
  (block $end (loop $entry
    (br_if $end (i32.gt_u (i32.add (local.get $at) (i32.const {header})) (local.get $used)))
    (local.set $namlen (i32.load offset=16 (i32.add (local.get $buf) (local.get $at))))
    (local.set $name (i32.add (i32.add (local.get $buf) (local.get $at)) (i32.const {header})))
    (br_if $end (i32.gt_u
      (i32.add (i32.add (local.get $at) (i32.const {header})) (local.get $namlen))
      (local.get $used)))
    (local.set $at
      (i32.add (i32.add (local.get $at) (i32.const {header})) (local.get $namlen)))
    (i32.store offset=8 (local.get $record) (local.get $at))      ;; consumed either way
    (br_if $entry (call $__rt_dir_name_is_dot                     ;; already reported above
      (local.get $name) (local.get $namlen)))
    (return (call $__rt_mixed_from_value (i64.const 1)            ;; tag 1 = string
      (i64.extend_i32_u (local.get $name))
      (i64.extend_i32_u (local.get $namlen))))))
  (i32.store offset=8 (local.get $record) (local.get $used))      ;; exhausted
  (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))

(func $__rt_dir_name_is_dot (param $p i32) (param $namlen i32) (result i32)
  (if (i32.eq (local.get $namlen) (i32.const 1))
    (then (return (i32.eq (i32.load8_u (local.get $p)) (i32.const 46)))))
  (if (i32.eq (local.get $namlen) (i32.const 2))
    (then (return (i32.and
      (i32.eq (i32.load8_u (local.get $p)) (i32.const 46))
      (i32.eq (i32.load8_u offset=1 (local.get $p)) (i32.const 46))))))
  (i32.const 0))
"#,
        dots = DIR_DOTS,
        header = DIRENT_HEADER
    )
}

/// `__rt_rewinddir`: sends the handle's cursor back to the first entry.
///
/// The snapshot is DROPPED rather than rewound, so the next `readdir` reads the directory again.
/// php's `rewinddir` rewinds the underlying stream, and a file created since `opendir` shows up
/// on the second pass; keeping the old snapshot would hide it.
///
/// php answers null whatever happens, including for a handle that is not a directory, so there is
/// nothing to report and nothing to fail.
fn rt_rewinddir() -> String {
    r#"(func $__rt_rewinddir (param $handle i32) (result i64)
  (local $fd i32)
  (local.set $fd (call $__rt_dir_fd (local.get $handle)))
  (if (i32.ge_s (local.get $fd) (i32.const 0))
    (then (call $__rt_dir_reset (local.get $fd))))
  (i64.const 0))
"#
    .to_string()
}

/// `__rt_closedir`: releases the handle's fd, its snapshot and its record.
///
/// The record is zeroed rather than left behind because WASI recycles fd numbers: the next
/// `opendir` handed this same number back would otherwise resume the closed directory's walk.
/// `opendir` resets it too, so neither half depends on the other having run.
fn rt_closedir() -> String {
    r#"(func $__rt_closedir (param $handle i32) (result i64)
  (local $fd i32)
  (local.set $fd (call $__rt_dir_fd (local.get $handle)))
  (if (i32.ge_s (local.get $fd) (i32.const 0))
    (then
      (call $__rt_dir_reset (local.get $fd))
      (drop (call $wasi_fd_close (local.get $fd)))
      (call $__rt_stream_cell_close (local.get $handle))))         ;; every copy of the handle now answers closed
  (i64.const 0))
"#
    .to_string()
}
