---
title: "error_clear_last()"
description: "Implemented by the compiler-injected error-handling prelude."
sidebar:
  order: 1112
---

## error_clear_last()

```php
function error_clear_last(): void
```

Implemented by the compiler-injected error-handling prelude.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected error_handling prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_clear_last` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/error_clear_last.md).
