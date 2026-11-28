<?php $value=$argc;try{settype($value,'string');echo 'W';}catch(TypeError $error){echo 'T';}echo gettype($value),'|',$value;
