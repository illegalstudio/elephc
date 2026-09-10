# PHP DOM specification review evidence

Artifact:

`docs/specs/php-dom-compliance.md`

Historical round-1/round-2 reviewers:

- `glm-5.2:cloud`
- `kimi-k2.7-code:cloud`
- `minimax-m3:cloud`

Current revision-6 reviewers:

- `glm-5.2:cloud`
- `kimi-k2.7-code:cloud`
- `kimi-k3:cloud`

Each review was sent independently through the local Ollama `/api/chat`
endpoint. The complete specification bytes were appended to this prompt
template:

```text
Tu es un reviewer indépendant et strict. Relis intégralement cette nouvelle
révision de la spécification PHP DOM pour Elephc, sa cohérence, sa faisabilité
et sa conformité au PHP 8.5.8 épinglé. L Appendix A contient des preuves
obtenues par exécution sur l oracle exact et par lecture du commit épinglé; une
limite de connaissance du modèle ne contredit pas une preuve reproductible.
Classe toute réserve BLOCKER, MAJOR, MINOR, NIT ou QUESTION avec correction
précise. Le consensus absolu exige zéro réserve et zéro question. Si et
seulement si tu n as aucune réserve, réponds uniquement
LOCK SPEC <sha256>. Sinon ne donne aucun LOCK. Réponds en français.

SPECIFICATION COMPLETE:
<complete artifact bytes>
```

## Round 1

Digest:

`63eda127e9c6959848c9b86676d6399572539b8d52b4dfd4074f679e1df313fc`

No reviewer locked this revision. The full responses are preserved in this
directory. Accepted findings produced:

- one authoritative ABI opcode instead of two;
- execution-context wrapper caching for documentless nodes;
- explicit platform-ABI versus native-engine dependency language;
- exact mutator-self and construction-state matrices;
- clarified repository formatting and DOMTokenList policies.

Findings contradicted by the pinned stub/source/oracle were answered with
reproducible evidence rather than copied into the specification. That evidence
is Appendix A of the final artifact.

## Round 2

Digest:

`2d58e6fe4787e82938d5f053c0271d534619687d8140a53aeea28c40a9712f4b`

All three complete re-reviews returned only:

```text
GLM 5.2:     LOCK SPEC 2d58e6fe4787e82938d5f053c0271d534619687d8140a53aeea28c40a9712f4b
Kimi K2.7:   LOCK SPEC 2d58e6fe4787e82938d5f053c0271d534619687d8140a53aeea28c40a9712f4b
MiniMax M3:  LOCK SPEC 2d58e6fe4787e82938d5f053c0271d534619687d8140a53aeea28c40a9712f4b
```

The specification is locked. Any byte change requires a fresh complete
three-model review.

## Round 3 (reviewer replacement; final revision 6)

MiniMax M3 was removed from the current college and replaced by Kimi K3. The
replacement college reviewed complete byte-for-byte revisions independently.
Findings on revisions 3 through 5 were either incorporated as architecture and
evidence clarifications or disproved with a PHP 8.5.8 CLI built from the peeled
commit and then made explicit in Appendix A. Every byte change invalidated all
earlier verdicts.

Final digest:

`fb1b6bac24987ba64ab7330262bc2f534d1273f0556b00daf4463071e8b02690`

All three complete revision-6 reviews returned only:

```text
GLM 5.2:    LOCK SPEC fb1b6bac24987ba64ab7330262bc2f534d1273f0556b00daf4463071e8b02690
Kimi K2.7:  LOCK SPEC fb1b6bac24987ba64ab7330262bc2f534d1273f0556b00daf4463071e8b02690
Kimi K3:    LOCK SPEC fb1b6bac24987ba64ab7330262bc2f534d1273f0556b00daf4463071e8b02690
```

Revision 6 is the current locked specification. Any byte change requires a
fresh complete review by the current three-model college.
