---
title: "hash_copy()"
description: "Clones an incremental hashing context into an independent HashContext object. Provided by the compiler-injected hash prelude in compiled code."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_copy.md).
