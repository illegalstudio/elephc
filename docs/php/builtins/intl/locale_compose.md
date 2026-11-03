---
title: "locale_compose()"
description: "PHP ext/intl `locale_compose()` backed by the managed ICU bridge."
---

## locale_compose()

```php
function locale_compose(array $subtags): string|false
```

PHP ext/intl `locale_compose()` backed by the managed ICU bridge.

**Parameters**:
- `$subtags` (`array`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_compose` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_compose.md).
