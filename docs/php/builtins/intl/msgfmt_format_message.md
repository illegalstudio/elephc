---
title: "msgfmt_format_message()"
description: "PHP ext/intl `msgfmt_format_message()` backed by the managed ICU bridge."
---

## msgfmt_format_message()

```php
function msgfmt_format_message(string $locale, string $pattern, array $values): string|false
```

PHP ext/intl `msgfmt_format_message()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)
- `$pattern` (`string`)
- `$values` (`array`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `msgfmt_format_message` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/msgfmt_format_message.md).
