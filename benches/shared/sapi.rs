use super::backend::Backend;
use super::env::scripts_dir;
use super::protocol::Method;
use ripht_php_sapi::{RiphtSapi, WebRequest};

pub struct SapiBackend {
    sapi: RiphtSapi,
}

impl SapiBackend {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for SapiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for SapiBackend {
    fn name(&self) -> &'static str {
        "rust_sapi"
    }

    fn execute(
        &mut self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let script_path = scripts_dir().join(script);

        let mut builder = match method {
            Method::Get => WebRequest::get(),
            Method::Post => WebRequest::post(),
            Method::Put => WebRequest::put(),
            Method::Delete => WebRequest::delete(),
            Method::Patch => WebRequest::patch(),
            Method::Head => WebRequest::head(),
            Method::Options => WebRequest::options(),
        };

        if let Some(b) = body {
            builder = builder
                .with_body(b.to_vec())
                .with_content_type("application/json");
        }

        let ctx = builder
            .build(&script_path)
            .ok()?;
        let result = self.sapi.execute(ctx).ok()?;

        Some(result.body().to_vec())
    }
}
