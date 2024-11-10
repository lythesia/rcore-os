use core::fmt;

use log::{Level, LevelFilter, Log};

struct NaiveLogger;

impl Log for NaiveLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        print_color(
            format_args!(
                "[{} {}:{}] {}\n",
                record.level(),
                record.file().unwrap_or("??.rs"),
                record.line().unwrap_or(0),
                record.args()
            ),
            color_code_for_level(&record.level()),
        );
    }

    fn flush(&self) {}
}

fn color_code_for_level(lvl: &Level) -> u8 {
    match lvl {
        Level::Error => 31,
        Level::Warn => 93,
        Level::Info => 34,
        Level::Debug => 32,
        Level::Trace => 90,
    }
}

fn print_color(args: fmt::Arguments, color_code: u8) {
    crate::print!("\u{1b}[{}m{}\u{1b}[0m", color_code, args);
}

pub fn init() {
    static LOGGER: NaiveLogger = NaiveLogger;
    log::set_logger(&LOGGER).expect("set_logger");
    log::set_max_level(match option_env!("LOG") {
        Some("error") | Some("ERROR") => LevelFilter::Error,
        Some("warn") | Some("WARN") => LevelFilter::Warn,
        Some("info") | Some("INFO") => LevelFilter::Info,
        Some("debug") | Some("DEBUG") => LevelFilter::Debug,
        Some("trace") | Some("TRACE") => LevelFilter::Trace,
        _ => LevelFilter::Off,
    });
}
