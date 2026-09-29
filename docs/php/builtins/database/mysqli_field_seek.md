---
title: "mysqli_field_seek()"
description: "Moves a result's column cursor to an offset."
sidebar:
  order: 128
---

## mysqli_field_seek()

```php
function mysqli_field_seek(mixed $result, int $index): bool
```

Moves a result's column cursor to an offset.

**Parameters**:
- `$result` (`mixed`)
- `$index` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_field_seek` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_field_seek.md).
