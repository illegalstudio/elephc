---
title: "pcntl_errno()"
description: "Returns the errno recorded by the most recent failing PCNTL operation."
sidebar:
  order: 329
---

## pcntl_errno()

```php
function pcntl_errno(): int
```

Returns the errno recorded by the most recent failing PCNTL operation.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_errno.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_errno.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_errno.md).
