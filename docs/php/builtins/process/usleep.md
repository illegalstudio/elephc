---
title: "usleep()"
description: "Delays execution for a number of microseconds."
sidebar:
  order: 331
---

## usleep()

```php
function usleep(int $microseconds): void
```

Delays execution for a number of microseconds.

**Parameters**:
- `$microseconds` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/usleep.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/usleep.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `usleep` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/usleep.md).

