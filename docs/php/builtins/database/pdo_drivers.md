---
title: "pdo_drivers()"
description: "Implemented by the compiler-injected PDO prelude."
sidebar:
  order: 182
---

## pdo_drivers()

```php
function pdo_drivers(): mixed
```

Implemented by the compiler-injected PDO prelude.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pdo_drivers` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/pdo_drivers.md).
