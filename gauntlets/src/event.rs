#[derive(Debug, Clone, serde::Serialize)]
pub struct HeaderValue {
    pub name: String,
    pub value: String,
}

impl HeaderValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LifecycleEvent {
    Headers {
        status_code: u16,
        headers: Vec<HeaderValue>,
    },
    Write {
        bytes: Vec<u8>,
    },
    Flush,
    Finish,
    Abort {
        reason: String,
    },
}
