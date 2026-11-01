---
title: "unregister_tick_function()"
description: "Removes a function previously registered to run on each tick."
sidebar:
  order: 794
---

## unregister_tick_function()

```php
function unregister_tick_function(mixed $callback): void
```

Removes a function previously registered to run on each tick.

**Parameters**:
- `$callback` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: not available - compiled programs cannot call this builtin (`aot-implementation-pending`).
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unregister_tick_function` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unregister_tick_function.md).
