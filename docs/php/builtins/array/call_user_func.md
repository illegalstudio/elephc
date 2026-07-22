---
title: "call_user_func()"
description: "Calls a callback with the given arguments."
sidebar:
  order: 49
---

## call_user_func()

```php
function call_user_func(callable $callback, ...$args): mixed
```

Calls a callback with the given arguments.

**Parameters**:
- `$callback` (`callable`)
- `...$args` — variadic: collects excess arguments into `$args`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/call_user_func.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/call_user_func.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `call_user_func` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/call_user_func.md).
