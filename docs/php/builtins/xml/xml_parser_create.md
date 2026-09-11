---
title: "xml_parser_create()"
description: "Creates a new XML parser object."
sidebar:
  order: 919
---

## xml_parser_create()

```php
function xml_parser_create(?string $encoding = null): mixed
```

Creates a new XML parser object.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_create.md).
