---
title: "pclose()"
description: "Closes process file pointer."
sidebar:
  order: 365
---

## pclose()

```php
function pclose(resource $handle): int
```

Closes process file pointer.

**Parameters**:
- `$handle` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/pclose.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pclose.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pclose` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/pclose.md).
