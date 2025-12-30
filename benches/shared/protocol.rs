#![allow(dead_code)]

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{self, Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Exec {
        method: Method,
        script: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<ByteBuf>,
    },
    Echo {
        size: usize,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Exec {
        status: u16,
        #[serde(with = "serde_bytes")]
        body: Vec<u8>,
    },
    Echo {
        size: usize,
    },
    Error {
        message: String,
    },
}

pub fn write_message<W: Write>(
    w: &mut W,
    msg: &impl Serialize,
) -> io::Result<()> {
    let payload = rmp_serde::to_vec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&payload)?;
    w.flush()?;

    Ok(())
}

pub fn read_message<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;

    rmp_serde::from_slice(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::{
        read_message, write_message, ByteBuf, Method, Request, Response,
    };

    #[test]
    fn test_roundtrip_exec_request() {
        let req = Request::Exec {
            method: Method::Post,
            script: "test.php".into(),
            uri: Some("/api/test".into()),
            content_type: Some("application/json".into()),
            body: Some(ByteBuf::from(b"hello".to_vec())),
        };

        let mut buf = Vec::new();
        write_message(&mut buf, &req).unwrap();

        let decoded: Request = read_message(&mut buf.as_slice()).unwrap();

        match decoded {
            Request::Exec { method, script, .. } => {
                assert_eq!(method, Method::Post);
                assert_eq!(script, "test.php");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_roundtrip_response() {
        let resp = Response::Exec {
            status: 200,
            body: b"Hello World".to_vec(),
        };

        let mut buf = Vec::new();
        write_message(&mut buf, &resp).unwrap();

        let decoded: Response = read_message(&mut buf.as_slice()).unwrap();

        match decoded {
            Response::Exec { status, body } => {
                assert_eq!(status, 200);
                assert_eq!(body, b"Hello World");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_error_response() {
        let resp = Response::Error {
            message: "script not found".into(),
        };

        let mut buf = Vec::new();
        write_message(&mut buf, &resp).unwrap();

        let decoded: Response = read_message(&mut buf.as_slice()).unwrap();

        match decoded {
            Response::Error { message } => {
                assert_eq!(message, "script not found");
            }
            _ => panic!("wrong variant"),
        }
    }
}
