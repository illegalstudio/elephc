---
title: "session_commit()"
description: "Alias of session_write_close()."
sidebar:
  order: 980
---

## session_commit()

```php
function session_commit(): bool
```

Alias of session_write_close().

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_commit` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_commit.md).
