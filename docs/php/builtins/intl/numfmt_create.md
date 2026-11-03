---
title: "numfmt_create()"
description: "PHP ext/intl `numfmt_create()` backed by the managed ICU bridge."
---

## numfmt_create()

```php
function numfmt_create(string $locale, int $style, ?string $pattern = null): ?object
```

PHP ext/intl `numfmt_create()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)
- `$style` (`int`)
- `$pattern` (`?string`), default `null`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_create.md).
