---
title: "grapheme_substr()"
description: "PHP ext/intl `grapheme_substr()` backed by the managed ICU bridge."
---

## grapheme_substr()

```php
function grapheme_substr(string $string, int $offset, ?int $length = null, string $locale = ''): string|false
```

PHP ext/intl `grapheme_substr()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`?int`), default `null`, optional
- `$locale` (`string`), default `''`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_substr.md).
