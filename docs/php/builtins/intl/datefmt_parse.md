---
title: "datefmt_parse()"
description: "PHP ext/intl `datefmt_parse()` backed by the managed ICU bridge."
---

## datefmt_parse()

```php
function datefmt_parse(object $formatter, string $string, int $offset = null): int|float|false
```

PHP ext/intl `datefmt_parse()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$string` (`string`)
- `$offset` (`int`), passed by reference, default `null`, optional

**Returns**: `int|float|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_parse.md).
