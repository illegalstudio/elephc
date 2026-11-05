---
title: "password_hash()"
description: "Hashes a password."
---

## password_hash()

```php
function password_hash(string $password, string $algo = PASSWORD_DEFAULT, array $options = []): string|false
```

Hashes a password.

**Parameters**:
- `$password` (`string`)
- `$algo` (`string`), default `PASSWORD_DEFAULT`, optional
- `$options` (`array`), default `[]`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/password/password_hash.md).
