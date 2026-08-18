---
title: "pcntl_sigwaitinfo()"
description: "Waits synchronously for one selected Linux signal and returns its number or false."
sidebar:
  order: 346
---

## pcntl_sigwaitinfo()

```php
function pcntl_sigwaitinfo(mixed $signals, mixed $info = []): mixed
```

Waits synchronously for one selected Linux signal and returns its number or false.

**Parameters**:
- `$signals` (`mixed`)
- `$info` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigwaitinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_sigwaitinfo.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_sigwaitinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_sigwaitinfo.md).
