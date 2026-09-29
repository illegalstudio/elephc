---
title: "date_create_immutable_from_format()"
description: "Creates a DateTimeImmutable by parsing a string against an explicit format."
sidebar:
  order: 196
---

## date_create_immutable_from_format()

```php
function date_create_immutable_from_format(string $format, string $datetime, mixed $timezone = null): mixed
```

Creates a DateTimeImmutable by parsing a string against an explicit format.

**Parameters**:
- `$format` (`string`)
- `$datetime` (`string`)
- `$timezone` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_create_immutable_from_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_create_immutable_from_format.md).
