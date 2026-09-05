---
title: "date_parse_from_format()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 206
---

## date_parse_from_format()

```php
function date_parse_from_format(string $format, string $datetime): array
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$format` (`string`)
- `$datetime` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_parse_from_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_parse_from_format.md).
