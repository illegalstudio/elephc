---
title: "str_ireplace()"
description: "Case-insensitive version of str_replace()."
---

## str_ireplace()

```php
function str_ireplace(string $search, string $replace, string $subject, int $count = null): string
```

Case-insensitive version of str_replace().

**Parameters**:
- `$search` (`string`)
- `$replace` (`string`)
- `$subject` (`string`)
- `$count` (`int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_ireplace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_ireplace.md).
