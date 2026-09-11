---
title: "xml_get_error_code()"
description: "Returns the parser's last error code."
sidebar:
  order: 916
---

## xml_get_error_code()

```php
function xml_get_error_code(mixed $parser): int
```

Returns the parser's last error code.

**Parameters**:
- `$parser` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_error_code.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_error_code.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_get_error_code` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_get_error_code.md).
