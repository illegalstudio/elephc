---
title: x86 throw entry alignment and the hosted request sweep
author: Guillaume
date: 2026-10-28
branch: fix/php-ext-followups
---

## Decided
- `__rt_throw_current` (x86_64) realigns `rsp` (`and rsp, -16`) after its pushes: compiled code reaches it by `call`, `jmp` and conditional branch, so the entry phase varies, and its cleanup walk runs destructors that can reach Rust (a hosted wrapper owner's `elephc_php_ext_object_release`). An 8-byte-off stack faulted on hashbrown's SSE `movaps` (#2116 review P0: readonly write inside a method, linux-x86_64 only).
- The engine's request sweep now records non-persistent strings, arrays/hash storage and objects (php-src's `pemalloc` semantics). Anything the engine keeps across requests must be persistent: `intern()` allocates persistently; `EG(regular_list)` and `EG(symbol_table)` are destroyed and re-initialized at request end instead of cleaned.
- Hosted wrapper bookkeeping properties are named `__elephc.handle` / `__elephc.owner` (no PHP identifier has a `.`), and runtime-name property ladders skip them (`prelude::is_host_plumbing_property`).
- Name lookups hide only the extension host's symbols (`prelude::is_host_internal_symbol`), never every `__elephc*`: PDO registers `__ElephcPDOSqliteBlobStream` / `__ElephcPDOPgsqlLobStream` with `stream_wrapper_register(…, self::class)`.

## Tried and ruled out
- Cross-compiling a hosted program from macOS: `elephc extension install --target linux-x86_64` is refused (a hosted extension's surface is read by running it). Build elephc itself for x86_64-unknown-linux-musl (the test image is Alpine) with RUSTUP_TOOLCHAIN=1.98 and docker gcc/ar wrappers as CC_/AR_/linker, then run elephc inside the image (elephc-scratch/tools/x86musl.sh, x86musl-gdb.sh).

## Watch out
- `cargo build --bin elephc` does not rebuild `target/debug/libelephc_*.a`; a probe links stale runtime archives. Rebuild them with `cargo build --lib -p <crate>`.
- `elephc extension add` walks up to a parent `elephc.toml` (elephc-scratch has one): run probes in a directory with none above it.
- In gdb, a breakpoint on an asm symbol with `push rbp; mov rbp, rsp` lands at +4: add 8 to the printed rsp for the entry phase.
