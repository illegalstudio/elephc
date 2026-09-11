---
title: "xml_parser_create_ns()"
description: "Creates a namespace-aware XML parser whose qualified names join the URI and local name with a separator."
sidebar:
  order: 920
---

## xml_parser_create_ns()

```php
function xml_parser_create_ns(?string $encoding = null, string $separator = ':'): mixed
```

Creates a namespace-aware XML parser whose qualified names join the URI and local name with a separator.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional
- `$separator` (`string`), default `':'`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create_ns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create_ns.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_create_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_create_ns.md).
