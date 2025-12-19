<?php
/**
 * JSON POST test script.
 * 
 * Tests handling of JSON request bodies.
 */

header('Content-Type: application/json');

$input = file_get_contents('php://input');
$json_data = json_decode($input, true);

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'content_type' => $_SERVER['CONTENT_TYPE'] ?? null,
    'raw_input' => $input,
    'json_decoded' => $json_data,
    'input_length' => strlen($input),
];

echo json_encode($response, JSON_PRETTY_PRINT);

