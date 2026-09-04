# Unanimous specification lock

## Locked artifact

- Spec: `.plans/streams-php-src-8.5-compliance.md`
- SHA-256: `d632f9efca7732ad2b51e55ba50cd8811953694d67e6f290fa88f22f5dcfa5ff`
- Frozen on: 2026-07-29
- Result: **absolute consensus — 3/3 independent `LOCK` verdicts**
- Blocking findings: none

Any edit to the specification invalidates this lock. A changed spec must receive
three new independent reviews against its new SHA-256.

## Reviewer matrix

| Reviewer | Ollama tag | Model ID | Verdict | Reviewed SHA matches |
|---|---|---|---|---|
| GLM 5.2 | `glm-5.2:cloud` | `ce8fd6f94793` | `LOCK` | yes |
| Kimi K2.7 | `kimi-k2.7-code:cloud` | `eda07a659237` | `LOCK` | yes |
| MiniMax M3 | `minimax-m3:cloud` | `d03a959f45c0` | `LOCK` | yes |

## Round history

1. Round 1 was invalidated after review exposed insufficiently explicit PHPT
   directory and SPL-consumer coverage.
2. Round 2 was invalidated after further evidence, manifest-path, model-identity,
   and audit-closure clarifications were incorporated.
3. Round 3 reviewed the exact bytes identified above. All three reviewers
   returned `LOCK` with no blockers.

Earlier-round verdicts do not apply to the locked artifact.

## Scope boundary

This consensus approves the specification for implementation planning. It is
not evidence that Elephc is already PHP-stream compliant, and it is not an
implementation or publication approval. No compiler, runtime, framework,
fixture, generated documentation, or test source was changed during this audit.

Implementation must proceed gate by gate under the spec's acceptance rules.
Every completed gate still requires its own source, oracle, focused-test,
supported-target, and ownership evidence.
