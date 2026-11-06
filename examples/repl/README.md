# Interactive pricing session

From the repository root:

```text
$ elephc repl
>>> require_once 'examples/repl/main.php';
int(1)
>>> $price = 100;
int(100)
>>> totalWithTax($price, 0.22)
float(122)
>>> function discount($price, $percent) {
...     return $price * (1 - $percent / 100);
... }
>>> totalWithTax(discount($price, 10), 0.22)
float(109.8)
>>> das
Error: eval() runtime failed
>>> $price
int(100)
>>> :quit
```

The function declaration and `$price` survive across submissions, including an
eval error such as the unknown constant above. Restarting the
REPL starts a new session while reusing the cached native host.
