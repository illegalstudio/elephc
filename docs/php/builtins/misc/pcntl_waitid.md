---
title: "pcntl_waitid()"
description: "Waits for a child state change and writes signal information plus optional PHP 8.5 resource usage."
sidebar:
  order: 351
---

## pcntl_waitid()

```php
function pcntl_waitid(int $idtype = 0, int $id = null, mixed $info = [], int $flags = 4, mixed $resource_usage = []): bool
```

Waits for a child state change and writes signal information plus optional PHP 8.5 resource usage.

**Parameters**:
- `$idtype` (`int`), default `0`, optional
- `$id` (`int`), default `null`, optional
- `$info` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `4`, optional
- `$resource_usage` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_waitid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_waitid.md).
