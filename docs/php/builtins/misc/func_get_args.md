---
title: "func_get_args()"
description: "Returns the arguments passed to the current function call."
sidebar:
  order: 327
---

## func_get_args()

```php
function func_get_args(): mixed
```

Returns the arguments passed to the current function call.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/func_get_args.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/func_get_args.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `func_get_args` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/func_get_args.md).
