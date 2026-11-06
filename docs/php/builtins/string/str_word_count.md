---
title: "str_word_count()"
description: "Counts the words in a string, or returns them as a list or byte-offset map."
---

## str_word_count()

```php
function str_word_count(string $string, int $format = 0, ?string $characters = null): array|int
```

Counts the words in a string, or returns them as a list or byte-offset map.

**Parameters**:
- `$string` (`string`)
- `$format` (`int`), default `0`, optional
- `$characters` (`?string`), default `null`, optional

**Returns**: `array|int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_word_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_word_count.md).
