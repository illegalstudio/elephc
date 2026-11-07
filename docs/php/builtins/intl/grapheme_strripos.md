---
title: "grapheme_strripos()"
description: "PHP ext/intl `grapheme_strripos()` backed by the managed ICU bridge."
---

## grapheme_strripos()

```php
function grapheme_strripos(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false
```

PHP ext/intl `grapheme_strripos()` backed by the managed ICU bridge.

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

For how `grapheme_strripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_strripos.md).
