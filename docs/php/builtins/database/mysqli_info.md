---
title: "mysqli_info()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 135
---

## mysqli_info()

```php
function mysqli_info(mixed $mysql): ?string
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `?string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_info.md).
