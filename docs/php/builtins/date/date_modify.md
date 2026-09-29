---
title: "date_modify()"
description: "Applies a relative modifier such as \"+1 day\" to a DateTime."
sidebar:
  order: 206
---

## date_modify()

```php
function date_modify(mixed $object, string $modifier): mixed
```

Applies a relative modifier such as "+1 day" to a DateTime.

**Parameters**:
- `$object` (`mixed`)
- `$modifier` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_modify` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_modify.md).
