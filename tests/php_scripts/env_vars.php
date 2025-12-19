<?php

header('Content-Type: application/json');

$val = getenv('TEST_ENV_KEY');
$missing = getenv('MISSING_ENV_KEY');

echo json_encode([
    'TEST_ENV_KEY' => $val === false ? null : $val,
    'MISSING_ENV_KEY' => $missing === false ? null : $missing,
], JSON_PRETTY_PRINT);
