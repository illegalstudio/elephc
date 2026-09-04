---
title: "session_module_name()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 861
---

## session_module_name()

```php
function session_module_name(string $module = null): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$module` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_module_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_module_name.md).
