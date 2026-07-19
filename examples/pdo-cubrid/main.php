<?php

$dsn = getenv('ELEPHC_CUBRID_DSN') ?: 'cubrid:host=127.0.0.1;port=33000;dbname=cubdb';
$db = new PDO($dsn, 'dba', '', [
    PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
]);

$db->exec('DROP TABLE IF EXISTS elephc_animals');
$db->exec('CREATE TABLE elephc_animals (id INTEGER AUTO_INCREMENT PRIMARY KEY, name VARCHAR(80))');

$insert = $db->prepare('INSERT INTO elephc_animals(name) VALUES (:name)');
$insert->execute(['name' => 'elephant']);

$row = $db->query('SELECT id, name FROM elephc_animals')->fetch(PDO::FETCH_ASSOC);
echo $row['id'] . ': ' . $row['name'] . PHP_EOL;

$tables = $db->cubrid_schema(PDO::CUBRID_SCH_TABLE, 'elephc_animals');
echo 'schema rows: ' . count($tables) . PHP_EOL;

$db->exec('DROP TABLE elephc_animals');
