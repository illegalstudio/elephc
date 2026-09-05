---
title: "pcntl_wstopsig()"
description: "Returns the stopping signal encoded in a child wait status."
sidebar:
  order: 358
---

## pcntl_wstopsig()

```php
function pcntl_wstopsig(int $status): mixed
```

Returns the stopping signal encoded in a child wait status.

**Parameters**:
- `$status` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wstopsig.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wstopsig.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wstopsig` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wstopsig.md).
