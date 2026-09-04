# Gate 0 implementation consensus

## Locked implementation

- Implementation commit: `c815dc3912e817dbb07d4b61103dfd3238a58136`
- Implementation tree: `f6e2fb26ee1b1645609e2ce78b3b22e84ed136d9`
- Parent commit: `239ea4aeaac4b1779ec475f455db23002c9da3dd`
- Exact parent-to-implementation diff SHA-256:
  `44d88df6dcf80736e4fc0c34c2fc3e7b3ac9a1308ed87c518a2cfae8fdca6492`
- Locked specification SHA-256:
  `d632f9efca7732ad2b51e55ba50cd8811953694d67e6f290fa88f22f5dcfa5ff`
- Review prompt SHA-256:
  `d8cc7b64e933be481e7e6c5ec9c121064644e5fc318c0b1639b82eec6987705c`
- Review date: 2026-07-29
- Result: **absolute consensus — 3/3 independent `LOCK` verdicts**
- Blocking findings: none

The reviewed implementation commit was not amended during or after the review
round. Any change to that commit, tree, diff, or the locked specification
invalidates this consensus and requires three new independent reviews.

## Reviewer matrix

| Reviewer | Ollama tag | Model ID | Verdict | Exact identifiers repeated |
|---|---|---|---|---|
| GLM 5.2 | `glm-5.2:cloud` | `ce8fd6f94793` | `LOCK` | yes |
| Kimi K2.7 | `kimi-k2.7-code:cloud` | `eda07a659237` | `LOCK` | yes |
| MiniMax M3 | `minimax-m3:cloud` | `d03a959f45c0` | `LOCK` | yes |

## Review protocol

Each reviewer ran in a dedicated ephemeral Codex instance using Ollama, high
reasoning effort, no approval prompts, and a read-only sandbox. Each received
the same prompt and reviewed the complete parent-to-implementation diff,
locked specification, source, generated artifacts, and focused test evidence.
The reviewers did not receive or consult another implementation reviewer's
verdict.

The first MiniMax process completed its inspection but stalled without writing
a verdict. That execution was invalidated and interrupted. A fresh independent
MiniMax instance reviewed the unchanged commit from the same prompt and emitted
the recorded `LOCK`.

## Scope boundary

This consensus accepts **Gate 0 only**: the frozen php-src manifest,
multi-target provenance, reachability classification, drift ledger,
differential oracle harness, and their focused validation evidence. It does not
claim that Elephc is already fully stream compliant, close any later gate, or
approve publication of the complete campaign. The 228 known incompatibilities
remain explicit and are assigned to later gates.
