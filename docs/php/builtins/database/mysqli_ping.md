---
title: "mysqli_ping()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 144
---

## mysqli_ping()

```php
function mysqli_ping(mixed $mysql): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_ping` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_ping.md).
