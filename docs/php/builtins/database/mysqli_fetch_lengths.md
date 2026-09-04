---
title: "mysqli_fetch_lengths()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 121
---

## mysqli_fetch_lengths()

```php
function mysqli_fetch_lengths(mixed $result): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_lengths` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_lengths.md).
