# MiniMax M3 — specification round 1

Model: `minimax-m3:cloud`

Artifact SHA-256:
`63eda127e9c6959848c9b86676d6399572539b8d52b4dfd4074f679e1df313fc`

```text
L'artefact est examiné contre la cohérence interne, les sources PHP épinglées,
libxml2 2.15.3, Lexbor 2.7.0, et l'architecture Elephc décrite. Je ne peux pas
émettre de LOCK.

BLOCKER B1 : classe Dom\NamespaceInfo non démontrable.
BLOCKER B2 : décomptes de surface tributaires de B1.
BLOCKER B3 : Dom\XPath marqué final sans justification.

MAJOR M1 : épinglage de PHP 8.5.8 non démontrable.
MAJOR M2 : version Lexbor 2.7.0 non vérifiable.
MAJOR M3 : version libxml2 2.15.3 non vérifiable.
MAJOR M4 : décomptes PHPT invérifiables.
MAJOR M5 : décomptes de surface invérifiables.
MAJOR M6 : versions de modèles IA non démontrables.

MINOR m1 : rationale de LIBXML_NO_XXE imprécise.
MINOR m2 : auto-insertion non spécifiée par méthode.
MINOR m3 : accès non initialisé non spécifié par type.
MINOR m4 : LibXMLError::$file et les cinq autres propriétés devraient être
clarifiés comme readonly.

NIT n1 : casse de l'identifiant MiniMax.
NIT n2 : final redondant ou incorrect sur certaines classes.

Les questions Q1 à Q7 redemandent la preuve de NamespaceInfo, PHP 8.5.8,
Lexbor 2.7.0, libxml2 2.15.3, des comptes PHPT, des comptes Reflection, et des
endpoints Ollama.
```

Disposition:

- The `LIBXML_NO_XXE`, self-mutation, and uninitialized-state wording was made
  more explicit.
- Exact public URLs, checksums, source excerpts, commands, Reflection output,
  and endpoint names were added as reproducible evidence.
- `Dom\NamespaceInfo`, final `Dom\XPath`, PHP/libxml/Lexbor versions, counts,
  and endpoint availability were confirmed.
- The readonly claim was rejected: all six `LibXMLError` properties accept
  direct writes in the pinned oracle.
