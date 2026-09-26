---
title: "mysqli_more_results()"
description: "Reports whether a multi-query has more results waiting."
sidebar:
  order: 141
---

## mysqli_more_results()

```php
function mysqli_more_results(mixed $mysql): bool
```

Reports whether a multi-query has more results waiting.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_more_results` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_more_results.md).
