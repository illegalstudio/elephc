---
title: "pcntl_setpriority()"
description: "Changes a process, process-group, or user scheduling priority."
sidebar:
  order: 339
---

## pcntl_setpriority()

```php
function pcntl_setpriority(int $priority, int $process_id = null, int $mode = 0): bool
```

Changes a process, process-group, or user scheduling priority.

**Parameters**:
- `$priority` (`int`)
- `$process_id` (`int`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setpriority.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setpriority.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setpriority` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setpriority.md).
