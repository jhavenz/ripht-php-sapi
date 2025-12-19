<?php
/**
 * Error handling test script.
 * 
 * Tests PHP error and warning generation for error capture.
 */

header('Content-Type: application/json');

error_log('This is a test error message');
trigger_error('This is a user-generated warning', E_USER_WARNING);
trigger_error('This is a user-generated notice', E_USER_NOTICE);

$response = [
    'status' => 'ok',
    'message' => 'Errors and warnings were generated',
];

echo json_encode($response, JSON_PRETTY_PRINT);

