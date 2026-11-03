---
title: "grapheme_str_split()"
description: "PHP ext/intl `grapheme_str_split()` backed by the managed ICU bridge."
---

## grapheme_str_split()

```php
function grapheme_str_split(string $string, int $length = 1): array|false
```

PHP ext/intl `grapheme_str_split()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `1`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_str_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_str_split.md).
