---
title: "mysqli_savepoint()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 153
---

## mysqli_savepoint()

```php
function mysqli_savepoint(mixed $mysql, string $name): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_savepoint` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_savepoint.md).
