---
id: gotcha-apple-s-as-is-slow-per-instruction-match-not-per-d7a7b82e
type: gotcha
title: "Apple's as is slow per instruction match, not per line: pre-encoded .inst words cut its CPU fivefold"
description: "mov x0, x20 costs ~47us to assemble, a .word 0.2us; the slice render hands frequent symbol-free AArch64 forms to as as .inst, with byte-identical objects"
created: 2026-09-27
sources:
  - path: src/linker/aarch64_encode.rs
    blob: 506c49a027d706b049966d789a8fe435bb75d440
    lines: 1-20
    snip: a9385b28e26a
---

# Apple's as is slow per instruction match, not per line: pre-encoded .inst words cut its CPU fivefold

## Fact

Apple's `as` (clang 21 integrated assembler) is not slow per line, it is slow per instruction
MATCH: 1.1 million copies of one line took 0.2 s as `.word`, 0.9 s as `movz x0, #1`, 6.9 s as
`mov x0, #1` and 51 s as `mov x0, x20` (the register-move alias resolution is ~47 us each). The
Symfony program is 37.5 million instructions, so plain text cost ~270 CPU-seconds of `as` per build.

The assembler split therefore hands the frequent symbol-free AArch64 forms to `as` already encoded,
as `.inst 0x...` (src/linker/aarch64_encode.rs), only inside the internal slices; the `.s` a
developer asks for is untouched. The object is byte-for-byte the same: checked by assembling every
Symfony slice both ways (18/18 objects identical, the 188 MB binaries identical) and on the example
programs. `as` CPU went 272 s -> 53 s.

Rules the encoder must keep, learned by diffing against `as`:
- mov #imm: MOVZ whenever one 16-bit chunk holds the value, else MOVN; a bitmask value stays text.
- a hex immediate like 0xffff000000000000 must be parsed as u64 (it overflows i64).
- ldr/str: scaled unsigned offset when aligned and in range, else LDUR/STUR for -256..255.
- anything with sp in a register form, a negative add/cmp immediate, or a writeback onto the
  transferred register stays text.

To extend it: add lines to scratch gen-encode-cases.py style (assemble with `as`, read words with
`otool -t`), regenerate aarch64_encode_cases.rs, then re-run the both-ways object comparison.
ELEPHC_ASM_ENCODE=0 turns the encoding off.

## Why

An encoder rule that differs from as silently miscompiles, and the codegen suite does not assemble through the split; any new form must be checked against as and by the both-ways object comparison
