---
title: "mysqli_init()"
description: "Creates an unconnected mysqli object for mysqli_real_connect()."
sidebar:
  order: 139
---

## mysqli_init()

```php
function mysqli_init(): mixed
```

Creates an unconnected mysqli object for mysqli_real_connect().

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_init.md).
