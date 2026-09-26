---
title: "mysqli_ping()"
description: "Checks the connection and reconnects when that is enabled."
sidebar:
  order: 147
---

## mysqli_ping()

```php
function mysqli_ping(mixed $mysql): bool
```

Checks the connection and reconnects when that is enabled.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_ping` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_ping.md).
