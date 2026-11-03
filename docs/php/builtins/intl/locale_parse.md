---
title: "locale_parse()"
description: "PHP ext/intl `locale_parse()` backed by the managed ICU bridge."
---

## locale_parse()

```php
function locale_parse(string $locale): ?array
```

PHP ext/intl `locale_parse()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)

**Returns**: `?array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_parse.md).
