mod api;
mod app;
mod catalogue;
mod dom;
mod gamepad;
mod player;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&info.to_string()));
        if let Some(node) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("bootStatus"))
        {
            node.set_text_content(Some("تعذّر تشغيل الواجهة. أعد فتح التطبيق."));
        }
    }));
    app::init();
    player::init();
    gamepad::init();
}
