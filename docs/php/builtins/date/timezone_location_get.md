---
title: "timezone_location_get()"
description: "Implemented by the compiler-injected tz prelude."
sidebar:
  order: 245
---

## timezone_location_get()

```php
function timezone_location_get(mixed $object): mixed
```

Implemented by the compiler-injected tz prelude.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tz prelude.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_location_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_location_get.md).
