---
title: "func_num_args()"
description: "Returns the number of arguments passed to the current function call."
sidebar:
  order: 328
---

## func_num_args()

```php
function func_num_args(): int
```

Returns the number of arguments passed to the current function call.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/func_num_args.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/func_num_args.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `func_num_args` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/func_num_args.md).
