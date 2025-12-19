<?php
/**
 * POST form data test script.
 * 
 * Tests $_POST superglobal population and raw input handling.
 */

header('Content-Type: application/json');

$input = file_get_contents('php://input');
$content_type = $_SERVER['CONTENT_TYPE'] ?? null;

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'content_type' => $content_type,
    'post_data' => $_POST,
    'raw_input_length' => strlen($input),
    'post_count' => count($_POST),
];

echo json_encode($response, JSON_PRETTY_PRINT);

