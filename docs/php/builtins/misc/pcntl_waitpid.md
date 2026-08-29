---
title: "pcntl_waitpid()"
description: "Waits for a selected child process and writes its target-native status."
sidebar:
  order: 351
---

## pcntl_waitpid()

```php
function pcntl_waitpid(int $process_id, mixed $status, int $flags = 0, mixed $resource_usage = []): int
```

Waits for a selected child process and writes its target-native status.

**Parameters**:
- `$process_id` (`int`)
- `$status` (`mixed`), passed by reference
- `$flags` (`int`), default `0`, optional
- `$resource_usage` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitpid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitpid.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_waitpid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_waitpid.md).
