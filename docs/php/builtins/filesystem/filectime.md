---
title: "filectime()"
description: "Lowers `filectime(path)` and boxes the runtime integer-or-false result."
sidebar:
  order: 93
---

## filectime()

```php
function filectime(string $filename): mixed
```

Lowers `filectime(path)` and boxes the runtime integer-or-false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filectime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filectime.md).

