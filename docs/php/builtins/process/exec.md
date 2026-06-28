---
title: "exec()"
description: "Lowers `exec(command)` by capturing shell stdout through the shared runtime helper."
sidebar:
  order: 284
---

## exec()

```php
function exec(string $command, array $output, int $result_code): string
```

Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.

**Parameters**:
- `$command` (`string`)
- `$output` (`array`), passed by reference
- `$result_code` (`int`), passed by reference

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/exec.md).

