---
title: "popen()"
description: "Opens process file pointer."
---

## popen()

```php
function popen(string $command, string $mode): mixed
```

Opens process file pointer.

**Parameters**:
- `$command` (`string`)
- `$mode` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `popen` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/popen.md).
