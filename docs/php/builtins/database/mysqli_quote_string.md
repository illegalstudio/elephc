---
title: "mysqli_quote_string()"
description: "Escapes a string and wraps it in single quotes."
---

## mysqli_quote_string()

```php
function mysqli_quote_string(mixed $mysql, string $string): string
```

Escapes a string and wraps it in single quotes.

**Parameters**:
- `$mysql` (`mixed`)
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_quote_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_quote_string.md).
