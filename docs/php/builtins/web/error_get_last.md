---
title: "error_get_last()"
description: "Implemented by the compiler-injected error-handling prelude."
sidebar:
  order: 1117
---

## error_get_last()

```php
function error_get_last(): ?array
```

Implemented by the compiler-injected error-handling prelude.

**Parameters**: none.

**Returns**: `?array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected error_handling prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_get_last` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/error_get_last.md).
