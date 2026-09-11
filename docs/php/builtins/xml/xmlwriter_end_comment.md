---
title: "xmlwriter_end_comment()"
description: "Ends the current comment."
sidebar:
  order: 996
---

## xmlwriter_end_comment()

```php
function xmlwriter_end_comment(mixed $writer): bool
```

Ends the current comment.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_comment.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_comment.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_comment` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_comment.md).
