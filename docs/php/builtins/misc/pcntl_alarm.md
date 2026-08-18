---
title: "pcntl_alarm()"
description: "Schedules a SIGALRM and returns the prior alarm's remaining seconds."
sidebar:
  order: 327
---

## pcntl_alarm()

```php
function pcntl_alarm(int $seconds): int
```

Schedules a SIGALRM and returns the prior alarm's remaining seconds.

**Parameters**:
- `$seconds` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_alarm.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_alarm.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_alarm` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_alarm.md).
