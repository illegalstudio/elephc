---
title: "xmlwriter_write_comment()"
description: "Writes a complete comment."
sidebar:
  order: 1027
---

## xmlwriter_write_comment()

```php
function xmlwriter_write_comment(mixed $writer, string $content): bool
```

Writes a complete comment.

**Parameters**:
- `$writer` (`mixed`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_comment.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_comment.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_comment` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_comment.md).
