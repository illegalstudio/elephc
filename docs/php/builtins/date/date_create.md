---
title: "date_create()"
description: "Creates a DateTime from a date/time string."
---

## date_create()

```php
function date_create(string $datetime = 'now', mixed $timezone = null): mixed
```

Creates a DateTime from a date/time string.

**Parameters**:
- `$datetime` (`string`), default `'now'`, optional
- `$timezone` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_create.md).
