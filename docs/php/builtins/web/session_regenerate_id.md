---
title: "session_regenerate_id()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 863
---

## session_regenerate_id()

```php
function session_regenerate_id(bool $delete_old = false): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$delete_old` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_regenerate_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_regenerate_id.md).
