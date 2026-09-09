---
title: "xml_error_string()"
description: "Returns the message for an XML parser error code."
sidebar:
  order: 911
---

## xml_error_string()

```php
function xml_error_string(int $error_code): ?string
```

Returns the message for an XML parser error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `?string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_error_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_error_string.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_error_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_error_string.md).
