use std::io::Write;

pub fn record(message: impl std::fmt::Display) {
    let dir = crate::model::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("gallery.log"))
    {
        let _ = writeln!(
            file,
            "{:?} pid={} {message}",
            std::time::SystemTime::now(),
            std::process::id()
        );
    }
}
pub fn init() {
    std::panic::set_hook(Box::new(|info| {
        let message = format!(
            "panic: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        record(&message);
        eprintln!("{message}");
    }));
}
