---
title: "mysqli_store_result()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 177
---

## mysqli_store_result()

```php
function mysqli_store_result(mixed $mysql, int $mode = 0): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$mode` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_store_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_store_result.md).
