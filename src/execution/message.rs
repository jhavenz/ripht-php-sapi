#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyslogLevel {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl SyslogLevel {
    pub fn from_raw(level: i32) -> Self {
        match level {
            0 => Self::Emergency,
            1 => Self::Alert,
            2 => Self::Critical,
            3 => Self::Error,
            4 => Self::Warning,
            5 => Self::Notice,
            6 => Self::Info,
            7 => Self::Debug,
            _ if level < 0 => Self::Emergency,
            _ => Self::Debug,
        }
    }

    pub fn is_error_or_worse(&self) -> bool {
        matches!(
            self,
            Self::Emergency | Self::Alert | Self::Critical | Self::Error
        )
    }

    pub fn is_warning_or_worse(&self) -> bool {
        self.is_error_or_worse() || matches!(self, Self::Warning)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Emergency => "emerg",
            Self::Alert => "alert",
            Self::Critical => "crit",
            Self::Error => "err",
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

impl std::fmt::Display for SyslogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<i32> for SyslogLevel {
    fn from(level: i32) -> Self {
        Self::from_raw(level)
    }
}

impl From<SyslogLevel> for i32 {
    fn from(level: SyslogLevel) -> Self {
        level as i32
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExecutionMessage {
    pub message: String,
    pub level: SyslogLevel,
}

impl ExecutionMessage {
    pub fn new(level: SyslogLevel, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level,
        }
    }

    pub fn from_syslog(syslog_level: i32, message: impl Into<String>) -> Self {
        Self::new(SyslogLevel::from_raw(syslog_level), message)
    }

    pub fn is_error(&self) -> bool {
        self.level.is_error_or_worse()
    }

    pub fn is_warning(&self) -> bool {
        matches!(self.level, SyslogLevel::Warning)
    }

    pub fn is_warning_or_worse(&self) -> bool {
        self.level
            .is_warning_or_worse()
    }
}

impl std::fmt::Display for ExecutionMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.level, self.message)
    }
}
