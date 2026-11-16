---
title: "jdtojewish()"
description: "Converts a Julian Day count into a Jewish date string."
---

## jdtojewish()

```php
function jdtojewish(int $julian_day, bool $hebrew = false, int $flags = 0): string
```

Converts a Julian Day count into a Jewish date string.

**Parameters**:
- `$julian_day` (`int`)
- `$hebrew` (`bool`), default `false`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtojewish` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtojewish.md).
