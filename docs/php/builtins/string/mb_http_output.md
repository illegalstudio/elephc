---
title: "mb_http_output()"
description: "Reads or changes the encoding selected for HTTP output conversion."
sidebar:
  order: 824
---

## mb_http_output()

```php
function mb_http_output(?string $encoding = null): string|bool
```

Reads or changes the encoding selected for HTTP output conversion.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_http_output.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_http_output.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_http_output` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_http_output.md).
