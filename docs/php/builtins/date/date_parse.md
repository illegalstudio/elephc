---
title: "date_parse()"
description: "Parses a date/time string into its components, warnings, and errors."
sidebar:
  order: 208
---

## date_parse()

```php
function date_parse(string $datetime): array
```

Parses a date/time string into its components, warnings, and errors.

**Parameters**:
- `$datetime` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_parse.md).
