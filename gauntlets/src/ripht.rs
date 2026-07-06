use std::time::Instant;

use ripht_php_sapi::{RiphtSapi, WebRequest};

use crate::{
    GauntletCase, HeaderValue, HttpMethod, RuntimeAdapter, RuntimeFailure,
    RuntimeFailureKind, RuntimeMessage, RuntimeMode, RuntimeResult,
};

pub struct RiphtBufferedAdapter {
    sapi: RiphtSapi,
}

impl RiphtBufferedAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtBufferedAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtBufferedAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtBuffered
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let mut request = request_builder(case);

        if let Some(body) = &case.body {
            request = request.with_body(body.clone());
        }

        if let Some(content_type) = case.content_type {
            request = request.with_content_type(content_type);
        }

        let ctx = match request.build(case.script_path()) {
            Ok(ctx) => ctx,
            Err(err) => {
                return RuntimeResult::failure(
                    self.name(),
                    self.mode(),
                    case.name,
                    started_at.elapsed(),
                    RuntimeFailure::new(
                        RuntimeFailureKind::BuildRequest,
                        err.to_string(),
                    ),
                );
            }
        };

        let result = match self.sapi.execute(ctx) {
            Ok(result) => result,
            Err(err) => {
                return RuntimeResult::failure(
                    self.name(),
                    self.mode(),
                    case.name,
                    started_at.elapsed(),
                    RuntimeFailure::new(
                        RuntimeFailureKind::Execute,
                        err.to_string(),
                    ),
                );
            }
        };

        let headers = result
            .all_headers()
            .map(|header| HeaderValue::new(header.name(), header.value()))
            .collect();

        let messages = result
            .all_messages()
            .map(|message| RuntimeMessage {
                level: message.level.to_string(),
                message: message.message.clone(),
            })
            .collect();

        RuntimeResult {
            runtime: self.name().to_string(),
            mode: self.mode(),
            case: case.name.to_string(),
            status_code: Some(result.status_code()),
            exit_status: Some(result.exit_status()),
            headers,
            body: result.body(),
            messages,
            report: None,
            events: Vec::new(),
            duration_ms: started_at
                .elapsed()
                .as_millis(),
            artifact_path: None,
            failure: None,
        }
    }
}

fn request_builder(case: &GauntletCase) -> WebRequest {
    match case.method {
        HttpMethod::Get => WebRequest::get(),
        HttpMethod::Post => WebRequest::post(),
        HttpMethod::Put => WebRequest::put(),
        HttpMethod::Delete => WebRequest::delete(),
        HttpMethod::Patch => WebRequest::patch(),
        HttpMethod::Head => WebRequest::head(),
        HttpMethod::Options => WebRequest::options(),
    }
}
