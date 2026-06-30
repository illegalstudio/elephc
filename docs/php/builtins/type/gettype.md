---
title: "gettype()"
description: "Lowers `gettype(value)` for statically concrete PHP types."
sidebar:
  order: 413
---

## gettype()

```php
function gettype(mixed $value): string
```

Lowers `gettype(value)` for statically concrete PHP types.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gettype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/gettype.md).

