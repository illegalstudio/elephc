---
id: gotcha-load-at-offset-used-x9-as-scratch-past-a-255-byt-d502c547
type: gotcha
title: "load_at_offset used x9 as scratch past a 255-byte frame, clobbering callers that hold x9"
description: "AArch64 ldur reaches 255 bytes; beyond it the address was built in x9 unconditionally, so adding one local silently broke a caller keeping a value in x9"
created: 2026-09-22
sources:
  - path: src/codegen_support/abi/frame.rs
    blob: 427270bb39181a99675a0777e30b4abdb5bab265
    lines: 200-235
    snip: 69d2483dfa7a
    anchor: "pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {"
  - path: src/codegen/lower_inst/globals_constants.rs
    blob: fd8ad3ca689b63c4fd0beef1974e478214fdf00f
    lines: 367-386
    snip: 967f85f3fb25
    anchor: "pub(super) fn lower_invoker_ref_arg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {"
---

# load_at_offset used x9 as scratch past a 255-byte frame, clobbering callers that hold x9

## Fact

`abi::load_at_offset(reg, offset)` emits `ldur reg, [x29, #-offset]` up to 255, and past that
materialized the address in a scratch register -- always `x9`. `x9` is also
`abi::symbol_scratch_reg`, and `lower_invoker_ref_arg` keeps the marker's SOURCE TAG there across
`materialize_local_storage_address`, which calls `load_at_offset` for ref-cell and dynamic slots.

MEASURED against php 8.5.10:
    foreach ($a as &$v) { $p($v); }           p(1);p(2);   correct
    foreach ($a as $k => &$v) { $p($v); }      p();p();     -- the unused KEY pushed the slot past
                                                               -256 and the source tag became an
                                                               address

FIX: `load_at_offset` computes the address INTO THE DESTINATION register (which it is about to
overwrite anyway); only a float destination keeps x9. x86_64 has no scratch at all (any offset
fits `[rbp - n]`). `store_at_offset` still uses x9 by necessity -- the source must survive -- so a
caller holding x9 across a deep STORE is still exposed.

HOW IT WAS FOUND: diffing `--emit-asm` of two probes that differed by one local, around the
`cufa_invoker_ref_cell` comment: one emitted `ldur x0, [x29, #-224]`, the other
`sub x9, x29, #280; ldr x0, [x9]` right after `mov x9, #0`.

## Why

A frame-size-dependent miscompile: the same program works or fails depending on how many locals the function has, with no diagnostic
