---
title: "grapheme_extract()"
description: "PHP ext/intl `grapheme_extract()` backed by the managed ICU bridge."
---

## grapheme_extract()

```php
function grapheme_extract(string $haystack, int $size, int $type = GRAPHEME_EXTR_COUNT, int $offset = 0, mixed $next = null): string|false
```

PHP ext/intl `grapheme_extract()` backed by the managed ICU bridge.

**Parameters**:
- `$haystack` (`string`)
- `$size` (`int`)
- `$type` (`int`), default `GRAPHEME_EXTR_COUNT`, optional
- `$offset` (`int`), default `0`, optional
- `$next` (`mixed`), passed by reference, default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_extract` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_extract.md).
