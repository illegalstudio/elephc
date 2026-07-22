---
title: "call_user_func_array()"
description: "Calls a callback with an array of parameters."
sidebar:
  order: 50
---

## call_user_func_array()

```php
function call_user_func_array(callable $callback, array $args): mixed
```

Calls a callback with an array of parameters.

**Parameters**:
- `$callback` (`callable`)
- `$args` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/call_user_func_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/call_user_func_array.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `call_user_func_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/call_user_func_array.md).
