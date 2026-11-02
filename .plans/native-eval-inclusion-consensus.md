# Inclusion specification consensus

Reviewed specification: .plans/native-eval-inclusion-spec.md, revision 3.
SHA-256: 459e29a0c61812134bd5fd53565f56f6dccce0290509874f1af122134353883a

- GLM 5.2 cloud: LOCK, target/native-eval-inclusion-v3-glm-review.md.
- Kimi K2.7-code cloud: LOCK, target/native-eval-inclusion-v3-kimi27-review.md.
- Kimi K3 cloud: LOCK, target/native-eval-inclusion-v3-kimi3-review.md.
- Root: LOCK on this contract, based on the supplied code and measured PHP oracle.

This is pre-implementation consensus on the exact specification, not an audit of
completed code, proof of full PHP parity or Symfony HTTP acceptance. Reviewers
received bounded source/evidence packets, not autonomous filesystem access.
The frozen spec header remains unchanged to preserve its reviewed hash; consensus
status is recorded here rather than modifying the reviewed document.

Kimi K3's discussion of forward-parent early binding is a reviewer assertion,
not locally executed evidence. Verify such eligibility cases against PHP/php-src
before implementing that rule. Final implementation reviews and Kimi K3 audit
remain required after functional completion.
