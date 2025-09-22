use eframe::egui::Context;
use parking_lot::Mutex;
use std::sync::LazyLock;

#[macro_export]
macro_rules! info {
    ($($message:tt)*) => {
        $crate::logging::log(std::fmt::format(format_args!($($message)*)), $crate::logging::LogLevel::Info)
    };
}

#[macro_export]
macro_rules! verbose {
    ($($message:tt)*) => {
        $crate::logging::log(std::fmt::format(format_args!($($message)*)), $crate::logging::LogLevel::Verbose)
    };
}

#[macro_export]
macro_rules! debug {
    ($($message:tt)*) => {
        $crate::logging::log(std::fmt::format(format_args!($($message)*)), $crate::logging::LogLevel::Debug)
    };
}

#[macro_export]
macro_rules! warn {
    ($($message:tt)*) => {
        $crate::logging::log(std::fmt::format(format_args!($($message)*)), $crate::logging::LogLevel::Warn)
    };
}

#[macro_export]
macro_rules! error {
    ($($message:tt)*) => {
        $crate::logging::log(std::fmt::format(format_args!($($message)*)), $crate::logging::LogLevel::Error)
    };
}

pub enum LogLevel {
    Info,
    Verbose,
    Debug,
    Warn,
    Error,
}

pub fn log<T: Into<String>>(message: T, level: LogLevel) {
    let log_message = message.into();
    let mut logger = LOGGER.lock();

    let level_string = match &level {
        LogLevel::Info => "INFO",
        LogLevel::Verbose => "VERBOSE",
        LogLevel::Debug => "DEBUG",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    };

    logger.logs.push((format!("[{level_string}] {log_message}"), level));

    if let Some(ctx) = &logger.ui {
        ctx.request_repaint();
    }
}

pub fn set_ui_context(ctx: Context) {
    let mut logger = LOGGER.lock();
    logger.ui = Some(ctx);
}

pub struct LoggingData {
    pub allow_verbose: bool,
    pub allow_debug: bool,
    pub logs: Vec<(String, LogLevel)>,
    pub ui: Option<Context>,
}

pub static LOGGER: LazyLock<Mutex<LoggingData>> = LazyLock::new(|| {
    Mutex::new(LoggingData {
        allow_verbose: true,
        allow_debug: true,
        logs: Vec::new(),
        ui: None,
    })
});
