---
title: "http_response_code()"
description: "Gets or sets the HTTP response code."
sidebar:
  order: 293
---

## http_response_code()

```php
function http_response_code(int $response_code = 0): int
```

Gets or sets the HTTP response code.

**Parameters**:
- `$response_code` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `http_response_code` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/http_response_code.md).

