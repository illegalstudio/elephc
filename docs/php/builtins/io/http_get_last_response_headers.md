---
title: "http_get_last_response_headers()"
description: "Returns the last HTTP response headers, or null when no request was made."
sidebar:
  order: 215
---

## http_get_last_response_headers()

```php
function http_get_last_response_headers(): mixed
```

Returns the last HTTP response headers, or null when no request was made.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `http_get_last_response_headers` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/http_get_last_response_headers.md).
