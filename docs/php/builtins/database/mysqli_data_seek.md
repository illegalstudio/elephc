---
title: "mysqli_data_seek()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 107
---

## mysqli_data_seek()

```php
function mysqli_data_seek(mixed $result, int $offset): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)
- `$offset` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_data_seek` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_data_seek.md).
