---
title: "session_start()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 869
---

## session_start()

```php
function session_start(mixed $options = []): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$options` (`mixed`), default `[]`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_start` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_start.md).
