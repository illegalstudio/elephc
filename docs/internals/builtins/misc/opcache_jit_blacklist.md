---
title: "opcache_jit_blacklist() - internals"
description: "Compiler internals for opcache_jit_blacklist(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 647
---

## `opcache_jit_blacklist()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:1037](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L1037) (`opcache_jit_blacklist`)
- **Function symbol**: `opcache_jit_blacklist()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_jit_blacklist(mixed $closure): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

Calls prefer the program's native OPcache prelude declarations, which carry the configured directives, live cache, and API restrictions. Dedicated interpreter handlers cover programs without those declarations ([`crates/elephc-magician/src/interpreter/builtins/registry/dispatch/mod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/registry/dispatch/mod.rs)). This route does not use an `eval_builtin!` binding.

## Cross-references

- [User reference for `opcache_jit_blacklist()`](../../../php/builtins/misc/opcache_jit_blacklist.md)
