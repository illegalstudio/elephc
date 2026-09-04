---
title: "http_clear_last_response_headers()"
description: "Clears the last HTTP response headers captured by an http:// stream."
sidebar:
  order: 214
---

## http_clear_last_response_headers()

```php
function http_clear_last_response_headers(): void
```

Clears the last HTTP response headers captured by an http:// stream.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `http_clear_last_response_headers` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/http_clear_last_response_headers.md).
