# GLM 5.2 raw response — specification round 1

```text
Voici l'analyse de la spécification de compatibilité PHP DOM 8.5 pour Elephc.

J'ai identifié deux points nécessitant des réserves. Le consensus absolu n'étant pas atteint, je ne fournis aucun verrouillage (LOCK).

### BLOCKER
**Section 4.1 - Implémentation et visibilité des propriétés virtuelles**
La spécification indique : *"Virtual DOM properties use internal property-handler metadata matching php-src. They are not user-visible PHP property hooks."*
Ce fait est démontrablement en contradiction avec le comportement de php-src depuis PHP 8.4. Dans PHP 8.4 et supérieur, les propriétés virtuelles du DOM (comme `DOMNode::$nodeName` ou `Dom\Node::$nodeName`) sont explicitement déclarées et implémentées à l'aide des *property hooks* du langage (ex: `public string $nodeName { get => ... }`). En conséquence, elles **sont** visibles par l'utilisateur via Reflection (ex: `ReflectionProperty::getHooks()`).
Ceci entre en conflit direct avec la section 1 qui exige : *"The PHP-visible surface matches PHP 8.5.8 [...] and Reflection output."* Si Elephc masque ces hooks, la sortie de Reflection différera de celle de l'oracle PHP 8.5.8, entraînant l'échec des tests de parité.
**Correction :** Supprimer l'affirmation selon laquelle les propriétés virtuelles ne sont pas des *user-visible PHP property hooks*. Spécifier plutôt que les propriétés virtuelles sont exposées publiquement via les *property hooks* de PHP pour correspondre exactement à la sortie de Reflection de php-src, tout en déléguant leur implémentation interne aux gestionnaires de propriétés internes d'Elephc.

### QUESTION
**Section 5.6 - Redondance de l'opcode dans l'ABI**
La signature de la fonction C `elephc_dom_call` inclut un paramètre `opcode` explicite : `elephc_dom_call(context, opcode, request_ptr, request_len, out_result) -> status`. Simultanément, la structure `RequestHeader` du message aplati contient également un champ `opcode: u32`.
Il n'est pas précisé si ces deux opcodes doivent correspondre obligatoirement, ni lequel fait autorité en cas d'incohérence, ni si l'un des deux est ignoré.
**Correction :** Clarifier la relation entre les deux. Soit retirer le champ `opcode` de `RequestHeader` (puisqu'il est passé en argument direct), soit indiquer explicitement que l'opcode de l'en-tête doit correspondre à l'argument de la fonction et soulever une erreur `ABI_ERROR` en cas de divergence.
```
