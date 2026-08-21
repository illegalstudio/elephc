# Kimi K2.7 raw response — specification round 1

```text
Je ne peux pas émettre de verrou. L’artifact contient au moins une réserve démontrable et plusieurs points à éclaircir avant qu’un consensus absolu soit possible.

---

## Résumé des réserves

| Classe | Nombre | Objet |
|---|---|
| **BLOCKER** | 1 | Surface des fonctions `dom_import_simplexml` / `Dom\import_simplexml` |
| **QUESTION** | 4 | Statut de `Dom\DOMException`, classe/message d’erreur d’import croisé, modificateurs `abstract`/`final` des classes modernes, cache des wrappers sans document |
| **NIT** | 2 | Terminologie « system-library fallbacks » vs liens système obligatoires, politique `cargo fmt` non complétée |

Aucun `LOCK` n’est donné.

---

## Détails

### BLOCKER — B1 : paramètre `?string $class_name = null` manquant sur les deux fonctions d’import SimpleXML

**Références :** §3.2, §3.3, §4.4, §4.13.

**Fait démontrable :** dans `php-src` actuel (et dans la documentation PHP), `dom_import_simplexml` et `Dom\import_simplexml` possèdent un second paramètre optionnel servant à la sous-classe enregistrée par `registerNodeClass` :

```php
function dom_import_simplexml(object $node, ?string $class_name = null): DOMAttr|DOMElement;
namespace Dom {
    function import_simplexml(object $node, ?string $class_name = null): Attr|Element;
}
```

La spécification ne liste qu’un seul paramètre. Cela rompt directement le critère « names, parameter names and order, defaults, named arguments » du §1.1 et la cohérence avec `registerNodeClass()` (§4.4).

**Correction :** ajouter `?string $class_name = null` comme deuxième paramètre dans les deux signatures du §3.2 et §3.3, et propager dans le manifeste généré, les tests de parité et les exemples. Si le stub épinglé de `php-8.5.8` imposait un nom de paramètre différent de `$class_name` ou un type de premier paramètre autre que `object`, ajuster uniquement ces détails, mais le second paramètre lui-même doit être présent.

---

### QUESTION — Q1 : `Dom\DOMException` est-il vraiment un alias insensible à la casse de `DOMException` ?

**Références :** §3.1, §3.3.

Le §3.3 affirme que `Dom\DomException` est un alias insensible à la casse, que `Reflection` rapporte le nom canonique `DOMException`, et que cela justifie le décompte 51 noms exportés / 50 définitions canoniques.

**À confirmer avec l’oracle :** dans les branches `php-src` récentes, `Dom\DOMException` est souvent une classe interne **fille** de `DOMException` (et non un alias). Si c’est le cas :
- `ReflectionClass('Dom\DOMException')->getName()` renvoie `Dom\DOMException`, et
- le nombre de définitions canoniques passe à 51, ce qui invalide le décompte, le test de parité et la justification « DOMException::$code rencontré deux fois ».

**Correction conditionnelle :**
- Si c’est un alias : fournir l’extrait du `php_dom.stub.php` épinglé montrant l’alias, et conserver le décompte.
- Si c’est une classe fille : corriger le §3.3, le décompte du §3.1, le manifeste et les tests de parité pour refléter `Dom\DOMException` comme définition canonique distincte.

---

### QUESTION — Q2 : classe et message exacts pour l’import SimpleXML croisé legacy ↔ moderne

**Références :** §3.6.

Le §3.6 affirme que les deux cas lèvent un `TypeError` avec les messages exacts :
- `Dom\import_simplexml(): Argument #1 ($node) must not be already imported as a DOMNode`
- `dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\Node`

**À confirmer avec l’oracle :** dans `php-src`, une vérification d’état *après* la vérification de type (nœud déjà importé) est généralement levée par `zend_argument_value_error()` et donne un **`ValueError`**, non un `TypeError`. Le texte exact (`DOMNode` vs `Dom\Node`, avec ou sans antislash, avec ou sans article) doit être extraite du PHPT ou de l’oracle.

**Correction conditionnelle :** remplacer la classe et le message par ceux observés sur l’oracle ; ne pas les inscrire comme normatifs tant qu’ils ne sont pas prouvés.

---

### QUESTION — Q3 : modificateurs `abstract`/`final` des classes modernes `Dom\…`

**Références :** §3.3.

La liste explicative du §3.3 ne marque `abstract` que sur `Dom\Document`, et `final` uniquement sur `HTMLDocument`, `XMLDocument`, `TokenList`, `NamespaceInfo` et `XPath`. Elle laisse `Dom\Node`, `Dom\CharacterData`, `Dom\Element`, `Dom\Attr`, etc., sans modificateur.

**À confirmer avec le manifeste/oracle :** dans `php-src` 8.4/8.5, les classes de base du modèle DOM (`Node`, `CharacterData`, `Element`, `Document`) sont généralement `abstract`, et les classes feuilles (`Attr`, `Text`, `Comment`, `DocumentFragment`, `NodeList`, `NamedNodeMap`, `HTMLCollection`, `Implementation`, `XPath`, etc.) sont généralement `final`.

Bien que le §3.3 dise que le manifeste généré est l’autorité, une liste explicative incorrecte risque d’induire en erreur les implémenteurs et les tests de parité.

**Correction conditionnelle :** aligner la liste du §3.3 avec les modificateurs exacts du `php_dom.stub.php` épinglé ; si des classes sont effectivement non-final/non-abstract dans la version cible, le manifeste doit le refléter.

---

### QUESTION — Q4 : propriétaire du cache de wrappers pour les nœuds sans document

**Références :** §4.2, §5.4.

Le §4.2 décrit un « document-owned weak wrapper cache ». Or certains objets DOM n’ont pas de document propriétaire, par exemple un `DOMDocumentType` créé par `DOMImplementation::createDocumentType()` (ou son équivalent moderne), ou un nœud après `ownerDocument` `null` dans certaines situations.

**Impact :** si le cache est strictement documentaire, l’identité du wrapper et la règle « repeated access returns the same live PHP wrapper » ne sont pas définies pour ces cas.

**Correction :** ajouter au §4.2 ou §5.4 une règle explicite : nœuds sans `ownerDocument` utilisent un cache global par contexte d’exécution (ou par `DOMImplementation`), et préciser comment la clé de cache (identité native + classe enregistrée) s’applique dans ce cas.

---

### NIT — N1 : « system-library fallbacks » vs liens système obligatoires

**Références :** §1.6, §5.1.

Le §1.6 interdit les « system-library fallbacks », tandis que le §5.1 liste des liens système cibles obligatoires (macOS `iconv`, Linux C/pthread/dl).

**Correction :** ajouter une phrase au §1.6 ou au §5.1 précisant que les bibliothèques système **obligatoires au runtime** (`libc`, `libpthread`, `libdl`, `libiconv` sur macOS) ne sont pas des « fallbacks » réduits ; l’interdiction vise les substitutions cibles qui masquent un comportement manquant ou une validation réduite.

---

### NIT — N2 : politique `cargo fmt`

**Références :** §6.5, §6.7.

Le §6.5 stipule : « `cargo fmt` and `cargo fmt --all` are never run in this repository. »

**Correction :** compléter par la méthode de contrôle de style effectivement utilisée (par exemple `rustfmt --check` en CI, ou un guide de style manuel), afin que la porte d’hygiène reste vérifiable et reproductible.

---

## Conclusion

L’artifact est globalement cohérent dans son architecture et dans la définition de « complet », mais la réserve **BLOCKER** B1 et les quatre **QUESTION** ci-dessus empêchent tout consensus absolu en l’état. Aucune ligne `LOCK SPEC 63eda127e9c6959848c9b86676d6399572539b8d52b4dfd4074f679e1df313fc` n’est émise.
```
