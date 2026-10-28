<?php
class LateRecord extends DemoRecord {
    public function vars() { return get_object_vars($this); }
    public function defaults() { return get_class_vars('DemoRecord'); }
    public function init() { $this->late = 5; return $this; }
}
class Mangled { public function __debugInfo(): array { return ["\0Mangled\0hidden" => 1, "\0*\0shared" => 2, "open" => 3]; } }
$r = new DemoRecord(7);
$k = (new LateRecord(3))->init();
var_dump(get_object_vars($r));
var_dump($k->vars());
foreach ((array) $r as $key => $value) { echo str_replace("\0", '~', $key), '=', var_export($value, true), "\n"; }
var_dump($k);
print_r($k);
echo "\n";
var_dump(property_exists('DemoRecord', 'secret'), property_exists($r, 'kind'), property_exists('DemoRecord', 'nope'), property_exists('DemoCounter', 'label'));
var_dump(get_class_vars('DemoCounter'));
var_dump($k->defaults());
var_dump(new Mangled);
print_r(new Mangled);
echo "\n";
