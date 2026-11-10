---
title: "datefmt_format_object()"
description: "PHP ext/intl `datefmt_format_object()` backed by the managed ICU bridge."
---

## datefmt_format_object()

```php
function datefmt_format_object(mixed $datetime, mixed $format = null, ?string $locale = null): string|false
```

PHP ext/intl `datefmt_format_object()` backed by the managed ICU bridge.

**Parameters**:
- `$datetime` (`mixed`)
- `$format` (`mixed`), default `null`, optional
- `$locale` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_format_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_format_object.md).
