---
title: "mysqli_fetch_field_direct()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 119
---

## mysqli_fetch_field_direct()

```php
function mysqli_fetch_field_direct(mixed $result, int $index): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)
- `$index` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_field_direct` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_field_direct.md).
