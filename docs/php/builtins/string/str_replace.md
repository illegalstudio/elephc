---
title: "str_replace()"
description: "Replaces all occurrences of a search string with a replacement string."
---

## str_replace()

```php
function str_replace(string|array $search, string|array $replace, string|array $subject, int $count = null): string
```

Replaces all occurrences of a search string with a replacement string.

**Parameters**:
- `$search` (`string|array`)
- `$replace` (`string|array`)
- `$subject` (`string|array`)
- `$count` (`int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_replace.md).
