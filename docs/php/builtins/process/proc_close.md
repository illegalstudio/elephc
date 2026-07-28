---
title: "proc_close()"
description: "Close a process opened by proc_open and return the exit status."
sidebar:
  order: 330
---

## proc_close()

```php
function proc_close(resource $process): int
```

Close a process opened by proc_open and return the exit status.

**Parameters**:
- `$process` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `proc_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/proc_close.md).
