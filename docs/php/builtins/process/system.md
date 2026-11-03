---
title: "system()"
description: "Executes an external program and displays the output."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `system` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/system.md).
