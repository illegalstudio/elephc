---
title: "session_name()"
description: "Reads or sets the session name, which is also the cookie name."
sidebar:
  order: 989
---

## session_name()

```php
function session_name(?string $name = null): mixed
```

Reads or sets the session name, which is also the cookie name.

**Parameters**:
- `$name` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_name.md).
