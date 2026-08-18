---
title: "pcntl_signal_dispatch()"
description: "Invokes callbacks for every signal currently pending in PCNTL's queue."
sidebar:
  order: 342
---

## pcntl_signal_dispatch()

```php
function pcntl_signal_dispatch(): bool
```

Invokes callbacks for every signal currently pending in PCNTL's queue.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_dispatch.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_dispatch.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_signal_dispatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_signal_dispatch.md).
