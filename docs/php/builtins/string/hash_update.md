---
title: "hash_update()"
description: "Feeds data into an incremental hashing context. Provided by the compiler-injected hash prelude in compiled code."
---

## hash_update()

```php
function hash_update(HashContext $context, string $data): bool
```

Feeds data into an incremental hashing context. Provided by the compiler-injected hash prelude in compiled code.

**Parameters**:
- `$context` (`HashContext`)
- `$data` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_update` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_update.md).
