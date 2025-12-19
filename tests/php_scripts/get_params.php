<?php

header('Content-Type: application/json');

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'get_params' => $_GET,
    'query_string' => $_SERVER['QUERY_STRING'] ?? null,
    'param_count' => count($_GET),
];

echo json_encode($response, JSON_PRETTY_PRINT);

