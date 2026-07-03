use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = write_panic_report(info) {
            eprintln!("Leetcode crash report written to {}", path.display());
        }
        previous(info);
    }));
}

pub fn crash_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("leetcode").join("crashes"))
}

fn write_panic_report(info: &std::panic::PanicHookInfo<'_>) -> Option<PathBuf> {
    let dir = crash_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let timestamp = unix_timestamp();
    let path = dir.join(format!("panic-{timestamp}-{}.log", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .ok()?;

    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|value| (*value).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_string());
    let location = info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "unknown location".to_string());
    let thread_name = std::thread::current()
        .name()
        .map(ToString::to_string)
        .unwrap_or_else(|| "unnamed".to_string());
    let executable = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unavailable: {err}"));
    let current_dir = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unavailable: {err}"));
    let backtrace = Backtrace::force_capture();

    let _ = writeln!(file, "app=Leetcode");
    let _ = writeln!(file, "version={}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(file, "timestamp={timestamp}");
    let _ = writeln!(file, "pid={}", std::process::id());
    let _ = writeln!(file, "thread={thread_name}");
    let _ = writeln!(file, "location={location}");
    let _ = writeln!(file, "executable={executable}");
    let _ = writeln!(file, "current_dir={current_dir}");
    let _ = writeln!(file);
    let _ = writeln!(file, "panic:");
    let _ = writeln!(file, "{payload}");
    let _ = writeln!(file);
    let _ = writeln!(file, "backtrace:");
    let _ = writeln!(file, "{backtrace}");
    Some(path)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
