---
title: "register_shutdown_function()"
description: "Registers a callback to run when the script terminates."
sidebar:
  order: 1121
---

## register_shutdown_function()

```php
function register_shutdown_function(callable $callback, ...$args): void
```

Registers a callback to run when the script terminates.

**Parameters**:
- `$callback` (`callable`)
- `...$args` - variadic: collects excess arguments into `$args`.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected error_handling prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `register_shutdown_function` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/register_shutdown_function.md).
