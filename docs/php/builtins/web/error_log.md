---
title: "error_log()"
description: "Sends an error message to the log, a file, or an email address."
sidebar:
  order: 927
---

## error_log()

```php
function error_log(string $message, int $message_type = 0, ?string $destination = null, ?string $additional_headers = null): bool
```

Sends an error message to the log, a file, or an email address.

**Parameters**:
- `$message` (`string`)
- `$message_type` (`int`), default `0`, optional
- `$destination` (`?string`), default `null`, optional
- `$additional_headers` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_log` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/error_log.md).
