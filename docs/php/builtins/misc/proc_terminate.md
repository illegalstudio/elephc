---
title: "proc_terminate()"
description: "Kills a process opened by proc_open()."
sidebar:
  order: 788
---

## proc_terminate()

```php
function proc_terminate(mixed $process, int $signal = 15): bool
```

Kills a process opened by proc_open().

**Parameters**:
- `$process` (`mixed`)
- `$signal` (`int`), default `15`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected proc prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_terminate` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_terminate.md).
