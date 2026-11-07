---
title: "msgfmt_parse()"
description: "PHP ext/intl `msgfmt_parse()` backed by the managed ICU bridge."
---

## msgfmt_parse()

```php
function msgfmt_parse(object $formatter, string $string): array|false
```

PHP ext/intl `msgfmt_parse()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$string` (`string`)

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `msgfmt_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/msgfmt_parse.md).
