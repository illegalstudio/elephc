---
title: "nl2br()"
description: "Inserts HTML line breaks before newlines in a string."
sidebar:
  order: 376
---

## nl2br()

```php
function nl2br(string $string): string
```

Inserts HTML line breaks before newlines in a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/nl2br.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/nl2br.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `nl2br` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/nl2br.md).

