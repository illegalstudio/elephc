---
title: "register_tick_function()"
description: "Registers a function to run on each tick."
sidebar:
  order: 785
---

## register_tick_function()

```php
function register_tick_function(mixed $callback, ...$args): bool
```

Registers a function to run on each tick.

**Parameters**:
- `$callback` (`mixed`)
- `...$args` - variadic: collects excess arguments into `$args`.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: not available - compiled programs cannot call this builtin (`aot-implementation-pending`).
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `register_tick_function` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/register_tick_function.md).
