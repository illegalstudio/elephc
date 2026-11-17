---
title: "mb_ereg_search_regs()"
description: "Returns the next multibyte regex match and captured groups, preserving numeric and named keys."
---

## mb_ereg_search_regs()

```php
function mb_ereg_search_regs(?string $pattern = null, ?string $options = null): array|false
```

Returns the next multibyte regex match and captured groups, preserving numeric and named keys.

**Parameters**:
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_regs` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_regs.md).
