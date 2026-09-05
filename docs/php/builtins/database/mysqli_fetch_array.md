---
title: "mysqli_fetch_array()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 115
---

## mysqli_fetch_array()

```php
function mysqli_fetch_array(mixed $result, int $mode = 3): ?array
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)
- `$mode` (`int`), default `3`, optional

**Returns**: `?array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_array.md).
