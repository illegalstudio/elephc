---
title: "pcntl_get_last_error()"
description: "Returns the errno recorded by the most recent failing PCNTL operation."
sidebar:
  order: 332
---

## pcntl_get_last_error()

```php
function pcntl_get_last_error(): int
```

Returns the errno recorded by the most recent failing PCNTL operation.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_get_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_get_last_error.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_get_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_get_last_error.md).
