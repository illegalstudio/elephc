---
title: "pcntl_sigtimedwait()"
description: "Waits up to a timeout for one selected Linux signal and returns its number or false."
sidebar:
  order: 346
---

## pcntl_sigtimedwait()

```php
function pcntl_sigtimedwait(mixed $signals, mixed $info = [], int $seconds = 0, int $nanoseconds = 0): mixed
```

Waits up to a timeout for one selected Linux signal and returns its number or false.

**Parameters**:
- `$signals` (`mixed`)
- `$info` (`mixed`), passed by reference, default `[]`, optional
- `$seconds` (`int`), default `0`, optional
- `$nanoseconds` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigtimedwait.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigtimedwait.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_sigtimedwait` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_sigtimedwait.md).
