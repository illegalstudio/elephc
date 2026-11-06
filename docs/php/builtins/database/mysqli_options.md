---
title: "mysqli_options()"
description: "Sets a connection option before connecting."
---

## mysqli_options()

```php
function mysqli_options(mixed $mysql, int $option, mixed $value): bool
```

Sets a connection option before connecting.

**Parameters**:
- `$mysql` (`mixed`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_options.md).
