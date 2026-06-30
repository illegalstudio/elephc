---
title: "popen()"
description: "Lowers `popen(command, mode)` and boxes the process pipe as `resource|false`."
sidebar:
  order: 305
---

## popen()

```php
function popen(string $command, string $mode): mixed
```

Lowers `popen(command, mode)` and boxes the process pipe as `resource|false`.

**Parameters**:
- `$command` (`string`)
- `$mode` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `popen` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/popen.md).

