---
title: "system()"
description: "Executes an external program and displays the output."
sidebar:
  order: 370
---

## system()

```php
function system(string $command): string
```

Executes an external program and displays the output.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/system.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/system.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `system` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/system.md).
