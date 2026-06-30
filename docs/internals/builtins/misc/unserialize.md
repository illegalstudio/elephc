---
title: "unserialize() — internals"
description: "Compiler internals for unserialize(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 285
---

## `unserialize()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/serialize.rs`:164](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/serialize.rs#L164) (`lower_unserialize`)
- **Function symbol**: `lower_unserialize()`


### Lowering notes

- Lowers `unserialize($data, $options?)` into the shared unserialize runtime helper.
- The source string is parsed by `__rt_unserialize_mixed`; a null result pointer
- (parse error or unsupported wire form) is boxed as PHP `false`. The optional
- `$options` argument is accepted but currently ignored.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_string`
- `__rt_unserialize_begin`
- `__rt_unserialize_mixed`

## Signature summary

```php
function unserialize(mixed $data, mixed $options): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `unserialize()`](../../../php/builtins/misc/unserialize.md)

