# Kimi K2.7 — specification round 1

Model: `kimi-k2.7-code:cloud`

Artifact SHA-256:
`63eda127e9c6959848c9b86676d6399572539b8d52b4dfd4074f679e1df313fc`

```text
Je ne peux pas émettre de verrou. L'artifact contient au moins une réserve
démontrable et plusieurs points à éclaircir avant qu'un consensus absolu soit
possible.

BLOCKER — B1 : paramètre ?string $class_name = null manquant sur les deux
fonctions d'import SimpleXML.

Le reviewer affirme que dom_import_simplexml et Dom\import_simplexml possèdent
un second paramètre optionnel servant à la sous-classe enregistrée par
registerNodeClass.

QUESTION — Q1 : Dom\DOMException est-il vraiment un alias insensible à la casse
de DOMException ?

QUESTION — Q2 : classe et message exacts pour l'import SimpleXML croisé legacy
versus moderne. Le reviewer demande si ValueError remplace TypeError.

QUESTION — Q3 : modificateurs abstract/final des classes modernes Dom\....

QUESTION — Q4 : propriétaire du cache de wrappers pour les nœuds sans document,
par exemple un doctype créé par DOMImplementation::createDocumentType().

NIT — N1 : "system-library fallbacks" versus liens système obligatoires.

NIT — N2 : compléter la politique cargo fmt par la méthode de contrôle de style.

Conclusion : le blocker et les quatre questions empêchent tout consensus
absolu.
```

Disposition:

- Q4 and both NITs were accepted and clarified.
- The parameter, alias, error-class, and modifier claims were rejected using
  exact stub and oracle output, now reproduced in Appendix A of the locked
  artifact.
