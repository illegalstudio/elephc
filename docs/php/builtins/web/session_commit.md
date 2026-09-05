---
title: "session_commit()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 853
---

## session_commit()

```php
function session_commit(): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_commit` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_commit.md).
