---
title: "hash_copy()"
description: "Clones an incremental hashing context into an independent HashContext object. Provided by the compiler-injected hash prelude in compiled code."
sidebar:
  order: 440
---

## hash_copy()

```php
function hash_copy(HashContext $context): HashContext
```

Clones an incremental hashing context into an independent HashContext object. Provided by the compiler-injected hash prelude in compiled code.

**Parameters**:
- `$context` (`HashContext`)

**Returns**: `HashContext`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected hash prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_copy.md).
