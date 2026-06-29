---
title: "settype()"
description: "Lowers `settype($local, \"type\")` by mutating the resolved local slot and returning true."
sidebar:
  order: 412
---

## settype()

```php
function settype(mixed $var, string $type): bool
```

Lowers `settype($local, "type")` by mutating the resolved local slot and returning true.

**Parameters**:
- `$var` (`mixed`), passed by reference
- `$type` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `settype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/settype.md).

