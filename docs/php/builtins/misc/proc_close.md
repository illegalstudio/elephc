---
title: "proc_close()"
description: "Closes a process opened by proc_open() and returns its exit code."
sidebar:
  order: 784
---

## proc_close()

```php
function proc_close(mixed $process): int
```

Closes a process opened by proc_open() and returns its exit code.

**Parameters**:
- `$process` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected proc prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_close.md).
