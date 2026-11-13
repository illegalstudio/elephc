---
title: "substr()"
description: "Returns a portion of a string specified by the offset and length."
---

## substr()

```php
function substr(string $string, int $offset, ?int $length = null): string
```

Returns a portion of a string specified by the offset and length.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr.md).
