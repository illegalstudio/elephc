<?php
class ObjectConversionItem {
    public string $name = 'item';
    public function __toString(): string { echo 'S'; return 'item'; }
}
class ObjectConversionPlace {
    public ObjectConversionItem $value;
    public ?ObjectConversionItem $optional = null;
    public ObjectConversionItem|string $choice;
    public function __construct() {
        $this->value = new ObjectConversionItem();
        $this->optional = new ObjectConversionItem();
        $this->choice = new ObjectConversionItem();
    }
}
$box = new ObjectConversionPlace();
$value =& $box->value;
$optional =& $box->optional;
$choice =& $box->choice;
$cast = settype(...);
try { settype($box->value, 'string'); echo 'X'; }
catch (TypeError $error) { echo 'C'; }
echo $value instanceof ObjectConversionItem ? '|I' : '|X';
$cast($box->choice, 'string');
echo '|', $choice;
settype($box->optional, 'null');
echo is_null($optional) ? '|N' : '|X';
$projection = (array)$box;
echo '|', get_class($projection['value']);
echo '|', json_encode($box);
function replaceObjectConversionValue(mixed &$value, mixed $replacement): void {
    echo 'W';
    $value = $replacement;
}
$replace = substr('replaceObjectConversionValue', 0, strlen($argv[0]) + 1024);
try { $replace($box->value, new stdClass()); echo 'X'; }
catch (TypeError $error) { echo '|R'; }
