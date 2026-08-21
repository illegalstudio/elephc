# GLM 5.2 — specification round 1

Model: `glm-5.2:cloud`

Artifact SHA-256:
`63eda127e9c6959848c9b86676d6399572539b8d52b4dfd4074f679e1df313fc`

```text
Voici l'analyse de la spécification de compatibilité PHP DOM 8.5 pour Elephc.

J'ai identifié deux points nécessitant des réserves. Le consensus absolu n'étant
pas atteint, je ne fournis aucun verrouillage (LOCK).

### BLOCKER
Section 4.1 - Implémentation et visibilité des propriétés virtuelles

La spécification indique : "Virtual DOM properties use internal
property-handler metadata matching php-src. They are not user-visible PHP
property hooks."

Ce fait est démontrablement en contradiction avec le comportement de php-src
depuis PHP 8.4. Dans PHP 8.4 et supérieur, les propriétés virtuelles du DOM
(comme DOMNode::$nodeName ou Dom\Node::$nodeName) sont explicitement déclarées
et implémentées à l'aide des property hooks du langage. En conséquence, elles
sont visibles par l'utilisateur via Reflection.

Correction : supprimer l'affirmation selon laquelle les propriétés virtuelles
ne sont pas des user-visible PHP property hooks.

### QUESTION
Section 5.6 - Redondance de l'opcode dans l'ABI

La signature de la fonction C elephc_dom_call inclut un paramètre opcode
explicite. Simultanément, la structure RequestHeader contient également un
champ opcode. Il n'est pas précisé si ces deux opcodes doivent correspondre.

Correction : retirer l'un des deux opcodes ou définir la validation de leur
égalité.
```

Disposition:

- The opcode duplication was accepted and removed.
- The property-hook claim was rejected from the exact oracle:
  `isVirtual()=true`, `getHooks()=[]` for both properties. The evidence was
  added to the locked artifact.
