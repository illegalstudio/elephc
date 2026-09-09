---
title: "xml_parser_free()"
description: "Frees an XML parser; a no-op kept for compatibility."
sidebar:
  order: 945
---

## xml_parser_free()

```php
function xml_parser_free(mixed $parser): bool
```

Frees an XML parser; a no-op kept for compatibility.

**Parameters**:
- `$parser` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_free.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_free.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_free.md).
