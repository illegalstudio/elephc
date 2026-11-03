---
title: "transliterator_transliterate()"
description: "PHP ext/intl `transliterator_transliterate()` backed by the managed ICU bridge."
---

## transliterator_transliterate()

```php
function transliterator_transliterate(mixed $transliterator, string $string, int $start = 0, int $end = -1): string|false
```

PHP ext/intl `transliterator_transliterate()` backed by the managed ICU bridge.

**Parameters**:
- `$transliterator` (`mixed`)
- `$string` (`string`)
- `$start` (`int`), default `0`, optional
- `$end` (`int`), default `-1`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `transliterator_transliterate` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/transliterator_transliterate.md).
