---
title: "grapheme_stripos()"
description: "PHP ext/intl `grapheme_stripos()` backed by the managed ICU bridge."
---

## grapheme_stripos()

```php
function grapheme_stripos(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false
```

PHP ext/intl `grapheme_stripos()` backed by the managed ICU bridge.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$locale` (`string`), default `''`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_stripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_stripos.md).
