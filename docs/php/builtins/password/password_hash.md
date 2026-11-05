---
title: "password_hash()"
description: "Hashes a password."
---

## password_hash()

```php
function password_hash(string $password, string|int|null $algo, array $options = []): string
```

Hashes a password.

**Parameters**:
- `$password` (`string`)
- `$algo` (`string|int|null`)
- `$options` (`array`), default `[]`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/password/password_hash.md).
