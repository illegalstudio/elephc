---
title: "passthru()"
description: "Lowers `passthru(command)` through libc `system()` for direct stdout passthrough."
sidebar:
  order: 286
---

## passthru()

```php
function passthru(string $command, int $result_code): void
```

Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.

**Parameters**:
- `$command` (`string`)
- `$result_code` (`int`), passed by reference

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `passthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/passthru.md).

