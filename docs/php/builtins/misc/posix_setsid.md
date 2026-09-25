---
title: "posix_setsid()"
description: "Creates a new session and makes the current process its leader."
sidebar:
  order: 688
---

## posix_setsid()

```php
function posix_setsid(): int
```

Creates a new session and makes the current process its leader.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setsid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setsid.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_setsid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_setsid.md).
