---
title: "mysqli_dump_debug_info()"
description: "Asks the server to write debugging information to its log."
sidebar:
  order: 114
---

## mysqli_dump_debug_info()

```php
function mysqli_dump_debug_info(mixed $mysql): bool
```

Asks the server to write debugging information to its log.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_dump_debug_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_dump_debug_info.md).
