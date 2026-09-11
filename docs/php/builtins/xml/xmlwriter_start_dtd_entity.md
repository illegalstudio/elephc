---
title: "xmlwriter_start_dtd_entity()"
description: "Starts a DTD entity declaration."
sidebar:
  order: 959
---

## xmlwriter_start_dtd_entity()

```php
function xmlwriter_start_dtd_entity(mixed $writer, string $name, bool $isParam): bool
```

Starts a DTD entity declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$isParam` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_entity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_entity.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_dtd_entity` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_dtd_entity.md).
