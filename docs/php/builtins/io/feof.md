---
title: "feof()"
description: "Lowers `feof(stream)` through the runtime EOF-flag table helper."
sidebar:
  order: 139
---

## feof()

```php
function feof(resource $stream): bool
```

Lowers `feof(stream)` through the runtime EOF-flag table helper.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `feof` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/feof.md).

