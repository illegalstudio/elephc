---
title: "passthru()"
description: "Executes an external program and passes its output directly."
---

## passthru()

```php
function passthru(string $command): void
```

Executes an external program and passes its output directly.

**Parameters**:
- `$command` (`string`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `passthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/passthru.md).
