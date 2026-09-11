---
title: "xmlwriter_open_uri()"
description: "Creates a writer that outputs to a URI or file path; throws ValueError when it cannot be opened."
sidebar:
  order: 947
---

## xmlwriter_open_uri()

```php
function xmlwriter_open_uri(string $uri): mixed
```

Creates a writer that outputs to a URI or file path; throws ValueError when it cannot be opened.

**Parameters**:
- `$uri` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_uri.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_uri.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_open_uri` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_open_uri.md).
