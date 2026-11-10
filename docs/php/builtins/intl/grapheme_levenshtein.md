---
title: "grapheme_levenshtein()"
description: "PHP ext/intl `grapheme_levenshtein()` backed by the managed ICU bridge."
---

## grapheme_levenshtein()

```php
function grapheme_levenshtein(string $string1, string $string2, int $insertion_cost = 1, int $replacement_cost = 1, int $deletion_cost = 1, string $locale = ''): int|false
```

PHP ext/intl `grapheme_levenshtein()` backed by the managed ICU bridge.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$insertion_cost` (`int`), default `1`, optional
- `$replacement_cost` (`int`), default `1`, optional
- `$deletion_cost` (`int`), default `1`, optional
- `$locale` (`string`), default `''`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_levenshtein` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_levenshtein.md).
