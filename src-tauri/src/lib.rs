mod commands;
mod detect;
mod error;
mod model;
mod patch;
mod vdf;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect,
            commands::inspect_path,
            commands::compute,
            commands::plan,
            commands::patch,
            commands::revert,
            commands::is_running,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Log panics to stderr and `%LOCALAPPDATA%\kh3-ultrawide-patcher\panic.log` so a crash
/// (which `panic = "abort"` would otherwise make silent) leaves a breadcrumb.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("[kh3-ultrawide-patcher] {info}\n");
        eprint!("{msg}");
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            let dir = std::path::Path::new(&base).join("kh3-ultrawide-patcher");
            let _ = std::fs::create_dir_all(&dir);
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("panic.log"))
            {
                let _ = f.write_all(msg.as_bytes());
            }
        }
    }));
}
