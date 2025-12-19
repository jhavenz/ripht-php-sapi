<?php
/**
 * Large input test script.
 * 
 * Tests handling of large request bodies.
 */

header('Content-Type: application/json');

$input = file_get_contents('php://input');
$input_length = strlen($input);

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'content_length' => $_SERVER['CONTENT_LENGTH'] ?? null,
    'input_length' => $input_length,
    'input_received' => $input_length > 0,
];

echo json_encode($response, JSON_PRETTY_PRINT);

