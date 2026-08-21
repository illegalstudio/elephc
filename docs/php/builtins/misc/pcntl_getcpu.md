---
title: "pcntl_getcpu()"
description: "Returns the logical CPU on which the calling thread is executing."
sidebar:
  order: 333
---

## pcntl_getcpu()

```php
function pcntl_getcpu(): int
```

Returns the logical CPU on which the calling thread is executing.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpu.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpu.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getcpu` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getcpu.md).
