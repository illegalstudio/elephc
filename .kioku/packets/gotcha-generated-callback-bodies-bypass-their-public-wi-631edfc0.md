---
id: gotcha-generated-callback-bodies-bypass-their-public-wi-631edfc0
type: gotcha
title: "Generated callback bodies bypass their public Windows adapters"
description: "Nested eval constructors builtin dispatch and output already have internal arguments while Rust entries need exact C adapters"
tags: [windows, abi]
created: 2026-10-11
verified_by: "cargo +1.98 test -p elephc --lib --features curl codegen_support::runtime::eval_bridge::"
stale_when: "Generated callback body convention or public callback adapter contract changes"
sources:
  - path: src/codegen_support/runtime/eval_bridge/string_literal.rs
    blob: 5e9d71368a3ec06657b2e7c90e77947c7ff8ee62
    lines: 14-36
    snip: 1bcd6aa89eea
    anchor: "pub(crate) fn emit(emitter: &mut Emitter) {"
  - path: src/codegen_support/runtime/eval_bridge/output.rs
    blob: 478b95699c96af777d9a6b852b9223f162f1ff08
    lines: 19-84
    snip: 73a2180456f9
    anchor: "fn call_generated_callback(emitter: &mut Emitter, symbol: &str) {"
  - path: src/codegen_support/runtime/eval_bridge/runtime_builtin_dispatch.rs
    blob: f872f382b259bf710a5655392b909a9fea5d69c8
    lines: 18-23
    snip: 00a308afa7e0
    anchor: "fn call_generated_callback(emitter: &mut Emitter, symbol: &str) {"
---

# Generated callback bodies bypass their public Windows adapters

## Fact

A generated callback body has already normalized native C arguments. Nested calls to other generated callbacks must target c_callback_internal_symbol, not bl_c(public), or Windows adapts the arguments a second time. Literal string construction previously converted a payload pointer into an allocation length. Dispatcher and protected output bodies share this boundary. Actual Rust callers still require the public C entry: output_v1 has exactly one request argument and preserves its protected status/frame behind a distinct internal implementation. Focused eval bridge tests pass 33 with two assembler tests ignored; Windows executable acceptance is a separate gate.
