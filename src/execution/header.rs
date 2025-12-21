/// HTTP response header
///
/// Malformed headers from PHP are silently dropped during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHeader {
    name: String,
    value: String,
}

impl ResponseHeader {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Parses "Name: Value" format per RFC 7230.
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        let colon_pos = memchr::memchr(b':', bytes)?;
        if colon_pos == 0 {
            return None;
        }

        let name_bytes = &bytes[..colon_pos];
        let mut value_start = colon_pos + 1;

        // Skip OWS after colon
        while value_start < bytes.len()
            && bytes[value_start].is_ascii_whitespace()
        {
            value_start += 1;
        }

        let value_bytes = &bytes[value_start..];

        let name_str = std::str::from_utf8(name_bytes)
            .ok()?
            .trim();
        if name_str.is_empty() {
            return None;
        }

        let value_string = match std::str::from_utf8(value_bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => String::from_utf8_lossy(value_bytes).into_owned(),
        };

        Some(Self {
            name: name_str.to_string(),
            value: value_string,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let h = ResponseHeader::new("Content-Type", "text/html");
        assert_eq!(h.name(), "Content-Type");
        assert_eq!(h.value(), "text/html");
    }

    #[test]
    fn test_parse_basic() {
        let h =
            ResponseHeader::parse(b"Content-Type: application/json").unwrap();
        assert_eq!(h.name(), "Content-Type");
        assert_eq!(h.value(), "application/json");
    }

    #[test]
    fn test_parse_with_whitespace() {
        let h = ResponseHeader::parse(b"Content-Type:   application/json  ")
            .unwrap();
        assert_eq!(h.name(), "Content-Type");
        assert_eq!(h.value(), "application/json  ");
    }

    #[test]
    fn test_parse_empty_value() {
        let h = ResponseHeader::parse(b"X-Empty:").unwrap();
        assert_eq!(h.name(), "X-Empty");
        assert_eq!(h.value(), "");
    }

    #[test]
    fn test_parse_no_colon() {
        assert!(ResponseHeader::parse(b"InvalidHeader").is_none());
    }

    #[test]
    fn test_parse_colon_at_start() {
        assert!(ResponseHeader::parse(b": value").is_none());
    }

    #[test]
    fn test_parse_whitespace_only_name() {
        assert!(ResponseHeader::parse(b"   : value").is_none());
    }

    #[test]
    fn test_parse_colon_in_value() {
        let h = ResponseHeader::parse(b"X-Timestamp: 12:34:56").unwrap();
        assert_eq!(h.name(), "X-Timestamp");
        assert_eq!(h.value(), "12:34:56");
    }

    #[test]
    fn test_parse_non_utf8_value() {
        let h = ResponseHeader::parse(b"X-Binary: \xff\xfe").unwrap();
        assert_eq!(h.name(), "X-Binary");
        assert!(h.value().contains('\u{FFFD}'));
    }

    #[test]
    fn test_parse_non_utf8_name() {
        assert!(ResponseHeader::parse(b"X-\xff-Header: value").is_none());
    }
}
