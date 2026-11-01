---
title: PHP stdin reads require duplicated descriptors and shared native eval cursors
author: Guillaume
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - src/codegen_support/runtime/io/file_get_contents_stdin.rs
  - src/codegen/lower_inst/builtins/io/phar_read.rs
  - crates/elephc-magician/src/stream_resources/operations.rs
  - tests/codegen/io/streams.rs
---

## Decided
`file_get_contents("php://stdin")` must duplicate fd 0 and read to EOF through shared bounded/unbounded stream readers, rather than stat the URL as a filesystem path. Persist the returned bytes before scratch reuse and close only the duplicate. Byte caps consume only their range. Seek before reading when an offset is requested; a pipe seek failure must not consume input. The eval path needs the same behavior and must recognize native STDIN payload zero, separately from eval-owned resource ids. Descriptor cloning retains close-on-exec.

Eight host-native regressions pass, including opaque eval, large input, bounded/empty reads, regular-file offsets, non-seekable pipes, negative lengths and a closed descriptor. One emission test checks all five target ABIs, including the final punctuation revision. These are reducer/emission results; a fresh Symfony build and Linux execution remain separate gates.

## Tried and ruled out
Calling a nonexistent __rt_fclose fails native linking; close the duplicated descriptor through libc. Fixing only eval file_get_contents leaves stream_get_contents(STDIN) unable to see the native cursor. Reading all bytes then slicing violates bounded-read cursor semantics.

## Watch out
The 377 stream tests yielded 368 passed, eight ignored and one existing callback failure. The named-params notification callback SIGSEGV also reproduces with the pre-stdin frozen compiler. It remains unfixed, as does the independently traced Symfony vault-list lifetime defect. Evidence and exact hashes are in target/framework-audit-20261020.Wcomr7 and .plans/symfony-web-execution-ledger.md. The old Symfony executable does not contain the stdin changes.

ASCII folding must apply only to letters: folding colon or slash makes control bytes 0x1a/0x0f alias URL punctuation. PHP rejects those paths and preserves input. The first optimized stdin artifact reproduced the aliases; both architecture probes now compare punctuation exactly, with a dedicated end-to-end regression. Its pre-fix artifact and oracle remain in the audit directory.
