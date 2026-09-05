---
title: "session_set_save_handler()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 868
---

## session_set_save_handler()

```php
function session_set_save_handler(mixed $handler_or_open = null, mixed $register_or_close = true, mixed $read = null, mixed $write = null, mixed $destroy = null, mixed $gc = null, mixed $create_sid = null, mixed $validate_id = null, mixed $update_timestamp = null): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$handler_or_open` (`mixed`), default `null`, optional
- `$register_or_close` (`mixed`), default `true`, optional
- `$read` (`mixed`), default `null`, optional
- `$write` (`mixed`), default `null`, optional
- `$destroy` (`mixed`), default `null`, optional
- `$gc` (`mixed`), default `null`, optional
- `$create_sid` (`mixed`), default `null`, optional
- `$validate_id` (`mixed`), default `null`, optional
- `$update_timestamp` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_set_save_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_set_save_handler.md).
