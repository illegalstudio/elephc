---
title: "session_unset()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 871
---

## session_unset()

```php
function session_unset(): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_unset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_unset.md).
