---
title: "grapheme_stristr()"
description: "PHP ext/intl `grapheme_stristr()` backed by the managed ICU bridge."
---

## grapheme_stristr()

```php
function grapheme_stristr(string $haystack, string $needle, bool $beforeNeedle = false, string $locale = ''): string|false
```

PHP ext/intl `grapheme_stristr()` backed by the managed ICU bridge.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$beforeNeedle` (`bool`), default `false`, optional
- `$locale` (`string`), default `''`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_stristr` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_stristr.md).
