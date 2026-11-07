---
title: "mysqli_change_user()"
description: "Changes the user of the database connection."
---

## mysqli_change_user()

```php
function mysqli_change_user(mixed $mysql, string $username, string $password, ?string $database): bool
```

Changes the user of the database connection.

**Parameters**:
- `$mysql` (`mixed`)
- `$username` (`string`)
- `$password` (`string`)
- `$database` (`?string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_change_user` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_change_user.md).
