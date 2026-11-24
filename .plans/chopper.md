# Shared lexical scanning with Chopper

- [x] Extract the native scanner from PR #2180 into elephc-chopper and replace elephc-tokenizer-engine.
- [x] Share compiler operator, identifier, digit, and trivia recognition with Chopper.
- [x] Share Magician operator and digit recognition with Chopper.
- [x] Share quoted string boundaries, single-quoted decoding, braced interpolation, and heredoc labels in Chopper.
- [x] Evaluate folding the validation-only parser build into elephc-syntax with separate parser entry points.
- [x] Consolidate the parser builds using separate compiler and validation entry points.
- [x] Move the shared parser and compiler frontend into Chopper and keep the C bridge separate.

The initial extraction preserved the Zend token stream and the validation-only parser.
The tokenizer bridge and Magician shared the PHP tokenizer API in that parser build.
At that stage Chopper depended only on the token catalog, so the compiler and eval
frontends could adopt the scanner independently. The extension retained access to PHP
token IDs and raw source bytes.

Verified on Linux x86_64 against PHP 8.5.12. The tokenizer parity suite, standalone
lossless scanning test, compiled PHP and eval smoke tests, and builtin audits passed.

The shared lexeme migration matched the baseline compiler tokens, ASTs, and diagnostics
on 337 PHP and LFC fixtures. Focused compiler lexer and Magician tests passed alongside
the PHP 8.5.12 oracle. An opaque eval smoke case returned 42 from a coalescing expression
with a separated hex literal.

In six interleaved debug benchmark runs per input, combined scanning and parsing took
about 151 ms on the ASCII operator sample before and after the change. The interpolation
sample rose from 120 to 128 ms. On the repeated Unicode-identifier sample the combined
time fell from about 1.91 s to 147 ms for PHP and from 1.49 s to 140 ms for LFC.

The string scanner changes passed 49 focused tests, including the PHP 8.5.12 tokenizer
oracle. Compiler tokens, ASTs, and diagnostics matched the previous lexer on 342 inputs.
The separate Unicode heredoc regression failed on the previous lexer and retained the
following semicolon and statement after the change.

In six interleaved debug runs per input, median scanning and parsing took 41.70 to
46.93 ms on the plain quote sample, 145.05 to 153.65 ms on the braced interpolation
sample, and 51.49 to 47.79 ms on the heredoc sample.

The compiler and `TOKEN_PARSE` now share one build of `elephc-syntax`. The
tokenizer entry point selects a scoped validation mode for the normalized lexer
and parser, restores the compiler's anonymous-class state, and discards parsed
statements. `elephc-tokenizer` remains the C ABI bridge. It and Magician use
`elephc-syntax::php_tokenizer`; Chopper provides the byte-preserving scanner and
the lexical rules reused by the compiler and Magician.

All 33 tokenizer and mode-isolation tests passed with the PHP 8.5.12 oracle.
Seven focused compiler parser tests passed, and a native executable exercised
`TOKEN_PARSE` through both the AOT bridge and runtime eval. The compiler produced
identical tokens, ASTs, and diagnostics on 334 repository examples.

On Linux x86_64, the stripped standalone `TOKEN_PARSE` probe built with Rust
1.99.1, opt-level `z`, fat LTO, and `panic = "abort"` went from 952,408 to
1,093,000 bytes (+14.8%) after parser consolidation. Across six interleaved
debug runs per version, median compiler scan and parse time on the ASCII
operator sample went from 132.14 to 136.16 ms.

The package consolidation places byte-preserving scanning in
`elephc_chopper::scanner` and the compiler frontend and PHP grammar in
`elephc_chopper::syntax`. Consumers keep the same compiler and validation entry
points through the new paths. The workspace resolves only `elephc-chopper` and
`elephc-tokenizer` for scanning, shared parsing, and the C bridge. The move keeps
the existing scoped validation mode.

The merged workspace passed all 33 tokenizer tests against PHP 8.5.12, the
lossless raw scanner test, four focused compiler lexer/parser tests, eleven CI
artifact graph tests, and the builtin documentation audits. One native Linux
x86_64 executable linked both tokenizer consumers and returned `5:5:parse`
from direct PHP and opaque runtime eval.
