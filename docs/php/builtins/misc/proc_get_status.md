---
title: "proc_get_status()"
description: "Returns information about a process opened by proc_open()."
sidebar:
  order: 781
---

## proc_get_status()

```php
function proc_get_status(mixed $process): array
```

Returns information about a process opened by proc_open().

**Parameters**:
- `$process` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected proc prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_get_status.md).
