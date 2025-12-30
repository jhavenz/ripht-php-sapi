#![allow(dead_code)]

use super::protocol::Method;

pub trait Backend {
    fn name(&self) -> &'static str;

    fn execute(
        &mut self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub struct BenchSuite {
    pub name: &'static str,
    pub script: &'static str,
    pub method: Method,
    pub body: Option<&'static [u8]>,
}

impl BenchSuite {
    pub const fn new(
        name: &'static str,
        script: &'static str,
        method: Method,
    ) -> Self {
        Self {
            name,
            script,
            method,
            body: None,
        }
    }

    pub const fn with_body(mut self, body: &'static [u8]) -> Self {
        self.body = Some(body);
        self
    }
}
