---
title: "count_chars()"
description: "Returns byte-frequency information about a string as a tally or a byte list."
---

## count_chars()

```php
function count_chars(string $string, int $mode = 0): array|string
```

Returns byte-frequency information about a string as a tally or a byte list.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`), default `0`, optional

**Returns**: `array|string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `count_chars` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/count_chars.md).
