---
title: "substr_replace()"
description: "Replaces text within a portion of a string."
---

## substr_replace()

```php
function substr_replace(string $string, string $replace, int $offset, int $length = null): string
```

Replaces text within a portion of a string.

**Parameters**:
- `$string` (`string`)
- `$replace` (`string`)
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `substr_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr_replace.md).
