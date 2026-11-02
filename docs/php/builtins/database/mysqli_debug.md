---
title: "mysqli_debug()"
description: "Performs debugging operations."
sidebar:
  order: 113
---

## mysqli_debug()

```php
function mysqli_debug(string $options): bool
```

Performs debugging operations.

**Parameters**:
- `$options` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_debug` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_debug.md).
