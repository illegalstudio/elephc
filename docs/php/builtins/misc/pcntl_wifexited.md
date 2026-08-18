---
title: "pcntl_wifexited()"
description: "Reports whether a child wait status represents normal termination."
sidebar:
  order: 354
---

## pcntl_wifexited()

```php
function pcntl_wifexited(int $status): bool
```

Reports whether a child wait status represents normal termination.

**Parameters**:
- `$status` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifexited.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifexited.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wifexited` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wifexited.md).
