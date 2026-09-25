---
title: "session_unset()"
description: "Removes every variable from the session without destroying it."
sidebar:
  order: 952
---

## session_unset()

```php
function session_unset(): bool
```

Removes every variable from the session without destroying it.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_unset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_unset.md).
