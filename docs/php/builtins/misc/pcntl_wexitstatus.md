---
title: "pcntl_wexitstatus()"
description: "Returns the exit code encoded in a child wait status."
sidebar:
  order: 352
---

## pcntl_wexitstatus()

```php
function pcntl_wexitstatus(int $status): mixed
```

Returns the exit code encoded in a child wait status.

**Parameters**:
- `$status` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wexitstatus.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wexitstatus.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wexitstatus` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wexitstatus.md).
