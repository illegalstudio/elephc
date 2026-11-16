---
title: "tidy_repair_string()"
description: "Repairs a string of HTML and returns the repaired markup."
---

## tidy_repair_string()

```php
function tidy_repair_string(string $string, array|string|null $config = null, ?string $encoding = null): string|false
```

Repairs a string of HTML and returns the repaired markup.

**Parameters**:
- `$string` (`string`)
- `$config` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_repair_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_repair_string.md).
