---
title: "posix_kill()"
description: "Sends a signal to a process; signal 0 only checks that the process exists."
sidebar:
  order: 775
---

## posix_kill()

```php
function posix_kill(int $process_id, int $signal): bool
```

Sends a signal to a process; signal 0 only checks that the process exists.

**Parameters**:
- `$process_id` (`int`)
- `$signal` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/posix_kill.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/posix_kill.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_kill` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_kill.md).
