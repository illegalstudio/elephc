---
title: "timezone_transitions_get()"
description: "Implemented by the compiler-injected tz prelude."
sidebar:
  order: 250
---

## timezone_transitions_get()

```php
function timezone_transitions_get(mixed $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): mixed
```

Implemented by the compiler-injected tz prelude.

**Parameters**:
- `$object` (`mixed`)
- `$timestampBegin` (`int`), default `PHP_INT_MIN`, optional
- `$timestampEnd` (`int`), default `PHP_INT_MAX`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tz prelude.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_transitions_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_transitions_get.md).
