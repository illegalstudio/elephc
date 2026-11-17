<?php $value=true;try{settype($value,'int');echo 'W';}catch(TypeError $error){echo 'T';}echo gettype($value),'|',$value;
