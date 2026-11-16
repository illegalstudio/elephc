---
title: Upload progress owns snapshots independently of session reloads
author: Guillaumeloulier
date: 2026-10-21
branch: test/session-upload-progress-lifetimes
touches:
  - crates/elephc-web/src/session/upload_progress.rs
  - crates/elephc-web/src/session/upload_progress/tests/reload_tests.rs
---

## Decided
At origin/main f5b378f433feb0d8b0be7f352038f202425c77a9, the PHP upload-progress dangling-zval-pointer mechanism is absent from Elephc's standalone web tracker. Tracker metadata owns its bytes and integer offsets. Progress is serialized afresh; each short-lock file RMW uses local owned buffers, without constructing a Zend session graph or retaining interior pointers. Both worker paths share this tracker. No runtime fix was necessary; documentation and real-file regressions were added.

## Tried and ruled out
The new tests replace the session between five frames over two file parts, force body relocation, poison stored counters, vary unrelated packed arrays, and verify independent expected bytes for all three serializers with cleanup on and off. All 21 focused upload-progress tests pass on macOS ARM64. Linux x86_64 and ARM64 cargo check --tests pass; these checks are not runtime evidence.

## Watch out
Do not claim blanket immunity from PHP CVEs or memory-safety bugs. The iOS device and Simulator web crate currently fail to compile because handler_broker/control.rs gates set_no_sigpipe/send_flags/recvmsg_flags only for macOS/Linux. The device failure also reproduces with unmodified audited upload_progress.rs. See .plans/session-upload-progress-lifetime-audit-20261021.md for provenance and test boundaries.
