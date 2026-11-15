extern class StaticSetterOutput { public ptr $cell; }
extern function __elephc_eval_value_static_property_set_v2(string $class, int $classLength,
    string $property, int $propertyLength, ptr $value, string $scope, int $scopeLength, ptr $error): int;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function __elephc_eval_value_new_object(string $class, int $length): ptr;
extern function __elephc_eval_value_release_v3(ptr $value, ptr $error): int;
extern function malloc(int $size): ptr;
extern function free(ptr $value): void;
class StaticSetterPrivate { private static int $value = 1;
    public static function read(): int { return self::$value; }
}
$error = malloc(8); ptr_set($error, 0);
$retired = malloc(8); ptr_set($retired, 0);
$thrown = ptr_cast<StaticSetterOutput>($error);
$input = __elephc_eval_value_int(7);
echo __elephc_eval_value_static_property_set_v2('StaticWriteTarget', 17, 'number', 6, $input, '', 0, $error);
echo ':', gettype(StaticWriteTarget::$number), ':', StaticWriteTarget::$number;
__elephc_eval_value_release_v3($input, $retired);
$input = __elephc_eval_value_array_new(0);
echo '|', __elephc_eval_value_static_property_set_v2('StaticWriteTarget', 17, 'number', 6, $input, '', 0, $error);
echo ptr_get($error) ? ':E:' : ':X:', StaticWriteTarget::$number;
__elephc_eval_value_release_v3($thrown->cell, $retired); ptr_set($error, 0);
__elephc_eval_value_release_v3($input, $retired);
$input = __elephc_eval_value_new_object('StaticWriteText', 15);
echo '|', __elephc_eval_value_static_property_set_v2('StaticWriteTarget', 17, 'late', 4, $input, '', 0, $error);
echo ':', StaticWriteTarget::$late;
__elephc_eval_value_release_v3($input, $retired);
$input = __elephc_eval_value_new_object('StaticWriteFailure', 18);
echo '|', __elephc_eval_value_static_property_set_v2('StaticWriteTarget', 17, 'failure', 7, $input, '', 0, $error);
echo ptr_get($error) && !isset(StaticWriteTarget::$failure) ? ':E' : ':X';
__elephc_eval_value_release_v3($thrown->cell, $retired); ptr_set($error, 0);
__elephc_eval_value_release_v3($input, $retired);
$input = __elephc_eval_value_new_object('StaticWriteReentry', 18);
echo '|', __elephc_eval_value_static_property_set_v2('StaticWriteTarget', 17, 'nested', 6, $input, '', 0, $error);
echo ':', StaticWriteTarget::$nested;
__elephc_eval_value_release_v3($input, $retired);
$input = __elephc_eval_value_int(9);
echo '|', __elephc_eval_value_static_property_set_v2('StaticSetterPrivate', 19, 'value', 5, $input, '', 0, $error);
echo ':', __elephc_eval_value_static_property_set_v2('StaticSetterPrivate', 19, 'value', 5, $input, 'StaticSetterPrivate', 19, $error);
echo ':', StaticSetterPrivate::read();
__elephc_eval_value_release_v3($input, $retired);
free($error); free($retired);
gc_collect_cycles();
