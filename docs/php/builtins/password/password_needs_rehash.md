---
title: "password_needs_rehash()"
description: "Reports whether a password hash needs rehashing."
---

## password_needs_rehash()

```php
function password_needs_rehash(string $hash, string $algo, array $options = []): bool
```

Reports whether a password hash needs rehashing.

**Parameters**:
- `$hash` (`string`)
- `$algo` (`string`)
- `$options` (`array`), default `[]`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_needs_rehash` is implemented in the compiler, see [the internals page](../../../internals/builtins/password/password_needs_rehash.md).
