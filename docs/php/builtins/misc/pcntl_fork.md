---
title: "pcntl_fork()"
description: "Forks the current process and returns the child or parent process identifier."
sidebar:
  order: 331
---

## pcntl_fork()

```php
function pcntl_fork(): int
```

Forks the current process and returns the child or parent process identifier.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_fork.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_fork.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_fork` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_fork.md).
