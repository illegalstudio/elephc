---
title: "pcntl_wifsignaled()"
description: "Reports whether a child wait status represents signal termination."
sidebar:
  order: 355
---

## pcntl_wifsignaled()

```php
function pcntl_wifsignaled(int $status): bool
```

Reports whether a child wait status represents signal termination.

**Parameters**:
- `$status` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifsignaled.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifsignaled.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wifsignaled` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wifsignaled.md).
