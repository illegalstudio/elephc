---
title: "date_sun_info()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 208
---

## date_sun_info()

```php
function date_sun_info(int $timestamp, float $latitude, float $longitude): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$timestamp` (`int`)
- `$latitude` (`float`)
- `$longitude` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sun_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sun_info.md).
