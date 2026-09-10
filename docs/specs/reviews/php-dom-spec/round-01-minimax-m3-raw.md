# MiniMax M3 raw response — specification round 1

```text
**Revue de la spécification de compatibilité PHP DOM 8.5**

L'artefact est examiné contre la cohérence interne, les sources PHP épinglées, libxml2 2.15.3, Lexbor 2.7.0, et l'architecture Elephc décrite. Je ne peux pas émettre de LOCK en raison de plusieurs préoccupations classées ci-dessous.

---

### BLOCKER

**B1. Classe `Dom\NamespaceInfo` non démontrable**
La §3.3 déclare `final Dom\NamespaceInfo` parmi les 25 classes modernes du namespace `Dom\`. Cette classe n'existe ni en PHP 8.4 (qui a introduit le namespace `Dom\` moderne) ni, à ma connaissance, en PHP 8.5. Le namespace `Dom\` moderne expose des informations d'espace de noms via des méthodes sur les nœuds (`getNamespaceURI`, `lookupNamespaceURI`, `lookupPrefix`, etc.), pas via une classe dédiée. La classe legacy `DOMNameSpaceNode` existe mais aucune modernisation en `Dom\NamespaceInfo` n'a été annoncée. Comme le décompte de 51 noms exportés et 50 définitions canoniques (§3.1) dépend de cette classe, son existence conditionne la cohérence de l'ensemble du manifeste de surface. **Correction**: retirer la classe et tous les décomptes associés; ou fournir la preuve documentaire (stub épinglé, énumération des membres, justification du modificateur `final`).

**B2. Décomptes de surface tributaires de B1**
Les comptes de §3.1 (51 noms exportés, 50 canoniques, 313 méthodes, 184 propriétés, 16 constantes/cas, 61 constantes d'extension, 2 fonctions) sont tous conditionnels à l'inventaire de classes. Tant que B1 n'est pas résolu, ces comptes ne sont pas verrouillables.

**B3. `Dom\XPath` marqué `final` sans justification**
La §3.3 déclare `final Dom\XPath`. Dans PHP 8.4, `Dom\XPath` n'est pas déclarée `final`; elle est sous-classable. Aucune modification de la version 8.5 ne la rend `final` à ma connaissance. La modification `final` ici est probablement une erreur de copie depuis `Dom\HTMLDocument`, `Dom\XMLDocument` ou `Dom\TokenList` qui, elles, sont réellement `final`. **Correction**: retirer le modificateur `final` de `Dom\XPath` ou prouver par le stub épinglé qu'il s'agit bien d'une modification volontaire de PHP 8.5.

---

### MAJOR

**M1. Épinglage de PHP 8.5.8 non démontrable**
La §2.1 s'appuie sur la version PHP 8.5.8 (commit `26b97507444c4fbda072f57dda1820f7b7d5e467`, archive `php-8.5.8.tar.xz` publiée le 2 juillet 2026, hash `58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2`). PHP 8.5 est en développement; une version 8.5.8 publiée en juillet 2026 suppose 8 versions correctives sur 8 mois après une GA de fin 2025 — calendrier plausible mais non démontrable. Le commit, l'archive et son hash ne peuvent pas être vérifiés sans accès à php-src au moment de la revue. **Correction**: confirmer l'existence et la disponibilité publique de l'archive, vérifier le hash, et confirmer la date de publication effective; sinon, repousser l'épinglage à une version existante et publiquement vérifiable, ou décrire une procédure de récupération et de vérification reproductible.

**M2. Version Lexbor 2.7.0 non vérifiable**
La §2.1 affirme que PHP 8.5.8 embarque Lexbor 2.7.0 (arbres `ext/lexbor` SHA `6bdcf7d6e7e9bd3946e87dda140ab1f8e4ef47be` et `ext/dom/lexbor` SHA `5b95c87cd4cbec6cb1eac347e79471fad79691b0`). PHP 8.4 embarque une version Lexbor antérieure (dans la branche 2.3.x). L'existence de Lexbor 2.7.0 et son intégration dans PHP 8.5.8 doivent être confirmées. **Correction**: fournir la preuve d'origine (release notes Lexbor 2.7.0, commit d'upgrade dans php-src); ou ajuster à la version réellement bundlée.

**M3. Version libxml2 2.15.3 non vérifiable**
La §2.2 épingle libxml2 2.15.3 (archive `libxml2-2.15.3.tar.xz`, hash `78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07`). La dernière version stable de libxml2 disponible dans mes données est proche de 2.13. La version 2.15.3 est plausible comme version future mais n'est pas démontrable. **Correction**: confirmer la disponibilité publique de l'archive, vérifier le hash; ou ajuster à une version existante.

**M4. Décomptes PHPT invérifiables**
La §6.2 énonce 868 fichiers `.phpt` sous `ext/dom/tests` (926 fichiers totaux), 32 sous `ext/libxml/tests` (36 totaux), 156 sous `ext/simplexml/tests` (164 totaux), total 1056. Ces nombres sont invérifiables sans accès à l'arbre source épinglé. **Correction**: inclure la sortie déterministe de `find ext/{dom,libxml,simplexml}/tests -name '*.phpt' | wc -l` et `find ext/{dom,libxml,simplexml}/tests -type f | wc -l` exécutée sur le commit épinglé, avec horodatage.

**M5. Décomptes de surface (313/184/16/61/2) invérifiables**
Ces nombres sont des affirmations non sourcées dans la spécification. La §6.1 mentionne que les manifestes sont générés depuis les stubs épinglés et le snapshot Reflection de l'oracle, mais ne les inclut pas. **Correction**: joindre les manifestes générés (au moins en référence stable) et confirmer les décomptes par un extrait de la sortie de `ReflectionExtension("dom")->getClasses()` suivie de l'agrégation.

**M6. Versions de modèles IA non démontrables**
La §8 nomme les endpoints `glm-5.2:cloud`, `kimi-k2.7-code:cloud`, et `minimax-m3:cloud`. Les versions GLM-5.2 et Kimi K2.7 ne sont pas attestées de manière certaine dans mes données; le versioning public connu de GLM est dans la série 4.x et celui de Kimi dans la série K2 d'origine. Le protocole de revue ne peut pas être exécuté si les endpoints ne sont pas effectivement disponibles. **Correction**: confirmer la disponibilité effective de ces modèles en tant qu'endpoints Ollama, ou ajuster à des modèles existants et vérifiables.

---

### MINOR

**m1. Rationale de `LIBXML_NO_XXE` imprécise**
La §3.4 dit: "`LIBXML_NO_XXE` is present because libxml2 is newer than 2.13.0". La constante est une définition de niveau PHP, introduite pour exploiter la politique d'entité renforcée de libxml2 ≥ 2.13. Sa présence n'est pas strictement conditionnée par la version de libxml2, mais par les choix de configuration du build PHP. **Correction**: reformuler en indiquant que la constante est définie par PHP pour refléter la politique d'entité de libxml2 ≥ 2.13, et que sa valeur numérique est celle définie dans le stub épinglé.

**m2. Auto-insertion non spécifiée**
La §4.5 mentionne "preserve self-insertion and evaluation rules" sans détailler le comportement PHP exact pour `appendChild`, `insertBefore`, `replaceChild`, `removeChild` lorsque l'argument est l'appelant lui-même. **Correction**: spécifier pour chaque méthode si l'auto-insertion déclenche une `DOMException` (code `DOM_HIERARCHY_REQUEST_ERR` typiquement) ou est sans effet, avec le message exact.

**m3. Accès non initialisé non spécifié**
La §4.4 mentionne "uninitialized access all match PHP" sans détailler les cas (propriétés internes ou visibles, lecture/écriture, diagnostics). **Correction**: lister les propriétés internes affectées, le diagnostic levé, et les chemins d'accès utilisateur qui les déclenchent.

**m4. `LibXMLError::$file` incohérent avec l'API publique documentée**
La §3.5 liste `LibXMLError` avec ses six propriétés typées publiques; cela inclut `level`, `code`, `column`, `message`, `file`, `line`. La spécification dit "exact construction, cloning, debug, and serialization restrictions" — correct — mais ne mentionne pas explicitement que les propriétés sont en lecture seule (toute écriture directe est ignorée ou provoque un warning en PHP). **Correction**: expliciter la sémantique read-only des six propriétés ou renvoyer au PHPT de référence.

---

### NIT

**n1. Casse incohérente de l'identifiant de modèle**
La §8 utilise `minimax-m3` (tout en minuscules), tandis que la convention stable et la désignation de l'endpoint de revue sont `MiniMax-M3` (camel case avec capitales M). **Correction**: aligner la casse sur `MiniMax-M3` pour la cohérence du protocole de revue.

**n2. "final" redondant sur classes déjà non-sous-classables par construction**
La §3.3 marque `final` sur `Dom\HTMLDocument`, `Dom\XMLDocument`, `Dom\TokenList`, `Dom\NamespaceInfo`, `Dom\XPath`. C'est cohérent avec PHP pour les trois premières, mais voir B3 pour `Dom\XPath` et B1 pour `Dom\NamespaceInfo`. **Correction**: conditionné à la résolution de B1 et B3.

---

### QUESTION

**Q1.** La classe `Dom\NamespaceInfo` existe-t-elle effectivement dans le stub `ext/dom/php_dom.stub.php` du commit `26b9750...`? Si oui, quels sont ses membres, constructeurs, et son statut `final`?

**Q2.** PHP 8.5.8 a-t-il été effectivement publié le 2 juillet 2026 avec le hash `58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2` et le commit `26b9750...`? Le cas échéant, où peut-on obtenir l'archive et vérifier le hash de manière reproductible?

**Q3.** Lexbor 2.7.0 a-t-il été releasé publiquement et intégré dans le `ext/lexbor` bundlé de PHP 8.5.8?

**Q4.** libxml2 2.15.3 a-t-il été releasé publiquement et son archive `libxml2-2.15.3.tar.xz` est-elle accessible avec le hash `78262a6e...42c7a07`?

**Q5.** Les comptes PHPT (868/32/156, totaux 926/36/164) sont-ils confirmés par exécution directe de `find` sur l'arbre source épinglé?

**Q6.** Les comptes de surface (313 méthodes, 184 propriétés, 16 constantes/cas, 61 constantes d'extension, 2 fonctions) sont-ils confirmés par la Reflection JSON de l'oracle construit selon §2.3?

**Q7.** Les modèles `glm-5.2:cloud` et `kimi-k2.7-code:cloud` existent-ils en tant qu'endpoints Ollama stables, accessibles, et configurés pour le protocole de revue? Sinon, quels modèles équivalents doivent être substitués?

---

### Conclusion

Plusieurs préoccupations de classe BLOCKER (B1-B3) et MAJOR (M1-M6), ainsi que sept questions ouvertes (Q1-Q7), empêchent l'émission d'un LOCK. La spécification doit, dans l'ordre:

1. Résoudre l'existence et la composition de `Dom\NamespaceInfo` (B1) et la qualification `final` de `Dom\XPath` (B3);
2. Confirmer l'épinglage de PHP 8.5.8, Lexbor 2.7.0, libxml2 2.15.3 par preuve d'archive et de hash (M1-M3);
3. Sourcer les comptes de surface et de PHPT par sortie déterministe (M4-M5);
4. Confirmer la disponibilité des modèles de revue (M6);
5. Préciser les comportements d'auto-insertion et d'accès non initialisé (m2-m3);
6. Aligner la casse de l'identifiant de modèle (n1).

Tant que ces points ne sont pas résolus avec preuve documentaire sourcée contre les artefacts épinglés, le consensus absolu (zéro réserve, zéro question) n'est pas atteint et aucun LOCK ne peut être émis.
```
