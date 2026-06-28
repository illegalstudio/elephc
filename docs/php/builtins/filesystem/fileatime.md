---
title: "fileatime()"
description: "Lowers `fileatime(path)` and boxes the runtime integer-or-false result."
sidebar:
  order: 92
---

## fileatime()

```php
function fileatime(string $filename): mixed
```

Lowers `fileatime(path)` and boxes the runtime integer-or-false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileatime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileatime.md).

