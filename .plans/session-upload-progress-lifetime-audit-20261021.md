# Session upload-progress lifetime audit — 2026-10-21

- [x] Fetch `origin/main` and rebase an isolated worktree onto it.
- [x] Compare the screenshot's two dangling-counter-pointer mechanisms with Elephc.
- [x] Trace both worker body-drain paths and the progress file RMW.
- [x] Add and run focused regressions using real session files.
- [x] Check Linux compilation and record the existing iOS compilation blocker.

## Result and provenance

The screenshot's upload-progress use-after-free mechanism does not apply to
Elephc's standalone `--web` tracker at audited upstream commit
`f5b378f433feb0d8b0be7f352038f202425c77a9`. No runtime fix is needed for this
mechanism. The change adds lifetime-invariant documentation and regression tests;
it does not change runtime behavior.

The main checkout had unrelated local changes and was 598 commits behind the
freshly fetched upstream. It was preserved. The dedicated worktree was rebased
onto the exact upstream commit above before inspection.

The PHP comparison used `ext/session/session.c` from PHP-8.4 commit
[`294f50b527ce4c3772ff78d395af28754a2379eb`](https://github.com/php/php-src/blob/294f50b527ce4c3772ff78d395af28754a2379eb/ext/session/session.c#L2984).
The PHP [release announcement](https://www.php.net/archive/2026.php#2026-10-20-1)
was also checked. This is an assessment of the mechanism shown in the screenshot,
not an assertion that all upcoming PHP security advisories have been audited.

## Why the lifetime failure is absent

The reported PHP implementation caches pointers to `bytes_processed` zvals in
arrays. Upload-progress updates reload and replace session data; destruction of
an old decoded tree can free storage those cached pointers still address.
Subsequent counter reads or writes can therefore use freed memory.

Elephc's `crates/elephc-web/src/session/upload_progress.rs` has different ownership:

- `Tracker`, `Inflight`, and `FileProgress` own names with `Vec<u8>` and hold
  counters and body offsets as integers. No pointer into a decoded session tree
  or body allocation is retained between updates.
- `advance_completed_parts` records completed files by value and resets the
  in-flight metadata at each new boundary. The next file has fresh metadata.
- `files` clones completed snapshots and calculates in-flight bytes from the
  current body length and `content_start`. Body reallocations preserve offsets.
- `serialize_progress` constructs a new owned byte buffer for every write.
- `rmw` opens and locks the session file, reads its bytes into a local `Vec`,
  splices the new entry into another owned buffer, writes, unlocks, and closes.
  Session walkers use temporary checked slices of that local buffer. They do
  not instantiate a PHP/Zend object graph or invoke user deserialization hooks.
- Reloading the file can replace all its serialized data without freeing any
  storage owned by the tracker. File locks protect the RMW's consistency; the
  absence of cached interior pointers is the memory-lifetime protection.
- Both `worker.rs` and `isolated_worker.rs` use the same tracker. They call
  `update` while accumulating frames and `complete` before the request handler.
  The tracker uses an independent file descriptor, not `state::SESSION_FD`.

This conclusion is specific to this path. Generated code, other runtime helpers,
FFI, unsafe Rust, application deserialization, and any separate PHP deployment
require their own security analysis.

## Validation

`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo test -p elephc-web --lib
session::upload_progress:: -- --test-threads=1` passed all **21** focused tests on
macOS ARM64, including the two new regressions in
`crates/elephc-web/src/session/upload_progress/tests/reload_tests.rs`.

Each new regression covers `php`, `php_binary`, and `php_serialize`. Five body
frames force buffer relocation, repeatedly update the first file, switch to a
second file, and finish it. Before every update, the disk session is replaced
with poisoned counter data and unrelated packed arrays alternating between 149
and one element. Independent expected-byte fixtures verify counters, per-file
completion, and preservation of unrelated entries. The session is replaced once
more before `complete`, with cleanup tested both enabled and disabled.

`cargo check -p elephc-web --tests --target <target>` passed for
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`. These are compilation
checks, not Linux execution or linking evidence.

The equivalent iOS device and Simulator checks fail on existing missing
`set_no_sigpipe`, `send_flags`, and `recvmsg_flags` definitions in
`crates/elephc-web/src/handler_broker/control.rs`. Its platform helpers are gated
only for macOS and Linux. The device failure was reproduced with the original
unmodified `upload_progress.rs` from the audited commit, then the audit changes
were restored. These unrelated failures are not introduced by the tests and
prevent claiming a green complete supported-target matrix.

`git diff --cached --check` passed. `trail check --base origin/main` reported 15
keyword-based conflicts, including treating this audit's own successful reload
tests as a rejected approach and matching unrelated COW, CI, and graph-cache
notes. The reported excerpts do not prohibit these tests or contradict the
recorded results; no prior technical decision is intentionally overridden.
The check's nonzero result is retained rather than reported as a pass.
