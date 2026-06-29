---
title: "shell_exec()"
description: "Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper."
sidebar:
  order: 292
---

## shell_exec()

```php
function shell_exec(string $command): string
```

Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `shell_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/shell_exec.md).

