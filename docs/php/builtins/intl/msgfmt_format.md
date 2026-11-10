---
title: "msgfmt_format()"
description: "PHP ext/intl `msgfmt_format()` backed by the managed ICU bridge."
---

## msgfmt_format()

```php
function msgfmt_format(object $formatter, array $values): string|false
```

PHP ext/intl `msgfmt_format()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$values` (`array`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `msgfmt_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/msgfmt_format.md).
