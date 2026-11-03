---
title: "mb_substr_count()"
description: "Counts non-overlapping occurrences of an encoded substring."
---

## mb_substr_count()

```php
function mb_substr_count(string $haystack, string $needle, ?string $encoding = null): int
```

Counts non-overlapping occurrences of an encoded substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substr_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substr_count.md).
