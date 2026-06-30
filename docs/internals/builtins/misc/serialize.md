---
title: "serialize() — internals"
description: "Compiler internals for serialize(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 284
---

## `serialize()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/serialize.rs`:33](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/serialize.rs#L33) (`lower_serialize`)
- **Function symbol**: `lower_serialize()`


### Lowering notes

- Lowers `serialize($value)` into the shared serialize runtime helper.
- Scalar static types are formatted directly through `__rt_serialize_value`; a
- Mixed/Union argument is unboxed and dispatched by `__rt_serialize_mixed`.
- Non-scalar static types (arrays/objects) are not yet supported and are rejected.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_serialize_begin`
- `__rt_serialize_mixed`
- `__rt_serialize_value`

## Signature summary

```php
function serialize(mixed $value): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `serialize()`](../../../php/builtins/misc/serialize.md)

