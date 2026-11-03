---
title: "grapheme_strlen()"
description: "PHP ext/intl `grapheme_strlen()` backed by the managed ICU bridge."
---

## grapheme_strlen()

```php
function grapheme_strlen(string $string): int|false|null
```

PHP ext/intl `grapheme_strlen()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)

**Returns**: `int|false|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/grapheme_strlen.md).
