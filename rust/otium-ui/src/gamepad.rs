use crate::{app, dom::*, player};
use gloo_timers::callback::Interval;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use web_sys::{Gamepad, GamepadButton};

struct GamepadState {
    prev_buttons: Vec<bool>,
    last_repeat: [f64; 20],
    last_axis_time: f64,
    connected_count: usize,
    _interval: Option<Interval>,
}

impl Default for GamepadState {
    fn default() -> Self {
        Self {
            prev_buttons: vec![false; 20],
            last_repeat: [0.0; 20],
            last_axis_time: 0.0,
            connected_count: 0,
            _interval: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<GamepadState> = RefCell::new(GamepadState::default());
}

fn now() -> f64 {
    js_sys::Date::now()
}

pub fn init() {
    let timer = Interval::new(45, poll_gamepads);
    STATE.with(|s| s.borrow_mut()._interval = Some(timer));
}

fn poll_gamepads() {
    let Ok(pads_array) = window().navigator().get_gamepads() else {
        return;
    };
    let mut active_pad: Option<Gamepad> = None;
    let mut total_connected = 0usize;

    for i in 0..pads_array.length() {
        let val = pads_array.get(i);
        if val.is_null() || val.is_undefined() {
            continue;
        }
        if let Ok(pad) = val.dyn_into::<Gamepad>() {
            if pad.connected() {
                total_connected += 1;
                if active_pad.is_none() {
                    active_pad = Some(pad);
                }
            }
        }
    }

    let time = now();

    STATE.with(|s| {
        let mut st = s.borrow_mut();

        // Detect connection transition
        if total_connected > 0 && st.connected_count == 0 {
            if player::is_open() {
                player::pill("تم توصيل يد التحكم");
            } else {
                app::notify("تم توصيل يد التحكم");
            }
        } else if total_connected == 0 && st.connected_count > 0 {
            if player::is_open() {
                player::pill("تم فصل يد التحكم");
            } else {
                app::notify("تم فصل يد التحكم");
            }
        }
        st.connected_count = total_connected;

        let Some(pad) = active_pad else {
            for b in st.prev_buttons.iter_mut() {
                *b = false;
            }
            return;
        };

        let buttons = pad.buttons();
        let btn_len = (buttons.length() as usize).min(20);

        for b in 0..btn_len {
            let is_pressed = if let Ok(btn_obj) = buttons.get(b as u32).dyn_into::<GamepadButton>() {
                btn_obj.pressed()
            } else {
                false
            };

            let was_pressed = st.prev_buttons[b];
            let just_pressed = is_pressed && !was_pressed;
            let is_nav_button = (12..=15).contains(&b);
            let repeat = is_pressed && was_pressed && is_nav_button && (time - st.last_repeat[b] > 180.0);

            if just_pressed || repeat {
                if is_nav_button {
                    st.last_repeat[b] = time;
                }
                handle_button(b, just_pressed);
            }

            st.prev_buttons[b] = is_pressed;
        }

        // Handle Left Analog Stick navigation
        let axes = pad.axes();
        if axes.length() >= 2 && (time - st.last_axis_time > 180.0) {
            let axis_x = axes.get(0).as_f64().unwrap_or(0.0);
            let axis_y = axes.get(1).as_f64().unwrap_or(0.0);

            let mut handled = true;
            if axis_y < -0.55 {
                if player::is_open() && !app::is_sheet_open() {
                    player::remote_arrow("arrowup");
                } else {
                    app::remote_focus_key("ArrowUp");
                }
            } else if axis_y > 0.55 {
                if player::is_open() && !app::is_sheet_open() {
                    player::remote_arrow("arrowdown");
                } else {
                    app::remote_focus_key("ArrowDown");
                }
            } else if axis_x < -0.55 {
                if player::is_open() && !app::is_sheet_open() {
                    player::seek(-5.0);
                } else {
                    app::remote_focus_key("ArrowLeft");
                }
            } else if axis_x > 0.55 {
                if player::is_open() && !app::is_sheet_open() {
                    player::seek(5.0);
                } else {
                    app::remote_focus_key("ArrowRight");
                }
            } else {
                handled = false;
            }

            if handled {
                st.last_axis_time = time;
            }
        }
    });
}

fn handle_button(btn: usize, just_pressed: bool) {
    match btn {
        // Button 0: A / Cross
        0 => {
            if !just_pressed { return; }
            if app::is_sheet_open() {
                if let Some(active) = document().active_element().and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) {
                    active.click();
                }
            } else if player::is_open() {
                player::toggle();
            } else if let Some(active) = document().active_element().and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) {
                active.click();
            } else {
                app::remote_focus_key("ArrowDown");
            }
        }

        // Button 1: B / Circle
        1 => {
            if !just_pressed { return; }
            if app::is_sheet_open() {
                app::close_sheet();
            } else if app::is_search_open() {
                app::search_close();
            } else if player::is_open() {
                player::close();
            }
        }

        // Button 2: X / Square
        2 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::toggle_subtitles();
            } else {
                app::search_open();
            }
        }

        // Button 3: Y / Triangle
        3 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::cycle_video_filter();
            } else {
                app::cycle_view(true);
            }
        }

        // Button 4: LB / L1
        4 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::seek(-10.0);
            } else {
                app::cycle_view(false);
            }
        }

        // Button 5: RB / R1
        5 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::seek(10.0);
            } else {
                app::cycle_view(true);
            }
        }

        // Button 6: LT / L2
        6 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::seek(-30.0);
            }
        }

        // Button 7: RT / R2
        7 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::seek(30.0);
            }
        }

        // Button 8: View / Select / Back
        8 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::toggle_theater();
            }
        }

        // Button 9: Menu / Start
        9 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::controls_help();
            } else {
                app::open_controls_guide();
            }
        }

        // Button 10: Left stick click (L3)
        10 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::fit();
            }
        }

        // Button 11: Right stick click (R3)
        11 => {
            if !just_pressed { return; }
            if player::is_open() {
                player::toggle_mute();
            }
        }

        // Button 12: D-pad Up
        12 => {
            if player::is_open() && !app::is_sheet_open() {
                player::remote_arrow("arrowup");
            } else {
                app::remote_focus_key("ArrowUp");
            }
        }

        // Button 13: D-pad Down
        13 => {
            if player::is_open() && !app::is_sheet_open() {
                player::remote_arrow("arrowdown");
            } else {
                app::remote_focus_key("ArrowDown");
            }
        }

        // Button 14: D-pad Left
        14 => {
            if player::is_open() && !app::is_sheet_open() {
                player::seek(-5.0);
            } else {
                app::remote_focus_key("ArrowLeft");
            }
        }

        // Button 15: D-pad Right
        15 => {
            if player::is_open() && !app::is_sheet_open() {
                player::seek(5.0);
            } else {
                app::remote_focus_key("ArrowRight");
            }
        }

        _ => {}
    }
}
