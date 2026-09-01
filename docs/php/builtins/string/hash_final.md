---
title: "hash_final()"
description: "Finalizes an incremental hashing context and returns the digest (hex, or raw bytes when $binary). Provided by the compiler-injected hash prelude in compiled code."
sidebar:
  order: 415
---

## hash_final()

```php
function hash_final(HashContext $context, bool $binary = false): string
```

Finalizes an incremental hashing context and returns the digest (hex, or raw bytes when $binary). Provided by the compiler-injected hash prelude in compiled code.

**Parameters**:
- `$context` (`HashContext`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected hash prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_final` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_final.md).
