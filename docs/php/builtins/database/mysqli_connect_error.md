---
title: "mysqli_connect_error()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 106
---

## mysqli_connect_error()

```php
function mysqli_connect_error(): string
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_connect_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_connect_error.md).
