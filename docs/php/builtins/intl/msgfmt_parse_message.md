---
title: "msgfmt_parse_message()"
description: "PHP ext/intl `msgfmt_parse_message()` backed by the managed ICU bridge."
---

## msgfmt_parse_message()

```php
function msgfmt_parse_message(string $locale, string $pattern, string $message): array|false
```

PHP ext/intl `msgfmt_parse_message()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)
- `$pattern` (`string`)
- `$message` (`string`)

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `msgfmt_parse_message` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/msgfmt_parse_message.md).
