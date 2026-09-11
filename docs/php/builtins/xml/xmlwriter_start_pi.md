---
title: "xmlwriter_start_pi()"
description: "Starts a processing instruction."
sidebar:
  order: 962
---

## xmlwriter_start_pi()

```php
function xmlwriter_start_pi(mixed $writer, string $target): bool
```

Starts a processing instruction.

**Parameters**:
- `$writer` (`mixed`)
- `$target` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_pi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_pi.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_pi` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_pi.md).
