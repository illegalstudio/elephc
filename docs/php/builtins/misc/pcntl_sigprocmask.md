---
title: "pcntl_sigprocmask()"
description: "Changes the signal mask and optionally writes the prior blocked signals."
sidebar:
  order: 344
---

## pcntl_sigprocmask()

```php
function pcntl_sigprocmask(int $mode, mixed $signals, mixed $old_signals = []): bool
```

Changes the signal mask and optionally writes the prior blocked signals.

**Parameters**:
- `$mode` (`int`)
- `$signals` (`mixed`)
- `$old_signals` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigprocmask.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigprocmask.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_sigprocmask` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_sigprocmask.md).
