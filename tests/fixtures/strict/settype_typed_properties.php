<?php
class TypedConversionPlace {
    public string $value = '18.9';
    public int $number = 13;
    public float $amount = 17.9;
    public bool $enabled = true;
}
$box = new TypedConversionPlace();
$alias =& $box->value;
$numberAlias =& $box->number;
$amountAlias =& $box->amount;
$enabledAlias =& $box->enabled;
settype($box->value, 'int');
echo $box->value, '|', $alias;
try { settype($box->value, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|A', $box->value; }
$box->value = '20.9';
echo '|', $alias;
$cast = settype(...);
$cast($box->value, 'int');
echo '|', $alias;
$cast($box->number, 'string');
echo '|', $box->number;
settype($box->amount, 'int');
echo '|', $box->amount;
$cast($box->enabled, 'int');
echo '|', $box->enabled;
$name = $argc > 0 ? 'value' : 'value';
$box->value = '23.9';
settype($box->{$name}, 'int');
echo '|', $alias;
