---
title: "xmlwriter_start_comment()"
description: "Starts a comment."
sidebar:
  order: 1014
---

## xmlwriter_start_comment()

```php
function xmlwriter_start_comment(mixed $writer): bool
```

Starts a comment.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_comment.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_comment.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_comment` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_comment.md).
