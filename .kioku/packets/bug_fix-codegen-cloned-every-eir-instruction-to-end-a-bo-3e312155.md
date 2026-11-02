---
id: bug_fix-codegen-cloned-every-eir-instruction-to-end-a-bo-3e312155
type: bug_fix
title: "Codegen cloned every EIR instruction to end a borrow that was never in the way"
description: "lower_instruction fetched each instruction with .cloned(), copying its operand Vec and its PhpType once per instruction of every body; FunctionContext::function is a &'a Function so copying the reference out first removes it, byte-identically"
created: 2026-09-20
sources:
  - path: src/codegen/lower_inst.rs
    blob: 5639821a127d267b5abaaf3b92557b83eb68f645
    lines: 155-170
    snip: c06822e672aa
    anchor: "pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {"
  - path: src/codegen/block_emit.rs
    blob: fa4f3ba2773a110454a6339a133586d2b796039f
    lines: 1777-1790
    snip: 993bfd4c2e72
    anchor: "fn emit_blocks(ctx: &mut FunctionContext<'_>) -> Result<()> {"
---

# Codegen cloned every EIR instruction to end a borrow that was never in the way

## Fact

Sampling the codegen phase again -- after the five whole-module scans were fixed -- gave a
completely different ranking:

    _xzm_free 2816   _platform_memmove 2280   PhpType::clone 1160   _xzm_xzone_malloc 1007
    Instruction::clone 788   drop_in_place<PhpType> 672   drop_in_place<Instruction> 431
    ExprKind::clone 426

Memory management was 64% of the non-idle samples and the IR clones above it were 26%, with the
first largely caused by the second. The cause:

    let inst = ctx.function.instruction(inst_id).cloned()...   // one Instruction per instruction

`Instruction` owns its operand `Vec` and its `PhpType`, and every `lower_*` arm takes `&inst` --
the clone existed only so the immutable borrow of `ctx.function` would not collide with the
`&mut ctx` the arms take. It does not have to: `FunctionContext::function` is a `&'a Function`,
a reference whose lifetime is the struct's parameter and NOT the borrow of `ctx`. Copying it
into a local first decouples them:

    let function = ctx.function;
    let inst = function.instruction(inst_id).ok_or_else(...)?;

221 match arms then pass `inst` instead of `&inst`, and it compiles with no other change.
`emit_blocks` had the same shape (`ctx.function.blocks.clone()`, every block of every body) and
takes the same fix.

Measured on the Symfony `--web` build, `.s` byte-identical (1396032370 / 42114361 / 178246552):

    Generating native code   40.37s -> 35.90s
    phase total              91.24s -> 85.30s
    user CPU                 91.03s -> 78.57s

**Look for this shape wherever a `&'a`-borrowed field is cloned "to end the borrow".** It is the
third instance this session: the reachability graph cloned a whole `ClassNode` per implementing
class for the same reason, and `seed_live_methods` cloned the live-class set per round. When the
field is a shared reference, copy the reference; when it is owned, `std::mem::take` the thing
being WRITTEN, never the index being read.

## Why

The old whole-module scans are gone from the codegen profile; what replaced them at the top was IR cloning, and the cause was one line.
