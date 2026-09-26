---
title: "pdo_drivers()"
description: "Returns the names of the PDO drivers this build provides."
sidebar:
  order: 185
---

## pdo_drivers()

```php
function pdo_drivers(): array
```

Returns the names of the PDO drivers this build provides.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pdo_drivers` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/pdo_drivers.md).
