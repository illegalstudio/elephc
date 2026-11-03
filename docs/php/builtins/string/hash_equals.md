---
title: "hash_equals()"
description: "Compares two strings using a constant-time algorithm."
---

## hash_equals()

```php
function hash_equals(string $known_string, string $user_string): bool
```

Compares two strings using a constant-time algorithm.

**Parameters**:
- `$known_string` (`string`)
- `$user_string` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_equals` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_equals.md).
