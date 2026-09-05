---
title: "mysqli_error_list()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 110
---

## mysqli_error_list()

```php
function mysqli_error_list(mixed $mysql): array
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_error_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_error_list.md).
