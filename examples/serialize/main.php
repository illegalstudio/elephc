<?php
// serialize — demonstrates serialize()/unserialize() and how Phar uses them to
// persist global metadata into an archive.

// serialize() produces PHP's exact wire format for scalars and arrays.
echo serialize(42), "\n";                 // i:42;
echo serialize(3.5), "\n";                // d:3.5;
echo serialize("hi"), "\n";               // s:2:"hi";
echo serialize(true), "\n";               // b:1;
echo serialize(null), "\n";               // N;
echo serialize([1, 2, 3]), "\n";          // a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}
echo serialize(["name" => "Ada", "age" => 36]), "\n";

// unserialize() is the inverse — round-trips back to the original value.
$blob = serialize(["lang" => "PHP", "stars" => 5, "tags" => ["fast", "native"]]);
$restored = unserialize($blob);
echo $restored["lang"], " has ", $restored["stars"], " stars\n";
echo "first tag: ", $restored["tags"][0], "\n";

// unserialize() returns false on malformed input.
var_dump(unserialize("not valid"));

// Objects serialize as O:<len>:"<Class>":<count>:{...} with PHP's exact key
// mangling (public bare, protected \0*\0name, private \0Class\0name).
class Point { public int $x = 1; protected int $y = 2; private int $z = 3; }
echo serialize(new Point()), "\n";

// __serialize()/__unserialize() customise the wire form: the object body is the
// returned array, and __unserialize() restores it.
class Money {
    public int $cents = 0;
    public string $currency = "USD";
    public function __serialize(): array {
        return ["cents" => $this->cents, "currency" => $this->currency];
    }
    public function __unserialize(array $data): void {
        $this->cents = (int) $data["cents"];
        $this->currency = (string) $data["currency"];
    }
}
$m = new Money();
$m->cents = 1299;
$m->currency = "EUR";
$back = unserialize(serialize($m));
echo $back->cents, " ", $back->currency, "\n";   // 1299 EUR

// Repeated objects become r:<index>; back-references and rebuild as one shared
// instance, so identity (===) survives a serialize()/unserialize() round-trip.
$shared = new Point();
$pair = unserialize(serialize([$shared, $shared]));
echo ($pair[0] === $pair[1] ? "same instance" : "two instances"), "\n";

// Phar stores its global metadata as a serialize()d blob, so metadata set on one
// Phar object is read back by another — and by the PHP interpreter.
$path = "build.phar";
$phar = new Phar($path);
$phar->addFromString("app.php", "<?php echo 'hello';");
$phar->setMetadata(["version" => "1.0.0", "author" => "elephc"]);

$reopened = new Phar($path);
$meta = $reopened->getMetadata();
echo "phar version: ", $meta["version"], " by ", $meta["author"], "\n";
echo "has metadata: ", $reopened->hasMetadata() ? "yes" : "no", "\n";
