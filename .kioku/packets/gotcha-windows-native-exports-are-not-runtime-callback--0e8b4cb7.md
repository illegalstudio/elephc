---
id: gotcha-windows-native-exports-are-not-runtime-callback--0e8b4cb7
type: gotcha
title: "Windows native exports are not runtime callback entries"
description: "Outbound eval scope set unset free already receive MS x64 arguments; do not apply a second MS-to-SysV callback thunk"
tags: [windows, abi]
created: 2026-10-11
verified_by: "cargo +1.98 test -p elephc --lib scope_release"
stale_when: "Scope export ABI or caller argument convention changes"
sources:
  - path: src/codegen_support/runtime/eval_bridge/scope_release.rs
    blob: a23fdfd389d6e38f26398ff78328cfc006a92d31
---

# Windows native exports are not runtime callback entries

## Fact

Scope mutation adapters are reached through the target-native C ABI planner. They use an ordinary target export label; genuine Rust-to-generated callbacks retain the callback adapter. Windows scope-set forwards words five and six after 32-byte shadow space and propagates an owned Throwable only after Rust returns.
