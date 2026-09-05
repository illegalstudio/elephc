---
title: "mysqli_init()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 136
---

## mysqli_init()

```php
function mysqli_init(): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_init.md).
