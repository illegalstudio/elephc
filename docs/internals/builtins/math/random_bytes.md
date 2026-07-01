---
title: "random_bytes() — internals"
description: "Compiler internals for random_bytes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 262
---

## `random_bytes()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/random.rs`:57](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/random.rs#L57) (`lower_random_bytes`)
- **Function symbol**: `lower_random_bytes()`


### Lowering notes

- Lowers `random_bytes()` into an owned CSPRNG binary string of the given length.
- Materializes the single length operand as an integer, passes it to the
- `__rt_random_bytes` runtime helper (length in `x0` on AArch64, `rdi` on
- x86_64), and stores the returned owned string result (`x1`/`x2` on AArch64,
- `rax`/`rdx` on x86_64) into the instruction's result slot. The runtime helper
- owns allocation, the cryptographic fill, and the fatal paths for a length
- below 1 or an unavailable entropy source.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_random_bytes`

## Signature summary

```php
function random_bytes(int $length): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `random_bytes()`](../../../php/builtins/math/random_bytes.md)

