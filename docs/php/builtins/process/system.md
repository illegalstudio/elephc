---
title: "system()"
description: "Lowers `system(command)` through libc `system()` and returns the legacy empty string result."
sidebar:
  order: 292
---

## system()

```php
function system(string $command, int $result_code): string
```

Lowers `system(command)` through libc `system()` and returns the legacy empty string result.

**Parameters**:
- `$command` (`string`)
- `$result_code` (`int`), passed by reference

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `system` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/system.md).

