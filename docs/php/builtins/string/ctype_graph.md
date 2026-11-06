---
title: "ctype_graph()"
description: "Checks if all characters in the string are visible."
sidebar:
  order: 908
---

## ctype_graph()

```php
function ctype_graph(string $text): bool
```

Checks if all characters in the string are visible.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_graph.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_graph.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_graph` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_graph.md).
