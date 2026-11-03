---
title: "strtok()"
description: "Tokenizes a string by a set of delimiter bytes."
sidebar:
  order: 1064
---

## strtok()

```php
function strtok(string $string, string $token = null): mixed
```

Tokenizes a string by a set of delimiter bytes.

**Parameters**:
- `$string` (`string`)
- `$token` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected strtok prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strtok.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtok.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strtok` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strtok.md).
