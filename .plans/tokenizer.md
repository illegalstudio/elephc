# Complete PHP tokenizer support

- [x] Implement a shared, byte-preserving PHP 8.5 tokenization engine and TOKEN_PARSE validation.
- [x] Expose token_get_all(), token_name(), PhpToken and all 154 baseline constants through AOT and eval.
- [x] Add focused regression and PHP differential coverage, plus a practical example.
- [x] Update generated builtin documentation and compatibility inventory.
- [x] Open a draft PR and configure CI validation. Do not run local builds or tests.

The reference surface is the PHP 8.5.10 snapshot in scripts/docs/php_baseline.json,
confirmed against the installed PHP 8.5.12 tokenizer extension. The class includes
six methods and four public properties. Tokenization must preserve bytes, comments,
whitespace, line numbers and byte positions independently of elephc's source grammar.
Both backends must consume the same engine on every supported target. A Rust bridge
may use a maintained PHP lexer/parser with an explicit compatibility adapter.

Validation is CI-only at the user's request. No local build or test command is authorized.

Draft PR: https://github.com/illegalstudio/elephc/pull/2062

CI covers the PHP 8.5 differential corpus, native and eval APIs, ownership, generated
builtin docs, all executable targets and iOS device/Simulator links. The PR's checks
are the live validation record. CI follow-up is tracked in Code Journal plan
4941f907-e8b7-4a3b-a3c2-48626f8c5041 until the final implementation passes.

The shared engine is an rlib without C exports. Magician consumes it directly, while
the standalone tokenizer bridge owns the C ABI, avoiding duplicate exported symbols.
Factory arguments use explicit scalar storage so their string owners are tracked.
Eval-declared subclasses are instantiated through Magician's dynamic class registry.
