---
title: "posix_setpgid()"
description: "Moves a process into a process group for job control."
sidebar:
  order: 362
---

## posix_setpgid()

```php
function posix_setpgid(int $process_id, int $process_group_id): bool
```

Moves a process into a process group for job control.

**Parameters**:
- `$process_id` (`int`)
- `$process_group_id` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setpgid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setpgid.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_setpgid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_setpgid.md).
