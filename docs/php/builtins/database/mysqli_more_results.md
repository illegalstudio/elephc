---
title: "mysqli_more_results()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 138
---

## mysqli_more_results()

```php
function mysqli_more_results(mixed $mysql): bool
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

For how `mysqli_more_results` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_more_results.md).
