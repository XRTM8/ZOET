mod server;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let port = server::start_embedded_server()
        .expect("failed to start ZOET embedded server");

    tauri::Builder::default()
        .setup(move |app| {
            let target_url = format!("http://127.0.0.1:{port}/index.html");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.navigate(target_url.parse().expect("valid URL"));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ZOET desktop application");
}
