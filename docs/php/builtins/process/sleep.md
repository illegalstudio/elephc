---
title: "sleep()"
description: "Delays execution for a number of seconds."
sidebar:
  order: 316
---

## sleep()

```php
function sleep(int $seconds): int
```

Delays execution for a number of seconds.

**Parameters**:
- `$seconds` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/sleep.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/sleep.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sleep` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/sleep.md).

