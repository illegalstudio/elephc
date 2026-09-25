---
title: "session_regenerate_id()"
description: "Replaces the session id, optionally deleting the old session's data."
sidebar:
  order: 990
---

## session_regenerate_id()

```php
function session_regenerate_id(bool $delete_old = false): bool
```

Replaces the session id, optionally deleting the old session's data.

**Parameters**:
- `$delete_old` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_regenerate_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_regenerate_id.md).
