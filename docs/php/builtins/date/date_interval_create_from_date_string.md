---
title: "date_interval_create_from_date_string()"
description: "Creates a DateInterval from a relative date string such as \"2 days\"."
sidebar:
  order: 203
---

## date_interval_create_from_date_string()

```php
function date_interval_create_from_date_string(string $datetime): mixed
```

Creates a DateInterval from a relative date string such as "2 days".

**Parameters**:
- `$datetime` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_interval_create_from_date_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_interval_create_from_date_string.md).
