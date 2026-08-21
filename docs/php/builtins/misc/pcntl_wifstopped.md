---
title: "pcntl_wifstopped()"
description: "Reports whether a child wait status represents a stopped process."
sidebar:
  order: 356
---

## pcntl_wifstopped()

```php
function pcntl_wifstopped(int $status): bool
```

Reports whether a child wait status represents a stopped process.

**Parameters**:
- `$status` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifstopped.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifstopped.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wifstopped` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wifstopped.md).
