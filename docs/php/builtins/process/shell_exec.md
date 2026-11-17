---
title: "shell_exec()"
description: "Executes a command via the shell and returns the complete output as a string."
---

## shell_exec()

```php
function shell_exec(string $command): string
```

Executes a command via the shell and returns the complete output as a string.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `shell_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/shell_exec.md).
