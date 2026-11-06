---
title: "html_entity_decode()"
description: "Converts HTML entities in a string back into their corresponding characters."
sidebar:
  order: 932
---

## html_entity_decode()

```php
function html_entity_decode(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

Converts HTML entities in a string back into their corresponding characters.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`string`), default `'UTF-8'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/html_entity_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/html_entity_decode.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `html_entity_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/html_entity_decode.md).
