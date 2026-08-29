---
title: "pcntl_signal()"
description: "Installs a callable, default, or ignored disposition for one signal."
sidebar:
  order: 341
---

## pcntl_signal()

```php
function pcntl_signal(int $signal, mixed $handler, bool $restart_syscalls = true): bool
```

Installs a callable, default, or ignored disposition for one signal.

**Parameters**:
- `$signal` (`int`)
- `$handler` (`mixed`)
- `$restart_syscalls` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_signal` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_signal.md).
