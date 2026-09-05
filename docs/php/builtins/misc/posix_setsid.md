---
title: "posix_setsid()"
description: "Creates a new session and makes the current process its leader."
sidebar:
  order: 363
---

## posix_setsid()

```php
function posix_setsid(): int
```

Creates a new session and makes the current process its leader.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setsid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/posix_setsid.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_setsid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_setsid.md).
