---
title: "error_log()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 846
---

## error_log()

```php
function error_log(string $message, int $message_type = 0, string $destination = null, string $additional_headers = null): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$message` (`string`)
- `$message_type` (`int`), default `0`, optional
- `$destination` (`string`), default `null`, optional
- `$additional_headers` (`string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_log` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/error_log.md).
