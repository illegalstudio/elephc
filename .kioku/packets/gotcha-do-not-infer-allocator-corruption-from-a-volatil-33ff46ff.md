---
id: gotcha-do-not-infer-allocator-corruption-from-a-volatil-33ff46ff
type: gotcha
title: "Do not infer allocator corruption from a volatile Rust return"
description: "mbstring identity retirement preserves all registers; failing Windows eval needs live continuation evidence"
tags: [windows, abi]
created: 2026-10-11
stale_when: "Identity adapter save layout or caller convention changes"
sources:
  - path: src/codegen_support/runtime/strings/mbstring/ini_identity.rs
    blob: 949e7ff876e68c97267882bcab2cc2598482b97d
---

# Do not infer allocator corruption from a volatile Rust return

## Fact

The x86_64 native identity adapter saves all fourteen non-frame GPRs and sixteen XMM registers, then restores rsp from rbp-112. A Rust callback may clobber RAX internally, but that does not establish a clobber visible to heap_free. Validate the actual adapter and live saved continuation before adding redundant spills.
