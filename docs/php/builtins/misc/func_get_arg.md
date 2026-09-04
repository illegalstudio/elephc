---
title: "func_get_arg()"
description: "Returns one argument from the current function call."
sidebar:
  order: 326
---

## func_get_arg()

```php
function func_get_arg(int $position): mixed
```

Returns one argument from the current function call.

**Parameters**:
- `$position` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/func_get_arg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/func_get_arg.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `func_get_arg` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/func_get_arg.md).
