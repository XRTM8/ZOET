mod server;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 1. Force maximum GPU hardware acceleration and video decoding on Windows
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--enable-gpu-rasterization --enable-zero-copy --ignore-gpu-blocklist --enable-hardware-overlays --enable-features=VaapiVideoDecoder,PlatformHEVCDecoderSupport,WebRTC-H264WithOpenH264FFmpeg --disable-software-rasterizer"
    );

    let port = server::start_embedded_server()
        .expect("failed to start ZOET embedded server");

    tauri::Builder::default()
        .setup(move |app| {
            let target_url = format!("http://127.0.0.1:{port}/index.html");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.maximize();
                let _ = window.navigate(target_url.parse().expect("valid URL"));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ZOET desktop application");
}
