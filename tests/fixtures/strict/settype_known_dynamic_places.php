<?php
class KnownDynamicConversionPlace {
    public mixed $value = '13.9';
    public function __construct() { $alias =& $this->value; }
}
$box = new KnownDynamicConversionPlace(); $name = $argc > 0 ? 'value' : 'VALUE';
settype($box->{$name}, 'int'); echo $box->value, '|';
$box->value = '14.9'; settype(type: 'int', var: $box->{$name}); echo $box->value, '|';
$convert = settype(...); $box->value = '15.9'; $convert($box->{$name}, 'int'); echo $box->value, '|';
$box->value = '16.9'; $convert(type: 'int', var: $box->{$name}); echo $box->value, '|';
$box->value = '17.9'; call_user_func('settype', $box->{$name}, 'int'); echo $box->value, '|';
call_user_func_array('settype', [$box->{$name}, 'int']); echo $box->value, '|';
$box->value = 'junk';
try { settype($box->{$name}, 'int'); echo 'X'; } catch (ValueError $error) { echo $box->value, '|'; }
try { call_user_func('settype', $box->{$name}, 'int'); echo 'X'; } catch (ValueError $error) { echo $box->value; }
