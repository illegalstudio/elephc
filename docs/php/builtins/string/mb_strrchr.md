---
title: "mb_strrchr()"
description: "Returns text before or from the last matching substring, or false."
---

## mb_strrchr()

```php
function mb_strrchr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false
```

Returns text before or from the last matching substring, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strrchr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strrchr.md).
