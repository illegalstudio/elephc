---
title: "pcntl_signal_dispatch()"
description: "Invokes callbacks for every signal currently pending in PCNTL's queue."
sidebar:
  order: 343
---

## pcntl_signal_dispatch()

```php
function pcntl_signal_dispatch(): bool
```

Invokes callbacks for every signal currently pending in PCNTL's queue.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_dispatch.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_dispatch.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_signal_dispatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_signal_dispatch.md).
