//! The playback session is independent from catalogue requests and sheets.
//! A replacement source is resolved before media is touched; failed replacement
//! attempts restore the last working episode and playback settings.
use crate::{api, app, catalogue, dom::*, icons};
use gloo_timers::callback::{Interval, Timeout};
use js_sys::{Array, Function, Object, Promise, Reflect};
use serde_json::{json, Value};
use std::cell::RefCell;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    AbortController, CanvasRenderingContext2d, DragEvent, Element, Event,
    FileReader, HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlVideoElement,
    KeyboardEvent, PointerEvent, TextTrack, TextTrackCue, TextTrackKind, TextTrackMode,
};

#[derive(Clone, Debug)]
pub struct PlaybackSnapshot {
    pub time: f64,
    pub paused: bool,
    pub rate: f64,
    pub volume: f64,
    pub muted: bool,
}
impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            time: 0.,
            paused: false,
            rate: 1.,
            volume: 1.,
            muted: false,
        }
    }
}
#[derive(Clone)]
struct Restore {
    episode: Value,
    series: Value,
    episodes: Vec<Value>,
    settings: PlaybackSnapshot,
    stream_index: usize,
    embedded: Option<Value>,
}
struct HlsSession {
    instance: JsValue,
    _handlers: Vec<Closure<dyn FnMut(JsValue, JsValue)>>,
}
#[derive(Clone, Default)]
struct Gesture {
    x: f64,
    y: f64,
    at: f64,
    moved: bool,
    hold: bool,
    previous_rate: f64,
}
struct Player {
    session: u64,
    attempt: u64,
    active_url: String,
    episode: Option<Value>,
    series: Value,
    episodes: Vec<Value>,
    controller: Option<AbortController>,
    switching: Option<AbortController>,
    switch_generation: u64,
    queue: Vec<usize>,
    position: usize,
    stream_index: usize,
    settings: PlaybackSnapshot,
    started: f64,
    advanced: f64,
    last_time: f64,
    last_buffer: f64,
    ready: bool,
    metadata: bool,
    refreshed: bool,
    rollback: Option<Restore>,
    bookmark: Option<PlaybackSnapshot>,
    embedded: Option<Value>,
    hls: Option<HlsSession>,
    watchdog: Option<Interval>,
    hud: Option<Timeout>,
    pill: Option<Timeout>,
    hold: Option<Timeout>,
    tap_timer: Option<Timeout>,
    embed_timer: Option<Timeout>,
    unlock_timer: Option<Timeout>,
    gesture: Gesture,
    last_tap: f64,
    embed_handler: Option<Closure<dyn FnMut(Event)>>,
    last_save: f64,
    next_deadline: Option<f64>,
    next_cancelled: bool,
    season: String,
    sub_offset: f64,
    locked: bool,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            session: 0,
            attempt: 0,
            active_url: String::new(),
            episode: None,
            series: Value::Null,
            episodes: vec![],
            controller: None,
            switching: None,
            switch_generation: 0,
            queue: vec![],
            position: 0,
            stream_index: 0,
            settings: PlaybackSnapshot::default(),
            started: 0.,
            advanced: 0.,
            last_time: 0.,
            last_buffer: 0.,
            ready: false,
            metadata: false,
            refreshed: false,
            rollback: None,
            bookmark: None,
            embedded: None,
            hls: None,
            watchdog: None,
            hud: None,
            pill: None,
            hold: None,
            tap_timer: None,
            embed_timer: None,
            unlock_timer: None,
            embed_handler: None,
            gesture: Gesture::default(),
            last_tap: 0.,
            last_save: 0.,
            next_deadline: None,
            next_cancelled: false,
            season: "1".into(),
            sub_offset: 0.,
            locked: false,
        }
    }
}
thread_local! { static STATE:RefCell<Player> = RefCell::new(Player::default()); }
fn video() -> HtmlVideoElement {
    el("nativePlayer").unchecked_into()
}
fn now() -> f64 {
    js_sys::Date::now()
}
fn rows(v: &Value, key: &str) -> Vec<Value> {
    v[key].as_array().cloned().unwrap_or_default()
}
fn style(id: &str, key: &str, value: &str) {
    let _ = el(id)
        .unchecked_into::<HtmlElement>()
        .style()
        .set_property(key, value);
}
fn class(id: &str, name: &str, enabled: bool) {
    let _ = el(id).class_list().toggle_with_force(name, enabled);
}
fn call(target: &JsValue, name: &str, args: &Array) -> Result<JsValue, JsValue> {
    let f = Reflect::get(target, &JsValue::from_str(name))?.dyn_into::<Function>()?;
    f.apply(target, args)
}
fn args(values: &[JsValue]) -> Array {
    values.iter().cloned().collect()
}
fn bridge(method: &str, value: JsValue) {
    if let Ok(target) = Reflect::get(window().as_ref(), &"KrmziBridge".into()) {
        let _ = call(&target, method, &args(&[value]));
    }
}
fn bridge_metadata(title: &str, series: &str) {
    if let Ok(target) = Reflect::get(window().as_ref(), &"KrmziBridge".into()) {
        let _ = call(&target, "updateMediaMetadata", &args(&[title.into(), series.into()]));
    }
}
fn source_name(source: &str) -> &str {
    if source == "cinemana" {
        "سينمانا"
    } else {
        "قرمزي"
    }
}
fn current(session: u64, attempt: u64) -> bool {
    STATE.with(|c| {
        let p = c.borrow();
        p.session == session && p.attempt == attempt
    }) && is_open()
}
pub fn is_open() -> bool {
    !el("mobilePlayer").class_list().contains("hidden")
}
pub fn current_episode() -> Option<Value> {
    STATE.with(|c| c.borrow().episode.clone())
}
fn episode_identity() -> Option<Value> {
    if let Some(episode) = current_episode() {
        return Some(episode);
    }
    let url = STATE.with(|c| c.borrow().active_url.clone());
    if url.is_empty() {
        return None;
    }
    let mut context = app::episode_context(&url);
    if !context.is_object() {
        context = json!({});
    }
    context["url"] = Value::String(url);
    Some(context)
}
pub fn snapshot() -> PlaybackSnapshot {
    let v = video();
    let bookmark = STATE.with(|c| {
        let p = c.borrow();
        p.embedded.as_ref().and(p.bookmark.clone())
    });
    bookmark.unwrap_or(PlaybackSnapshot {
        time: v.current_time().max(0.),
        paused: v.paused(),
        rate: v.playback_rate(),
        volume: v.volume(),
        muted: v.muted(),
    })
}
fn restore_point() -> Option<Restore> {
    let settings = snapshot();
    STATE.with(|c| {
        let p = c.borrow();
        Some(Restore {
            episode: p.episode.clone()?,
            series: p.series.clone(),
            episodes: p.episodes.clone(),
            settings,
            stream_index: p.stream_index,
            embedded: p.embedded.clone(),
        })
    })
}
fn cancel_switch() {
    let (controller, pending_frame) = STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.switch_generation += 1;
        let pending = p.embed_timer.take().is_some();
        (p.switching.take(), pending)
    });
    if let Some(controller) = controller {
        controller.abort();
    }
    if pending_frame {
        let frame = el("embedPlayer");
        let _ = Reflect::set(frame.as_ref(), &"onload".into(), &JsValue::NULL);
        let _ = Reflect::set(frame.as_ref(), &"onerror".into(), &JsValue::NULL);
        let _ = frame.remove_attribute("src");
        show("embedPlayer", false);
        STATE.with(|c| c.borrow_mut().embed_handler = None);
    }
}
pub fn sheet_closed() {
    cancel_switch();
}
fn stop_hls() {
    let previous = STATE.with(|c| c.borrow_mut().hls.take());
    if let Some(previous) = previous {
        let _ = call(&previous.instance, "destroy", &Array::new());
    }
}
fn release_media() {
    stop_hls();
    let v = video();
    let _ = v.pause();
    let _ = v.remove_attribute("src");
    let _ = v.remove_attribute("poster");
    v.load();
    let frame = el("embedPlayer");
    let _ = Reflect::set(frame.as_ref(), &"onload".into(), &JsValue::NULL);
    let _ = Reflect::set(frame.as_ref(), &"onerror".into(), &JsValue::NULL);
    STATE.with(|c| c.borrow_mut().embed_handler = None);
    show("nativePlayer", false);
    show("embedPlayer", false);
    let _ = frame.remove_attribute("src");
    class("mobilePlayer", "embed-mode", false);
    class("mobilePlayer", "video-playing", false);
}
fn loading(message: &str) {
    html("playerSpinner",&format!("<div class=\"spinner-ring\"></div><p class=\"player-load-status\" role=\"status\">{}</p>",esc(message)));
    class("mobilePlayer", "player-loading", true);
    class("mobilePlayer", "player-error", false);
    show("playerSpinner", true);
}
fn error(message: &str) {
    html("playerSpinner",&format!("<p class=\"player-message\" role=\"status\">{}</p><div class=\"player-retry-actions\"><button class=\"player-retry-btn\" data-player-action=\"retry\">إعادة المحاولة</button><button class=\"player-retry-btn\" data-player-action=\"sources\">المصادر والسيرفرات</button></div>",esc(message)));
    class("mobilePlayer", "player-loading", true);
    class("mobilePlayer", "player-error", true);
    show("playerSpinner", true);
    show_hud(false);
}
fn art(item: &Value) -> String {
    for key in ["artwork", "poster", "thumbnail"] {
        let value = s(item, key);
        if !value.is_empty() {
            return value;
        }
    }
    String::new()
}
fn backdrop(url: &str) {
    style(
        "playerBackdrop",
        "background-image",
        &if url.is_empty() {
            "none".into()
        } else {
            format!("url({})", serde_json::to_string(url).unwrap_or_default())
        },
    );
}
fn title_ui(episode: &Value) {
    let ep_title = s(episode, "title");
    let series_title = s(episode, "seriesTitle");
    let main_title = if !series_title.is_empty() {
        catalogue::clean_title(&series_title)
    } else {
        catalogue::clean_title(&ep_title)
    };
    let season_str = catalogue::season(episode);
    let ep_num_str = catalogue::episode_number(episode);
    let season_val = season_str.parse::<usize>().unwrap_or(1);
    let ed = catalogue::edition(episode);
    let ed_tag = if ed == "dubbed" { " · مدبلج" } else { " · مترجم" };

    let sub_title = if !ep_num_str.is_empty() {
        if season_val > 1 {
            format!("الموسم {} · الحلقة {}{}", season_val, ep_num_str, ed_tag)
        } else {
            format!("الحلقة {}{}", ep_num_str, ed_tag)
        }
    } else {
        let clean_ep = catalogue::clean_title(&ep_title);
        if clean_ep != main_title && !clean_ep.is_empty() {
            let stripped = clean_ep
                .strip_prefix(&main_title)
                .map(|s| s.trim_start_matches([' ', '-', '·', ':']))
                .unwrap_or(&clean_ep)
                .trim();
            if !stripped.is_empty() {
                format!("{}{}", stripped, ed_tag)
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    text("hudEpTitle", &main_title);
    if sub_title.is_empty() {
        text("hudSeriesTitle", "");
        show("hudSeriesTitle", false);
    } else {
        text("hudSeriesTitle", &sub_title);
        show("hudSeriesTitle", true);
    }
    let full_meta = format!("{} - {}", main_title, sub_title);
    bridge_metadata(
        if sub_title.is_empty() {
            &main_title
        } else {
            &full_meta
        },
        &main_title,
    );
    html(
        "playerFavBtn",
        if app::is_favorite(episode) {
            icons::HEART_SOLID
        } else {
            icons::HEART_OUTLINE
        },
    );
    let _ = el("playerFavBtn").set_attribute(
        "aria-pressed",
        if app::is_favorite(episode) {
            "true"
        } else {
            "false"
        },
    );
}
fn subtitles(episode: &Value) {
    let v = video();
    let tracks = v.query_selector_all("track").unwrap();
    for i in 0..tracks.length() {
        if let Some(n) = tracks.item(i) {
            let _ = v.remove_child(&n);
        }
    }
    let tracks = rows(episode, "subtitles");
    show("toggleSubtitlesBtn", !tracks.is_empty());
    for track in tracks {
        if let Ok(node) = document().create_element("track") {
            let _ = node.set_attribute("kind", "subtitles");
            let _ = node.set_attribute("label", &s(&track, "label"));
            let _ = node.set_attribute("srclang", &s(&track, "lang"));
            let url = format!(
                "/api/cinemana/subtitle?url={}",
                js_sys::encode_uri_component(&s(&track, "url"))
            );
            let _ = node.set_attribute("src", &url);
            if s(&track, "lang") == "ar" {
                let _ = node.set_attribute("default", "");
            }
            let _ = v.append_child(&node);
        }
    }
    sync_tool_states();
}
pub fn open_episode(url: String, resume: Option<PlaybackSnapshot>) {
    open(url, resume, false);
}
fn open(url: String, resume: Option<PlaybackSnapshot>, fresh: bool) {
    if url.is_empty() {
        return;
    }
    save_history(true);
    cancel_switch();
    let context = app::episode_context(&url);
    let resume = resume.unwrap_or_else(|| {
        let time = context["currentTime"]
            .as_f64()
            .filter(|t| *t > 10.)
            .unwrap_or(0.);
        PlaybackSnapshot {
            time,
            volume: video().volume(),
            muted: video().muted(),
            ..Default::default()
        }
    });
    let controller = AbortController::new().ok();
    let session = STATE.with(|c| {
        let mut p = c.borrow_mut();
        if let Some(previous) = p.controller.take() {
            previous.abort();
        }
        p.session += 1;
        p.attempt += 1;
        p.active_url = url.clone();
        p.controller = controller.clone();
        p.episode = None;
        p.episodes.clear();
        p.queue.clear();
        p.series = Value::Null;
        p.rollback = None;
        p.bookmark = None;
        p.embedded = None;
        p.refreshed = fresh;
        p.settings = resume.clone();
        p.next_deadline = None;
        p.next_cancelled = false;
        p.embed_timer = None;
        p.sub_offset = 0.;
        p.locked = false;
        p.unlock_timer = None;
        p.session
    });
    class("mobilePlayer", "touch-locked", false);
    show("unlockTouchBtn", false);
    release_media();
    el("appFrame").set_scroll_top(0);
    show("mobilePlayer", true);
    sync_tool_states();
    show("autoNextCard", false);
    show("skipIntroBtn", false);
    html("drawerEpisodesCarousel", "");
    text("drawerCount", "جاري جلب الحلقات…");
    backdrop(&art(&context));
    let ep_title = s(&context, "title");
    let series_title = s(&context, "seriesTitle");
    text("hudEpTitle", &ep_title);
    text("hudSeriesTitle", &series_title);
    text("activeServerLabel", "المصدر");
    text("activeQualityLabel", "الجودة");
    bridge_metadata(&ep_title, &series_title);
    loading("جاري تجهيز الحلقة…");
    show_hud(false);
    spawn_local(async move {
        let mut params = vec![("url", url.clone())];
        if fresh {
            params.push(("fresh", "1".into()));
        }
        let signal = controller.as_ref().map(|c| c.signal());
        let result =
            api::call_with_signal("/api/episode/detail", &params, None, signal.as_ref()).await;
        if STATE.with(|c| c.borrow().session != session)
            || signal.as_ref().is_some_and(|s| s.aborted())
        {
            return;
        }
        match result {
            Ok(mut episode) if episode["error"].is_null() && episode["isSuccess"] != false => {
                episode["artwork"] = Value::String(art(&context));
                STATE.with(|c| {
                    let mut p = c.borrow_mut();
                    p.episode = Some(episode.clone());
                    p.controller = None;
                });
                title_ui(&episode);
                subtitles(&episode);
                backdrop(&s(&episode, "artwork"));
                if !rows(&episode, "directStreams").is_empty() {
                    play_direct(None, resume, false);
                } else if let Some(server) = rows(&episode, "servers").iter().find(|s| s["name"].as_str().map(|n| n.eq_ignore_ascii_case("ok")).unwrap_or(false)) {
                    play_embed(server.clone());
                } else {
                    error("ما توفرت روابط بث مباشر لهذه الحلقة. جرّب مصدراً أو سيرفراً آخر.");
                }
                let is_movie = s(&context, "mediaType") == "movie"
                    || s(&context, "type") == "movie"
                    || s(&episode, "mediaType") == "movie"
                    || s(&episode, "type") == "movie"
                    || s(&episode, "seriesUrl").is_empty();
                let series_url = s(&episode, "seriesUrl");
                if !is_movie && !series_url.is_empty() {
                    load_drawer(series_url, session);
                } else {
                    text("drawerCount", "فيلم");
                    show("drawerEpisodeTools", false);
                    html(
                        "drawerEpisodesCarousel",
                        "<p class=\"empty-note\">أنت تشاهد فيلماً.</p>",
                    );
                }
            }
            _ => error(if catalogue::provider(&json!({"url":url})) == "cinemana" {
                "سينمانا غير متاح على اتصالك. اختر قرمزي من المصادر أو أعد المحاولة."
            } else {
                "تعذر تجهيز الحلقة. أعد المحاولة أو اختر سيرفراً آخر."
            }),
        }
    });
}
fn initial_index(streams: &[Value]) -> usize {
    if storage("krmzi_quality_mode").as_deref() == Some("manual") {
        if let Some(pref) = storage("krmzi_pref_quality") {
            let target_h = pref
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);
            if target_h > 0 {
                if let Some(index) = streams
                    .iter()
                    .position(|s| quality_height(s) == target_h && s["type"] != "hls")
                {
                    return index;
                }
            }
            if let Some(index) = streams
                .iter()
                .position(|stream| s(stream, "quality") == pref && stream["type"] != "hls")
            {
                return index;
            }
        }
    }
    if let Some(index) = streams.iter().position(|s| s["type"] == "hls") {
        return index;
    }
    streams
        .iter()
        .enumerate()
        .filter(|(_, s)| quality_height(s) <= 720 && quality_height(s) > 0)
        .max_by_key(|(_, s)| quality_height(s))
        .map(|(i, _)| i)
        .unwrap_or(0)
}
fn quality_height(stream: &Value) -> u32 {
    s(stream, "quality")
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}
fn play_direct(index: Option<usize>, settings: PlaybackSnapshot, transaction: bool) {
    let streams = current_episode()
        .map(|e| rows(&e, "directStreams"))
        .unwrap_or_default();
    if streams.is_empty() {
        return fail_final();
    }
    let index = index
        .unwrap_or_else(|| initial_index(&streams))
        .min(streams.len() - 1);
    let old = if transaction { restore_point() } else { None };
    let mut rest: Vec<usize> = (0..streams.len())
        .filter(|i| *i != index && streams[*i]["type"] != "hls")
        .collect();
    let height = quality_height(&streams[index]);
    rest.sort_by_key(|i| {
        (
            quality_height(&streams[*i]) > height,
            std::cmp::Reverse(quality_height(&streams[*i])),
        )
    });
    let mut queue = vec![index];
    queue.extend(rest);
    queue.truncate(3);
    STATE.with(|c| {
        let mut p = c.borrow_mut();
        if transaction && p.rollback.is_none() {
            p.rollback = old;
        }
        p.settings = settings;
        p.queue = queue;
        p.position = 0;
        p.embedded = None;
        p.embed_timer = None;
    });
    start_attempt();
}
fn media_url(stream: &Value) -> String {
    let url = s(stream, "url");
    if stream["type"] == "hls" && stream["provider"] != "cinemana" {
        format!("/api/media?url={}", js_sys::encode_uri_component(&url))
    } else {
        url
    }
}
fn start_attempt() {
    stop_hls();
    let selected = STATE.with(|c| {
        let mut p = c.borrow_mut();
        let streams = rows(p.episode.as_ref()?, "directStreams");
        let index = *p.queue.get(p.position)?;
        let selected = streams.get(index)?.clone();
        p.stream_index = index;
        p.attempt += 1;
        p.started = now();
        p.advanced = p.started;
        p.last_time = p.settings.time;
        p.last_buffer = 0.;
        p.ready = false;
        p.metadata = false;
        Some((
            selected,
            p.settings.clone(),
            p.session,
            p.attempt,
            p.position,
        ))
    });
    let Some((selected, settings, session, attempt, position)) = selected else {
        return fail_final();
    };
    let v = video();
    let _ = v.pause();
    show("embedPlayer", false);
    let _ = el("embedPlayer").remove_attribute("src");
    show("nativePlayer", false);
    class("mobilePlayer", "embed-mode", false);
    class("mobilePlayer", "video-playing", false);
    show("autoNextCard", false);
    loading(if position > 0 {
        "جاري تجربة رابط بديل…"
    } else {
        "جاري تحميل الفيديو…"
    });
    let source = current_episode()
        .map(|ep| catalogue::provider(&ep).to_string())
        .unwrap_or_default();
    text("activeServerLabel", source_name(&source));
    let height = quality_height(&selected);
    let quality = if selected["type"] != "hls" && height > 0 {
        format!("{height}p")
    } else {
        "الجودة".into()
    };
    text("activeQualityLabel", &quality);
    show("skipIntroBtn", false);
    v.set_preload("auto");
    v.set_volume(settings.volume.clamp(0., 1.));
    v.set_muted(settings.muted);
    v.set_playback_rate(settings.rate.clamp(0.5, 2.));
    let hls = selected["type"] == "hls" && install_hls(&media_url(&selected), session, attempt);
    if !hls {
        v.set_src(&media_url(&selected));
        v.load();
    }
    if !settings.paused {
        play_video();
    }
    show_hud(!settings.paused);
}
fn install_hls(url: &str, session: u64, attempt: u64) -> bool {
    let Ok(constructor) = Reflect::get(window().as_ref(), &"Hls".into()) else {
        return false;
    };
    if call(&constructor, "isSupported", &Array::new())
        .ok()
        .and_then(|v| v.as_bool())
        != Some(true)
    {
        return false;
    }
    let Ok(constructor_fn) = constructor.clone().dyn_into::<Function>() else {
        return false;
    };
    let is_tv = document()
        .document_element()
        .and_then(|e| e.get_attribute("data-display"))
        .as_deref()
        == Some("tv");
    let max_buf = if is_tv { 60. } else { 35. };
    let back_buf = if is_tv { 30. } else { 20. };
    let config = Object::new();
    for (key, value) in [
        ("enableWorker", if is_tv { JsValue::FALSE } else { JsValue::TRUE }),
        ("autoStartLoad", JsValue::FALSE),
        ("maxBufferLength", max_buf.into()),
        ("maxMaxBufferLength", 120.into()),
        ("backBufferLength", back_buf.into()),
        ("fragLoadingTimeOut", 20000.into()),
        ("manifestLoadingTimeOut", 15000.into()),
        ("fragLoadingMaxRetry", 6.into()),
        ("levelLoadingMaxRetry", 6.into()),
    ] {
        let _ = Reflect::set(&config, &key.into(), &value);
    }
    let Ok(engine) = Reflect::construct(&constructor_fn, &args(&[config.into()])) else {
        return false;
    };
    let events = Reflect::get(&constructor, &"Events".into()).unwrap_or(JsValue::UNDEFINED);
    let mut handlers = vec![];
    for name in ["MANIFEST_PARSED", "ERROR"] {
        let name = name.to_string();
        let instance = engine.clone();
        let event = Reflect::get(&events, &JsValue::from_str(&name))
            .unwrap_or_else(|_| JsValue::from_str(&name));
        let callback = Closure::wrap(Box::new(move |_event: JsValue, data: JsValue| {
            if !current(session, attempt) {
                return;
            }
            match name.as_str() {
                "ERROR" => {
                    if Reflect::get(&data, &"fatal".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        == Some(true)
                    {
                        schedule_failure(session, attempt);
                    }
                }
                "MANIFEST_PARSED" => {
                    let levels = Reflect::get(&instance, &"levels".into())
                        .map(|v| Array::from(&v))
                        .unwrap_or_default();
                    let manual = storage("krmzi_quality_mode").as_deref() == Some("manual");
                    let target = if manual {
                        storage("krmzi_pref_quality")
                            .unwrap_or_default()
                            .chars()
                            .take_while(char::is_ascii_digit)
                            .collect::<String>()
                            .parse::<u32>()
                            .unwrap_or(720)
                    } else {
                        720
                    };
                    let mut chosen = 0;
                    let mut best = 0;
                    for (i, level) in levels.iter().enumerate() {
                        let height = Reflect::get(&level, &"height".into())
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.) as u32;
                        if height <= target && height >= best {
                            chosen = i;
                            best = height;
                        }
                    }
                    let _ = Reflect::set(&instance, &"startLevel".into(), &(chosen as u32).into());
                    if manual {
                        let chosen_val = (chosen as u32).into();
                        let _ = Reflect::set(&instance, &"currentLevel".into(), &chosen_val);
                        let _ = Reflect::set(&instance, &"loadLevel".into(), &chosen_val);
                        let _ = Reflect::set(&instance, &"nextLevel".into(), &chosen_val);
                    }
                    let position = STATE.with(|c| c.borrow().settings.time);
                    let _ = call(
                        &instance,
                        "startLoad",
                        &args(&[JsValue::from_f64(if position > 0. {
                            position
                        } else {
                            -1.
                        })]),
                    );
                }
                _ => {}
            }
        }) as Box<dyn FnMut(JsValue, JsValue)>);
        let _ = call(&engine, "on", &args(&[event, callback.as_ref().clone()]));
        handlers.push(callback);
    }
    STATE.with(|c| {
        c.borrow_mut().hls = Some(HlsSession {
            instance: engine.clone(),
            _handlers: handlers,
        })
    });
    let _ = call(&engine, "loadSource", &args(&[JsValue::from_str(url)]));
    let _ = call(&engine, "attachMedia", &args(&[video().into()]));
    true
}
fn play_video() {
    let v = video();
    let Ok(promise) = v.play() else {
        return;
    };
    let (session, attempt) = STATE.with(|c| {
        let p = c.borrow();
        (p.session, p.attempt)
    });
    spawn_local(async move {
        if let Err(error) = JsFuture::from(promise).await {
            if !current(session, attempt) {
                return;
            }
            let name = Reflect::get(&error, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            if name == "NotAllowedError" {
                STATE.with(|c| c.borrow_mut().settings.paused = true);
                show("playerSpinner", false);
                class("mobilePlayer", "player-loading", false);
                show_hud(false);
            }
        }
    });
}
fn schedule_failure(session: u64, attempt: u64) {
    Timeout::new(0, move || {
        if current(session, attempt) {
            failed();
        }
    })
    .forget();
}
fn failed() {
    if STATE.with(|c| c.borrow().queue.is_empty()) {
        return;
    }
    let time = video().current_time();
    let next = STATE.with(|c| {
        let mut p = c.borrow_mut();
        if p.ready && time > 0. {
            p.settings.time = time;
        }
        p.position += 1;
        p.position < p.queue.len()
    });
    if next {
        start_attempt();
    } else {
        fail_final();
    }
}
fn fail_final() {
    STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.queue.clear();
        p.attempt += 1;
    });
    stop_hls();
    let restore = STATE.with(|c| c.borrow_mut().rollback.take());
    if let Some(restore) = restore {
        STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.active_url = s(&restore.episode, "url");
            p.episode = Some(restore.episode.clone());
            p.series = restore.series.clone();
            p.episodes = restore.episodes.clone();
            p.refreshed = true;
        });
        title_ui(&restore.episode);
        subtitles(&restore.episode);
        backdrop(&s(&restore.episode, "artwork"));
        if let Some(server) = restore.embedded {
            play_embed(server);
        } else {
            play_direct(Some(restore.stream_index), restore.settings, false);
        }
        render_drawer();
        app::notify("تعذر التبديل، رجعنا للسيرفر السابق");
        return;
    }
    let refresh = STATE.with(|c| {
        let mut p = c.borrow_mut();
        if p.refreshed {
            return None;
        }
        p.refreshed = true;
        Some((p.episode.clone()?, p.settings.clone(), p.session, p.attempt))
    });
    if let Some((episode, mut settings, session, attempt)) = refresh {
        settings.time = settings.time.max(video().current_time());
        loading("جاري تجديد رابط المشاهدة…");
        spawn_local(async move {
            let result = api::call(
                "/api/episode/detail",
                &[("url", s(&episode, "url")), ("fresh", "1".into())],
                None,
            )
            .await;
            if !current(session, attempt) {
                return;
            }
            if let Ok(mut replacement) = result {
                if !rows(&replacement, "directStreams").is_empty() {
                    replacement["artwork"] = episode["artwork"].clone();
                    STATE.with(|c| c.borrow_mut().episode = Some(replacement));
                    play_direct(None, settings, false);
                    return;
                }
            }
            fallback_embed();
        });
    } else {
        fallback_embed();
    }
}
fn fallback_embed() {
    if let Some(server) = current_episode().and_then(|e| {
        rows(&e, "servers")
            .into_iter()
            .find(|s| s["name"].as_str().map(|n| n.eq_ignore_ascii_case("ok")).unwrap_or(false))
    }) {
        play_embed(server);
    } else {
        error("تعذر تشغيل روابط البث المباشر. أعد المحاولة أو اختر مصدراً آخر.");
    }
}
fn play_embed(server: Value) {
    let url = s(&server, "embedUrl");
    if url.is_empty() {
        return;
    }
    cancel_switch();
    let bookmark = snapshot();
    let (session, generation, keep_direct) = STATE.with(|c| {
        let mut p = c.borrow_mut();
        let direct =
            p.embedded.is_none() && p.episode.is_some() && video().get_attribute("src").is_some();
        if direct {
            p.bookmark = Some(bookmark);
        }
        p.embed_timer = None;
        (p.session, p.switch_generation, direct)
    });
    if !keep_direct {
        loading("جاري فتح السيرفر الخارجي…");
    } else {
        pill("جاري فتح السيرفر، المشاهدة مستمرة");
    }
    show("embedPlayer", false);
    let _ = el("embedPlayer").set_attribute("src", &url);
    let frame = el("embedPlayer");
    let loaded = Closure::wrap(Box::new(move |_e: Event| {
        if STATE.with(|c| {
            let p = c.borrow();
            p.session != session || p.switch_generation != generation
        }) {
            return;
        }
        let _ = Reflect::set(el("embedPlayer").as_ref(), &"onload".into(), &JsValue::NULL);
        stop_hls();
        let v = video();
        let _ = v.pause();
        let _ = v.remove_attribute("src");
        v.load();
        show("nativePlayer", false);
        show("embedPlayer", true);
        show("playerSpinner", false);
        class("mobilePlayer", "embed-mode", true);
        class("mobilePlayer", "player-loading", false);
        class("mobilePlayer", "player-error", false);
        STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.attempt += 1;
            p.embedded = Some(server.clone());
            p.embed_timer = None;
            p.rollback = None;
        });
        text("activeServerLabel", &s(&server, "name"));
        text("activeQualityLabel", "من السيرفر");
        pill("السيرفر الخارجي يتحكم بموضع المشاهدة");
        show_hud(false);
    }) as Box<dyn FnMut(Event)>);
    let _ = Reflect::set(frame.as_ref(), &"onload".into(), loaded.as_ref());
    STATE.with(|c| c.borrow_mut().embed_handler = Some(loaded));
    let timer = Timeout::new(15000, move || {
        if STATE.with(|c| {
            let p = c.borrow();
            p.session != session || p.switch_generation != generation || p.embed_timer.is_none()
        }) {
            return;
        }
        let _ = Reflect::set(el("embedPlayer").as_ref(), &"onload".into(), &JsValue::NULL);
        let _ = el("embedPlayer").remove_attribute("src");
        if keep_direct {
            pill("تعذر فتح السيرفر، المشاهدة مستمرة");
        } else {
            error("تعذر فتح السيرفر. اختر سيرفراً آخر.");
        }
    });
    STATE.with(|c| c.borrow_mut().embed_timer = Some(timer));
}
fn load_drawer(url: String, session: u64) {
    spawn_local(async move {
        if let Ok(series) = api::call("/api/series/detail", &[("url", url)], None).await {
            if STATE.with(|c| c.borrow().session != session) {
                return;
            }
            let episodes = rows(&series, "episodes");
            let artwork = art(&series);
            let needs_art = STATE.with(|c| {
                let mut p = c.borrow_mut();
                let needs_art = p
                    .episode
                    .as_ref()
                    .is_some_and(|ep| s(ep, "artwork").is_empty());
                if needs_art {
                    if let Some(ep) = p.episode.as_mut() {
                        ep["artwork"] = Value::String(artwork.clone());
                    }
                }
                p.series = series;
                p.episodes = episodes;
                p.season = p
                    .episode
                    .as_ref()
                    .map(|e| s(e, "season"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "1".into());
                needs_art
            });
            if needs_art {
                backdrop(&artwork);
            }
            show("drawerEpisodeTools", true);
            if let Ok(input) = el("drawerEpisodeSearch").dyn_into::<HtmlInputElement>() {
                input.set_value("");
            }
            render_drawer();
        }
    });
}
fn season_of(episode: &Value) -> String {
    let season = s(episode, "season");
    if season.is_empty() {
        "1".into()
    } else {
        season
    }
}
fn episode_card(ep: &Value, index: usize, artwork: &str, current_url: &str) -> String {
    let url = s(ep, "url");
    let active = url == current_url;
    let number = catalogue::episode_number(ep);
    let number = if number.is_empty() {
        (index + 1).to_string()
    } else {
        number
    };
    format!("<button class=\"episode-card {}\" data-player-action=\"episode\" data-url=\"{}\" {}><span class=\"episode-art\">{}<span class=\"episode-play\">{}</span></span><span class=\"episode-copy\"><small>{}</small><strong>{}</strong></span><span class=\"episode-number\">{}</span></button>",if active{"active"}else{""},esc(&url),if active{"aria-current=\"true\""}else{""},if artwork.is_empty(){icons::FILM.into()}else{format!("<img src=\"{}\" alt=\"\" loading=\"lazy\" decoding=\"async\">",esc(artwork))},icons::PLAY,if active{"تشاهد الآن"}else{""},esc(&s(ep,"title")),esc(&number))
}
fn render_drawer() {
    let (episodes, season, episode) = STATE.with(|c| {
        let p = c.borrow();
        (
            p.episodes.clone(),
            p.season.clone(),
            p.episode.clone().unwrap_or(Value::Null),
        )
    });
    let mut seasons: Vec<String> = episodes.iter().map(season_of).collect();
    seasons.sort_by_key(|s| s.parse::<u32>().unwrap_or(0));
    seasons.dedup();
    let has_multiple_seasons = seasons.len() > 1;
    let active_season = if has_multiple_seasons {
        if seasons.contains(&season) {
            season
        } else {
            seasons.first().cloned().unwrap_or_else(|| "1".into())
        }
    } else {
        String::new()
    };
    if has_multiple_seasons {
        show("drawerSeasonTabs", true);
        html("drawerSeasonTabs",&seasons.iter().map(|s|format!("<button class=\"season-tab {}\" data-player-action=\"season\" data-season=\"{}\">الموسم {}</button>",if *s==active_season{"active"}else{""},esc(s),esc(s))).collect::<String>());
    } else {
        show("drawerSeasonTabs", false);
    }
    let query = el("drawerEpisodeSearch")
        .dyn_into::<HtmlInputElement>()
        .map(|i| catalogue::normalized(&i.value()))
        .unwrap_or_default();
    let artwork = s(&episode, "artwork");
    let current_url = s(&episode, "url");
    let content = episodes
        .iter()
        .filter(|ep| !has_multiple_seasons || season_of(ep) == active_season)
        .filter(|ep| {
            query.is_empty()
                || catalogue::normalized(&format!(
                    "{} {}",
                    s(ep, "title"),
                    catalogue::episode_number(ep)
                ))
                .contains(&query)
        })
        .enumerate()
        .map(|(i, ep)| episode_card(ep, i, &artwork, &current_url))
        .collect::<String>();
    if episodes.is_empty() {
        text("drawerCount", "فيلم");
        show("drawerEpisodeTools", false);
        html(
            "drawerEpisodesCarousel",
            "<p class=\"empty-note\">أنت تشاهد فيلماً.</p>",
        );
        return;
    }
    html(
        "drawerEpisodesCarousel",
        if content.is_empty() {
            "<p class=\"empty-note\">لا توجد حلقات مطابقة.</p>"
        } else {
            &content
        },
    );
    text("drawerCount", &format!("{} حلقة", episodes.len()));
    let scroller = el("drawerEpisodesCarousel");
    if let Ok(Some(active_el)) = scroller.query_selector(".episode-card.active") {
        let sr = scroller.get_bounding_client_rect();
        let er = active_el.get_bounding_client_rect();
        let cur = scroller.scroll_top() as f64;
        let target = cur + (er.top() - sr.top()) - (sr.height() - er.height()) / 2.;
        scroller.set_scroll_top(target.max(0.) as i32);
    }
    el("appFrame").set_scroll_top(0);
}
fn all_episodes() {
    let (episodes, episode) = STATE.with(|c| {
        let p = c.borrow();
        (p.episodes.clone(), p.episode.clone().unwrap_or(Value::Null))
    });
    if episodes.is_empty() {
        app::open_sheet(
            "الفيلم",
            "<p class=\"empty-note\">أنت تشاهد فيلماً سينمائياً كاملاً.</p>",
        );
        return;
    }
    let content = episodes
        .iter()
        .enumerate()
        .map(|(i, ep)| episode_card(ep, i, &s(&episode, "artwork"), &s(&episode, "url")))
        .collect::<String>();
    app::open_sheet(
        "الحلقات",
        &format!(
            "<div class=\"episode-card-list\">{}</div>",
            if content.is_empty() {
                "<p class=\"empty-note\">قائمة الحلقات غير متاحة.</p>"
            } else {
                &content
            }
        ),
    );
}
pub fn open_sources() {
    let Some(episode) = episode_identity() else {
        return;
    };
    let current_source = catalogue::provider(&episode);
    let other = if current_source == "cinemana" {
        "krmzi"
    } else {
        "cinemana"
    };
    let group = app::known_series().into_iter().find(|item| {
        s(item, "url") == s(&episode, "seriesUrl")
            || rows(item, "offers")
                .iter()
                .any(|o| s(o, "url") == s(&episode, "seriesUrl"))
    });
    let offers = group
        .as_ref()
        .map(|g| rows(g, "offers"))
        .unwrap_or_default();
    let mut content = String::new();
    let mut found = false;
    for offer in offers.iter().filter(|o| catalogue::provider(o) == other) {
        found = true;
        content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"provider\" data-url=\"{}\"><span>نفس الحلقة من {}</span>{}</button>",esc(&s(offer,"url")),source_name(other),icons::SWITCH));
    }
    if !found {
        content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"discover\" data-source=\"{other}\"><span>ابحث عن نفس العمل في {}</span>{}</button>",source_name(other),icons::SWITCH));
    }

    let ed = catalogue::edition(&episode);
    let is_dubbed = ed == "dubbed";
    let target_ed = if is_dubbed { "original" } else { "dubbed" };
    let target_label = if is_dubbed { "المترجمة" } else { "المدبلجة" };
    let target_pill = if is_dubbed {
        format!("<span class=\"sheet-pill subbed\">{} مترجم</span>", icons::SUBTITLES)
    } else {
        format!("<span class=\"sheet-pill dubbed\">{} مدبلج</span>", icons::DUBBED)
    };
    let direct_url = offers
        .iter()
        .find(|o| catalogue::edition(o) == target_ed)
        .map(|o| s(o, "url"))
        .unwrap_or_default();
    let instant_label = if !direct_url.is_empty() { " (فوري)" } else { "" };
    content.push_str(&format!(
        "<button class=\"sheet-row highlight-edition\" data-player-action=\"switch-edition\" data-edition=\"{}\" data-url=\"{}\"><span>التبديل إلى النسخة {}{}</span>{}</button>",
        target_ed,
        esc(&direct_url),
        target_label,
        instant_label,
        target_pill
    ));
    if !rows(&episode, "directStreams").is_empty() {
        content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"direct\"><span>مشغّل {} المباشر</span><span class=\"sheet-pill\">{}</span></button>",source_name(current_source),if STATE.with(|c|c.borrow().embedded.is_none()){"الحالي"}else{"تشغيل"}));
    }
    for (i, server) in rows(&episode, "servers").iter().enumerate() {
        let name = s(server, "name");
        let is_direct = name.eq_ignore_ascii_case("ok");
        if is_direct {
            content.push_str(&format!(
                "<button class=\"sheet-row\" data-player-action=\"embed\" data-index=\"{i}\"><span>سيرفر مباشر ({})</span><span class=\"sheet-pill direct\">{} مباشر</span></button>",
                esc(&name),
                icons::BOLT
            ));
        } else {
            content.push_str(&format!(
                "<button class=\"sheet-row\" data-player-action=\"embed\" data-index=\"{i}\"><span>سيرفر {}</span><span class=\"sheet-pill\">خارجي</span></button>",
                esc(&name)
            ));
        }
    }
    content.push_str("<p class=\"source-switch-note\">التبديل بين المصدرين المباشرين يحتفظ بموضعك وإعداداتك. السيرفر الخارجي يستخدم مشغّله الخاص وقد يبدأ من البداية.</p>");
    app::open_sheet("المصادر والسيرفرات", &content);
}
fn begin_switch(title: &str) -> (u64, u64, Option<AbortController>) {
    cancel_switch();
    app::open_sheet(
        title,
        "<p class=\"empty-note\" role=\"status\">المشاهدة مستمرة أثناء تجهيز المصدر الآخر…</p>",
    );
    let controller = AbortController::new().ok();
    let (session, generation) = STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.switching = controller.clone();
        (p.session, p.switch_generation)
    });
    (session, generation, controller)
}
fn switch_current(session: u64, generation: u64, controller: &Option<AbortController>) -> bool {
    STATE.with(|c| {
        let p = c.borrow();
        p.session == session && p.switch_generation == generation
    }) && controller.as_ref().is_none_or(|c| !c.signal().aborted())
        && is_open()
}
fn discover(source: String) {
    let Some(episode) = episode_identity() else {
        return;
    };
    let series = STATE.with(|c| c.borrow().series.clone());
    let title = if !s(&episode, "seriesTitle").is_empty() {
        s(&episode, "seriesTitle")
    } else if !s(&series, "title").is_empty() {
        s(&series, "title")
    } else {
        s(&episode, "title")
    };
    let clean = catalogue::clean_series_title(&title);
    let identity = json!({
        "title": clean.clone(),
        "aliases": if episode["aliases"].is_array() { &episode["aliases"] } else { &series["aliases"] },
        "year": series["year"],
        "type": "series",
        "edition": catalogue::edition(&episode)
    });
    let (session, generation, controller) =
        begin_switch(&format!("البحث في {}", source_name(&source)));
    spawn_local(async move {
        let signal = controller.as_ref().map(|c| c.signal());
        let mut queries = vec![clean.clone()];
        if clean != title && !title.is_empty() {
            queries.push(title.clone());
        }
        if let Some(aliases) = identity["aliases"].as_array() {
            for a in aliases.iter().filter_map(Value::as_str) {
                let clean_a = catalogue::clean_series_title(a);
                if !clean_a.is_empty() && !queries.contains(&clean_a) {
                    queries.push(clean_a);
                }
            }
        }
        for key in ["en_title", "other_title"] {
            let val = s(&series, key);
            if !val.is_empty() {
                let clean_val = catalogue::clean_series_title(&val);
                if !clean_val.is_empty() && !queries.contains(&clean_val) {
                    queries.push(clean_val);
                }
            }
        }
        if let Some(slug) = catalogue::slug_from_url(&s(&series, "url")) {
            let clean_slug = catalogue::clean_series_title(&slug);
            if !clean_slug.is_empty() && !queries.contains(&clean_slug) {
                queries.push(clean_slug);
            }
        }
        for q in queries {
            let result = api::call_with_signal(
                "/api/search",
                &[("q", q)],
                Some(&source),
                signal.as_ref(),
            )
            .await;
            if !switch_current(session, generation, &controller) {
                return;
            }
            if let Some(candidate) = result.ok().and_then(|v| catalogue::find_counterpart(&identity, &rows(&v, "results"))) {
                let url = if !s(&candidate, "seriesUrl").is_empty() {
                    s(&candidate, "seriesUrl")
                } else {
                    s(&candidate, "url")
                };
                switch_provider(url);
                return;
            }
        }
        app::open_sheet(
            "ما لقينا تطابقاً مؤكداً",
            "<p class=\"empty-note\">المسلسل غير متاح أو اسمه مختلف في المصدر الآخر. المشاهدة الحالية مستمرة.</p>",
        );
    });
}
fn switch_edition(target_edition: String, direct_url: Option<String>) {
    let Some(episode) = episode_identity() else {
        return;
    };
    let series = STATE.with(|c| c.borrow().series.clone());
    let title = if !s(&episode, "seriesTitle").is_empty() {
        s(&episode, "seriesTitle")
    } else if !s(&series, "title").is_empty() {
        s(&series, "title")
    } else {
        s(&episode, "title")
    };
    let clean = catalogue::clean_series_title(&title);
    let identity = json!({
        "title": clean.clone(),
        "aliases": if episode["aliases"].is_array() { &episode["aliases"] } else { &series["aliases"] },
        "year": series["year"],
        "type": "series",
        "edition": &target_edition
    });
    let current_source = catalogue::provider(&episode).to_string();
    let other_source = if current_source == "cinemana" { "krmzi".to_string() } else { "cinemana".to_string() };
    let target_label = if target_edition == "dubbed" { "المدبلجة" } else { "المترجمة" };

    let (session, generation, controller) = begin_switch(&format!("الانتقال إلى النسخة {target_label}"));
    spawn_local(async move {
        let signal = controller.as_ref().map(|c| c.signal());
        // 0. If direct_url was provided or is in series["offers"], use it directly!
        let mut target_series_url = direct_url.filter(|u| !u.is_empty());
        if target_series_url.is_none() {
            if let Some(offers) = series["offers"].as_array() {
                if let Some(offer) = offers.iter().find(|o| catalogue::edition(o) == target_edition) {
                    let u = s(offer, "url");
                    if !u.is_empty() {
                        target_series_url = Some(u);
                    }
                }
            }
        }
        if let Some(url) = target_series_url {
            switch_provider(url);
            return;
        }

        // 1. Look in known_series for counterpart with target_edition
        let known = app::known_series();
        if let Some(candidate) = catalogue::find_counterpart_with_edition(&identity, &known, Some(&target_edition)) {
            let url = if !s(&candidate, "seriesUrl").is_empty() {
                s(&candidate, "seriesUrl")
            } else {
                s(&candidate, "url")
            };
            switch_provider(url);
            return;
        }

        // 2. Search on current source first, then other source
        let mut queries = vec![clean.clone()];
        if target_edition == "dubbed" {
            queries.push(format!("{clean} مدبلج"));
        } else {
            queries.push(format!("{clean} مترجم"));
        }
        if let Some(aliases) = identity["aliases"].as_array() {
            for a in aliases.iter().filter_map(Value::as_str) {
                let clean_a = catalogue::clean_series_title(a);
                if !clean_a.is_empty() && !queries.contains(&clean_a) {
                    queries.push(clean_a);
                }
            }
        }
        for key in ["en_title", "other_title"] {
            let val = s(&series, key);
            if !val.is_empty() {
                let clean_val = catalogue::clean_series_title(&val);
                if !clean_val.is_empty() && !queries.contains(&clean_val) {
                    queries.push(clean_val);
                }
            }
        }
        if let Some(slug) = catalogue::slug_from_url(&s(&series, "url")) {
            let clean_slug = catalogue::clean_series_title(&slug);
            if !clean_slug.is_empty() && !queries.contains(&clean_slug) {
                queries.push(clean_slug);
            }
        }
        for src in [&current_source, &other_source] {
            for q in &queries {
                let result = api::call_with_signal(
                    "/api/search",
                    &[("q", q.clone())],
                    Some(src),
                    signal.as_ref(),
                )
                .await;
                if !switch_current(session, generation, &controller) {
                    return;
                }
                if let Some(candidate) = result.ok().and_then(|v| {
                    catalogue::find_counterpart_with_edition(&identity, &rows(&v, "results"), Some(&target_edition))
                }) {
                    let url = if !s(&candidate, "seriesUrl").is_empty() {
                        s(&candidate, "seriesUrl")
                    } else {
                        s(&candidate, "url")
                    };
                    switch_provider(url);
                    return;
                }
            }
        }
        app::open_sheet(
            "النسخة غير متاحة",
            &format!("<p class=\"empty-note\">لم نجد النسخة {target_label} لهذا المسلسل في السيرفرات الحالية. المشاهدة الحالية مستمرة.</p>"),
        );
    });
}
fn switch_provider(url: String) {
    let Some(episode) = episode_identity() else {
        return;
    };
    let (session, generation, controller) = begin_switch("تجهيز المصدر البديل");
    spawn_local(async move {
        let signal = controller.as_ref().map(|c| c.signal());
        let result = api::call_with_signal(
            "/api/series/detail",
            &[("url", url.clone())],
            None,
            signal.as_ref(),
        )
        .await;
        if !switch_current(session, generation, &controller) {
            return;
        }
        let Ok(series) = result else {
            app::open_sheet(
                "المصدر غير متاح",
                "<p class=\"empty-note\">تعذر جلب الحلقات. المشاهدة الحالية مستمرة.</p>",
            );
            return;
        };
        let prev_offers = STATE.with(|c| c.borrow().series["offers"].clone());
        let mut series = series;
        if !prev_offers.is_null() && (series["offers"].is_null() || series["offers"].as_array().map_or(false, |a| a.is_empty())) {
            series["offers"] = prev_offers;
        }
        let target_episodes = rows(&series, "episodes");
        let current_all = STATE.with(|c| c.borrow().episodes.clone());
        let matched = catalogue::match_episode_cross_season(&episode, &current_all, &target_episodes);
        let Some(matched) = matched else {
            app::open_sheet("ترقيم الحلقات مختلف",&format!("<p class=\"empty-note\">ما غيرنا الحلقة تلقائياً حتى ما نفتح حلقة مختلفة.</p><button class=\"sheet-row\" data-player-action=\"browse-source\" data-url=\"{}\">عرض حلقات المصدر الآخر</button>",esc(&url)));
            return;
        };
        let result = api::call_with_signal(
            "/api/episode/detail",
            &[("url", s(&matched, "url"))],
            None,
            signal.as_ref(),
        )
        .await;
        if !switch_current(session, generation, &controller) {
            return;
        }
        let Ok(mut candidate) = result else {
            app::open_sheet(
                "المصدر غير متاح",
                "<p class=\"empty-note\">تعذر تجهيز رابط الحلقة. المشاهدة الحالية مستمرة.</p>",
            );
            return;
        };
        if rows(&candidate, "directStreams").is_empty() {
            app::open_sheet("الرابط غير متاح","<p class=\"empty-note\">المصدر الآخر لا يوفر رابطاً مباشراً لهذه الحلقة. المشاهدة الحالية مستمرة.</p>");
            return;
        }
        let settings = snapshot();
        let restore = restore_point();
        save_history(true);
        app::close_sheet();
        candidate["artwork"] = Value::String(s(&episode, "artwork"));
        let candidate_clone = candidate.clone();
        let new_session = STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.session += 1;
            p.attempt += 1;
            p.active_url = s(&candidate, "url");
            p.episode = Some(candidate);
            p.series = series;
            p.episodes = target_episodes;
            p.rollback = restore;
            p.refreshed = false;
            p.next_deadline = None;
            p.next_cancelled = false;
            p.session
        });
        title_ui(&candidate_clone);
        subtitles(&candidate_clone);
        backdrop(&s(&candidate_clone, "artwork"));
        play_direct(None, settings, false);
        load_drawer(s(&candidate_clone, "seriesUrl"), new_session);
    });
}
fn switch_mirror(index: usize) {
    let Some(episode) = current_episode() else {
        return;
    };
    let Some(server) = rows(&episode, "servers").get(index).cloned() else {
        return;
    };
    if !s(&server, "name").eq_ignore_ascii_case("ok") || s(&server, "id").is_empty() {
        app::open_sheet(
            "سيرفر غير مباشر",
            "<p class=\"empty-note\">هذا السيرفر خارجي ولا يوفر بثاً مباشراً داخل التطبيق. المشاهدة الحالية مستمرة بدون انقطاع.</p>",
        );
        return;
    }
    let (session, generation, controller) = begin_switch("تجهيز السيرفر البديل");
    spawn_local(async move {
        let signal = controller.as_ref().map(|c| c.signal());
        let result = api::call_with_signal(
            "/api/episode/detail",
            &[
                ("url", s(&episode, "url")),
                ("server", s(&server, "id")),
                ("fresh", "1".into()),
            ],
            None,
            signal.as_ref(),
        )
        .await;
        if !switch_current(session, generation, &controller) {
            return;
        }
        match result {
            Ok(mut candidate) if !rows(&candidate, "directStreams").is_empty() => {
                let settings = snapshot();
                let restore = restore_point();
                app::close_sheet();
                candidate["artwork"] = episode["artwork"].clone();
                STATE.with(|c| {
                    let mut p = c.borrow_mut();
                    p.session += 1;
                    p.episode = Some(candidate);
                    p.rollback = restore;
                    p.refreshed = false;
                });
                play_direct(None, settings, false);
            }
            Ok(_) | Err(_) => {
                app::open_sheet(
                    "السيرفر غير متاح",
                    "<p class=\"empty-note\">تعذر استخراج البث المباشر من هذا السيرفر. المشاهدة الحالية مستمرة بدون انقطاع.</p>",
                );
            }
        }
    });
}
pub fn open_quality() {
    let Some(episode) = current_episode() else {
        return;
    };
    let mut content = "<div class=\"sheet-sub-title\">جودة الفيديو</div>".to_string();
    let levels = hls_levels();
    let is_manual = storage("krmzi_quality_mode").as_deref() == Some("manual");
    let pref_quality = storage("krmzi_pref_quality").unwrap_or_default();
    if levels.len() > 1 {
        let auto_icon = if !is_manual { icons::CHECK } else { "" };
        content.push_str(&format!(
            "<button class=\"sheet-row\" data-player-action=\"quality-auto\"><span class=\"sheet-row-label\">{auto_icon}تلقائي (تكيف ديناميكي)</span></button>"
        ));
    }
    if !levels.is_empty() {
        for (index, height) in levels.iter().enumerate() {
            let label = if *height > 0 { format!("{height}p") } else { format!("المستوى {}", index + 1) };
            let is_active = is_manual && (label == pref_quality || format!("{height}p") == pref_quality);
            let icon = if is_active { icons::CHECK } else { "" };
            content.push_str(&format!(
                "<button class=\"sheet-row\" data-player-action=\"quality-hls\" data-index=\"{index}\"><span class=\"sheet-row-label\">{icon}{label}</span></button>"
            ));
        }
    } else {
        let streams = rows(&episode, "directStreams");
        if streams.iter().filter(|stream| stream["type"] != "hls").count() > 1
            && is_manual
        {
            content.push_str("<button class=\"sheet-row\" data-player-action=\"quality-default\"><span>الجودة الافتراضية</span></button>");
        }
        for (index, stream) in streams.iter().enumerate() {
            if stream["type"] != "hls" {
                let q_str = s(stream, "quality");
                let is_active = is_manual && (q_str == pref_quality || format!("{}p", quality_height(stream)) == pref_quality);
                let icon = if is_active { icons::CHECK } else { "" };
                content.push_str(&format!(
                    "<button class=\"sheet-row\" data-player-action=\"quality\" data-index=\"{index}\"><span class=\"sheet-row-label\">{icon}{}</span></button>",
                    esc(&q_str)
                ));
            }
        }
    }
    let audio_tracks = hls_audio_tracks();
    if audio_tracks.len() > 1 {
        content.push_str("<div class=\"sheet-sub-title\" style=\"margin-top:14px;\">المسار الصوتي</div>");
        let active = current_hls_audio_track();
        for (index, name) in &audio_tracks {
            let is_active = *index == active;
            let icon = if is_active { icons::CHECK } else { "" };
            content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"audio-track\" data-index=\"{index}\"><span class=\"sheet-row-label\">{icon}{name}</span></button>"));
        }
    }
    content.push_str("<div class=\"sheet-sub-title\" style=\"margin-top:14px;\">حجم خط الترجمة</div>");
    let current_size = storage("zoet_sub_size").unwrap_or_else(|| "medium".into());
    for (sz_key, sz_label) in [("small", "صغير"), ("medium", "متوسط (افتراضي)"), ("large", "كبير")] {
        let is_active = current_size == sz_key;
        let icon = if is_active { icons::CHECK } else { "" };
        content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"sub-size\" data-size=\"{sz_key}\"><span class=\"sheet-row-label\">{icon}{sz_label}</span></button>"));
    }
    content.push_str("<div class=\"sheet-sub-title\" style=\"margin-top:14px;\">لون خط الترجمة</div>");
    let current_color = storage("zoet_sub_color").unwrap_or_else(|| "yellow".into());
    for (c_key, c_label, _) in SUB_COLORS {
        let is_active = current_color == c_key;
        let icon = if is_active { icons::CHECK } else { "" };
        content.push_str(&format!("<button class=\"sheet-row\" data-player-action=\"sub-color\" data-color=\"{c_key}\"><span class=\"sheet-row-label\">{icon}{c_label}</span></button>"));
    }
    app::open_sheet("خيارات المشغل والصوت", &content);
}
fn hls_levels() -> Vec<u32> {
    STATE.with(|c| {
        c.borrow()
            .hls
            .as_ref()
            .map(|h| {
                let levels = Reflect::get(&h.instance, &"levels".into()).unwrap_or(JsValue::UNDEFINED);
                if Array::is_array(&levels) {
                    Array::from(&levels)
                        .iter()
                        .map(|level| {
                            Reflect::get(&level, &"height".into())
                                .ok()
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.) as u32
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    })
}
fn hls_audio_tracks() -> Vec<(i32, String)> {
    STATE.with(|c| {
        c.borrow()
            .hls
            .as_ref()
            .map(|h| {
                let tracks = Reflect::get(&h.instance, &"audioTracks".into()).unwrap_or(JsValue::UNDEFINED);
                if Array::is_array(&tracks) {
                    Array::from(&tracks)
                        .iter()
                        .enumerate()
                        .map(|(i, track)| {
                            let name = Reflect::get(&track, &"name".into())
                                .ok()
                                .and_then(|v| v.as_string())
                                .or_else(|| {
                                    Reflect::get(&track, &"lang".into())
                                        .ok()
                                        .and_then(|v| v.as_string())
                                })
                                .unwrap_or_else(|| format!("مسار {}", i + 1));
                            (i as i32, name)
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    })
}
fn current_hls_audio_track() -> i32 {
    STATE.with(|c| {
        c.borrow()
            .hls
            .as_ref()
            .and_then(|h| {
                Reflect::get(&h.instance, &"audioTrack".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as i32)
            })
            .unwrap_or(-1)
    })
}
fn set_hls_audio_track(index: i32) {
    let instance = STATE.with(|c| c.borrow().hls.as_ref().map(|h| h.instance.clone()));
    if let Some(h) = instance {
        let _ = Reflect::set(&h, &"audioTrack".into(), &index.into());
    }
}
fn apply_sub_size() {
    let size = storage("zoet_sub_size").unwrap_or_else(|| "medium".into());
    for s in ["small", "medium", "large"] {
        class("mobilePlayer", &format!("sub-size-{s}"), s == size);
    }
}
fn hls_level(index: i32) {
    let instance = STATE.with(|c| c.borrow().hls.as_ref().map(|h| h.instance.clone()));
    if let Some(h) = instance {
        let val: JsValue = index.into();
        let _ = Reflect::set(&h, &"currentLevel".into(), &val);
        let _ = Reflect::set(&h, &"loadLevel".into(), &val);
        let _ = Reflect::set(&h, &"nextLevel".into(), &val);
    }
}
fn save_history(force: bool) {
    let time = now();
    let episode = STATE.with(|c| {
        let mut p = c.borrow_mut();
        if !force && time - p.last_save < 5000. {
            return None;
        }
        p.last_save = time;
        p.episode.clone()
    });
    if let Some(episode) = episode {
        let settings = snapshot();
        let duration = video().duration();
        app::save_player_history(
            &episode,
            settings.time,
            if duration.is_finite() { duration } else { 0. },
        );
    }
}
pub fn close() {
    save_history(true);
    cancel_switch();
    STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.session += 1;
        p.attempt += 1;
        p.active_url.clear();
        if let Some(controller) = p.controller.take() {
            controller.abort();
        }
        p.episode = None;
        p.episodes.clear();
        p.rollback = None;
        p.embed_timer = None;
        p.next_deadline = None;
        p.hud = None;
        p.hold = None;
        p.tap_timer = None;
        p.unlock_timer = None;
        p.locked = false;
    });
    class("mobilePlayer", "touch-locked", false);
    show("unlockTouchBtn", false);
    release_media();
    show("mobilePlayer", false);
    show("playerSpinner", false);
    show("skipIntroBtn", false);
    show("autoNextCard", false);
    bridge("setVideoPlaying", false.into());
    bridge("setLandscape", false.into());
    if document().fullscreen_element().is_some() {
        document().exit_fullscreen();
    }
}
pub fn toggle() {
    if !is_open() {
        return;
    }
    if video().paused() {
        play_video();
    } else {
        let _ = video().pause();
    }
    show_hud(true);
}
pub fn seek(seconds: f64) {
    let v = video();
    if v.duration().is_finite() && v.duration() > 0. {
        v.set_current_time((v.current_time() + seconds).clamp(0., v.duration()));
        progress();
    }
}
fn format_time(time: f64) -> String {
    let time = if time.is_finite() {
        time.max(0.) as u64
    } else {
        0
    };
    if time >= 3600 {
        format!("{}:{:02}:{:02}", time / 3600, (time / 60) % 60, time % 60)
    } else {
        format!("{:02}:{:02}", time / 60, time % 60)
    }
}
fn progress() {
    let v = video();
    let duration = v.duration();
    if !duration.is_finite() || duration <= 0. {
        show("skipIntroBtn", false);
        return;
    }
    let time = v.current_time();
    let percent = (time / duration * 100.).clamp(0., 100.);
    style("timelineProgress", "width", &format!("{percent}%"));
    style("timelineThumb", "left", &format!("{percent}%"));
    text(
        "hudTime",
        &format!("{} / {}", format_time(time), format_time(duration)),
    );
    let _ = el("timelineBar").set_attribute("aria-valuenow", &format!("{percent:.0}"));
    let _ = el("timelineBar").set_attribute(
        "aria-valuetext",
        &format!("{} من {}", format_time(time), format_time(duration)),
    );
    let buffered = v.buffered();
    if buffered.length() > 0 {
        if let Ok(end) = buffered.end(buffered.length() - 1) {
            style(
                "timelineLoaded",
                "width",
                &format!("{}%", (end / duration * 100.).clamp(0., 100.)),
            );
        }
    }
    if time < duration - 1. {
        let cancel = STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.next_deadline.take().is_some()
        });
        if cancel {
            show("autoNextCard", false);
        }
    }
    show("skipIntroBtn", intro_end(&v).is_some());
    save_history(false);
}
fn intro_end(v: &HtmlVideoElement) -> Option<f64> {
    if !STATE.with(|c| {
        let p = c.borrow();
        p.metadata && p.episode.is_some() && p.embedded.is_none()
    }) {
        return None;
    }
    let tracks = v.text_tracks()?;
    let time = v.current_time();
    let duration = v.duration();
    (0..tracks.length())
        .filter_map(|index| tracks.get(index))
        .filter(|track| track.kind() == TextTrackKind::Chapters)
        .find_map(|track| {
            if track.mode() == TextTrackMode::Disabled {
                track.set_mode(TextTrackMode::Hidden);
            }
            let cues = track.cues()?;
            (0..cues.length()).filter_map(|index| cues.get(index)).find_map(|cue| {
                let end = cue.end_time();
                let is_intro = matches!(
                    cue.text().trim().to_ascii_lowercase().as_str(),
                    "intro" | "opening" | "الشارة" | "شارة البداية" | "المقدمة"
                );
                (is_intro && cue.start_time() <= time && time < end && end <= duration)
                    .then_some(end)
            })
        })
}
fn sync_video_quality() {
    if !STATE.with(|c| {
        let p = c.borrow();
        p.metadata && p.episode.is_some() && p.embedded.is_none()
    }) {
        return;
    }
    let height = video().video_height();
    if height > 0 {
        let quality = format!("{height}p");
        let label = el("activeQualityLabel");
        if label.text_content().as_deref() != Some(quality.as_str()) {
            label.set_text_content(Some(&quality));
        }
    }
}
fn metadata() {
    let settings = STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.metadata = true;
        p.settings.clone()
    });
    let v = video();
    if v.duration().is_finite() && settings.time > 0. {
        v.set_current_time(settings.time.min((v.duration() - 1.).max(0.)));
    }
    v.set_playback_rate(settings.rate);
    v.set_volume(settings.volume);
    v.set_muted(settings.muted);
    sync_video_quality();
    progress();
}
fn ready(playing: bool) {
    let (paused, switched) = STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.ready = true;
        p.advanced = now();
        let commit = playing || p.settings.paused;
        let switched = if commit {
            p.rollback.take().is_some()
        } else {
            false
        };
        (p.settings.paused, switched)
    });
    show("nativePlayer", true);
    if playing || paused {
        show("playerSpinner", false);
        class("mobilePlayer", "player-loading", false);
        class("mobilePlayer", "player-error", false);
        class("mobilePlayer", "video-playing", true);
    }
    if switched {
        save_history(true);
        pill("تم تغيير المصدر مع حفظ موضعك");
    }
}
pub fn pill(message: &str) {
    text("swipeValue", message);
    show("swipeIndicator", true);
    let timer = Timeout::new(1800, || show("swipeIndicator", false));
    STATE.with(|c| c.borrow_mut().pill = Some(timer));
}
fn is_hud_visible() -> bool {
    !el("playerHudTop").class_list().contains("hud-hidden")
        || !el("playerHudBottom").class_list().contains("hud-hidden")
        || !el("hudCenter").class_list().contains("hud-hidden")
}
fn hide_hud() {
    STATE.with(|c| {
        c.borrow_mut().hud = None;
    });
    class("playerHudTop", "hud-hidden", true);
    class("playerHudBottom", "hud-hidden", true);
    class("hudCenter", "hud-hidden", true);
    if !video().paused() {
        class("playerCanvas", "cursor-hidden", true);
    }
}
fn show_hud(auto_hide: bool) {
    class("playerHudTop", "hud-hidden", false);
    class("playerHudBottom", "hud-hidden", false);
    class("playerCanvas", "cursor-hidden", false);
    class(
        "hudCenter",
        "hud-hidden",
        STATE.with(|c| c.borrow().embedded.is_some()),
    );
    let timer = if auto_hide {
        Some(Timeout::new(3200, || {
            let bottom = el("playerHudBottom");
            let hovering = bottom.matches(":hover").unwrap_or(false)
                && window()
                    .match_media("(any-hover: hover)")
                    .ok()
                    .flatten()
                    .is_some_and(|query| query.matches());
            let focused = bottom.query_selector(":focus-visible").ok().flatten().is_some();
            if !hovering
                && !focused
                && (!video().paused() || STATE.with(|c| c.borrow().embedded.is_some()))
            {
                hide_hud();
            }
        }))
    } else {
        None
    };
    STATE.with(|c| c.borrow_mut().hud = timer);
}
pub fn toggle_theater() {
    let wide = !el("mobilePlayer").class_list().contains("wide-mode");
    class("mobilePlayer", "wide-mode", wide);
    store("zoet_player_wide", if wide { "1" } else { "0" });
    sync_tool_states();
    pill(if wide {
        "وضع العرض: شاشة عريضة"
    } else {
        "وضع المسرح: قائمة الحلقات الجانبية"
    });
}
fn sync_tool_states() {
    let sub_showing = video()
        .text_tracks()
        .map(|tracks| {
            (0..tracks.length()).any(|i| {
                tracks
                    .get(i)
                    .is_some_and(|t| is_caption_track(&t) && t.mode() == TextTrackMode::Showing)
            })
        })
        .unwrap_or(false);
    class("toggleSubtitlesBtn", "active", sub_showing);
    let _ = el("toggleSubtitlesBtn").set_attribute("aria-pressed", if sub_showing { "true" } else { "false" });
    let wide = el("mobilePlayer").class_list().contains("wide-mode");
    class("toggleTheaterBtn", "active", wide);
    let fit = storage("otium_fit_mode").unwrap_or_else(|| "contain".into());
    class("toggleAspectRatioBtn", "active", fit != "contain");
    let fs = document().fullscreen_element().is_some();
    class("toggleFullscreenBtn", "active", fs);
    html(
        "toggleFullscreenBtn",
        if fs {
            icons::FULLSCREEN_EXIT
        } else {
            icons::FULLSCREEN_ENTER
        },
    );
}
fn set_speed(rate: f64) {
    let rate = rate.clamp(0.5, 2.0);
    video().set_playback_rate(rate);
    STATE.with(|c| c.borrow_mut().settings.rate = rate);
    pill(&format!("السرعة: {rate}×"));
}
fn reveal_unlock_pill() {
    show("unlockTouchBtn", true);
    let timer = Timeout::new(2400, || {
        if STATE.with(|c| c.borrow().locked) {
            show("unlockTouchBtn", false);
        }
    });
    STATE.with(|c| c.borrow_mut().unlock_timer = Some(timer));
}
fn set_touch_locked(locked: bool) {
    STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.locked = locked;
        p.unlock_timer = None;
    });
    class("mobilePlayer", "touch-locked", locked);
    if locked {
        app::close_sheet();
        hide_hud();
        reveal_unlock_pill();
        pill("تم قفل شاشة اللمس");
    } else {
        show("unlockTouchBtn", false);
        show_hud(true);
        pill("تم إلغاء قفل الشاشة");
    }
}
pub fn open_tools_sheet() {
    let v = video();
    let cur_rate = v.playback_rate();
    let cur_fit = storage("otium_fit_mode").unwrap_or_else(|| "contain".into());
    let cur_filter = storage("zoet_video_filter").unwrap_or_else(|| "normal".into());
    let cur_offset = STATE.with(|c| c.borrow().sub_offset);
    let has_subs = v.text_tracks().is_some_and(|tracks| {
        (0..tracks.length()).any(|i| tracks.get(i).as_ref().is_some_and(is_caption_track))
    });
    let is_locked = STATE.with(|c| c.borrow().locked);
    let is_wide = el("mobilePlayer").class_list().contains("wide-mode");

    let mut html = String::from("<div class=\"player-tools-sheet\">");

    html.push_str("<div class=\"sheet-sub-title\">سرعة التشغيل</div><div class=\"sheet-chip-grid\">");
    for rate in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
        let active = (cur_rate - rate).abs() < 0.05;
        let label = if (rate - 1.0).abs() < 0.01 { "عادي 1×".into() } else { format!("{rate}×") };
        html.push_str(&format!(
            "<button class=\"sheet-chip-btn {}\" data-player-action=\"speed\" data-rate=\"{rate}\">{label}</button>",
            if active { "active" } else { "" }
        ));
    }
    html.push_str("</div>");

    html.push_str("<div class=\"sheet-sub-title\">أبعاد العرض</div><div class=\"sheet-chip-grid\">");
    for (mode, label) in [("contain", "الإطار الكامل"), ("cover", "ملء الشاشة"), ("fill", "تمدد")] {
        let active = cur_fit == mode;
        html.push_str(&format!(
            "<button class=\"sheet-chip-btn {}\" data-player-action=\"fit-mode\" data-mode=\"{mode}\">{label}</button>",
            if active { "active" } else { "" }
        ));
    }
    html.push_str("</div>");

    html.push_str("<div class=\"sheet-sub-title\">فلاتر الصورة</div><div class=\"sheet-chip-grid\">");
    for (id, label, _) in FILTERS {
        let active = cur_filter == id;
        html.push_str(&format!(
            "<button class=\"sheet-chip-btn {}\" data-player-action=\"filter-preset\" data-filter=\"{id}\">{label}</button>",
            if active { "active" } else { "" }
        ));
    }
    html.push_str("</div>");

    html.push_str("<div class=\"sheet-sub-title\">الترجمة والتزامن</div><div class=\"sheet-chip-grid\">");
    if has_subs {
        html.push_str("<button class=\"sheet-chip-btn\" data-player-action=\"sub-toggle\">تشغيل / إيقاف الترجمة</button>");
    }
    html.push_str(&format!(
        "<button class=\"sheet-chip-btn\" data-player-action=\"sub-delay\" data-delta=\"-0.5\">تقديم -0.5ث</button><button class=\"sheet-chip-btn\" data-player-action=\"sub-delay-reset\">تصفير ({:+.1}ث)</button><button class=\"sheet-chip-btn\" data-player-action=\"sub-delay\" data-delta=\"0.5\">تأخير +0.5ث</button>",
        cur_offset
    ));
    html.push_str("<button class=\"sheet-chip-btn\" data-player-action=\"sub-import\">تحميل ملف (.srt / .vtt)</button></div>");

    html.push_str("<div class=\"sheet-sub-title\">إجراءات سريعة</div>");
    html.push_str(&format!(
        "<button class=\"sheet-row\" data-player-action=\"lock-touch\"><span class=\"sheet-row-label\">{}<span>{}</span></span><span class=\"sheet-pill\">لمنع النقرات العرضية</span></button>",
        icons::LOCK,
        if is_locked { "إلغاء قفل شاشة اللمس" } else { "قفل شاشة اللمس" }
    ));
    html.push_str(&format!(
        "<button class=\"sheet-row\" data-player-action=\"snapshot\"><span class=\"sheet-row-label\">{}<span>التقاط صورة من المشهد</span></span><span class=\"sheet-pill\">PNG</span></button>",
        icons::CAMERA
    ));
    html.push_str("<button class=\"sheet-row\" data-player-action=\"toggle-pip\"><span>صورة داخل صورة (نافذة عائمة)</span><span class=\"sheet-pill\">PiP</span></button>");
    if window().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.) >= 1024. {
        html.push_str(&format!(
            "<button class=\"sheet-row\" data-player-action=\"toggle-theater\"><span>{}</span><span class=\"sheet-pill\">T</span></button>",
            if is_wide { "إظهار قائمة الحلقات الجانبية" } else { "وضع الشاشة العريضة" }
        ));
    }
    html.push_str("<button class=\"sheet-row\" data-player-action=\"controls-help\"><span>اختصارات التحكم والريموت</span><span class=\"sheet-pill\">؟</span></button>");
    html.push_str("</div>");
    app::open_sheet("أدوات المشغل", &html);
}
pub fn adjust_sub_delay(delta_seconds: f64) {
    let new_offset = STATE.with(|c| {
        let mut p = c.borrow_mut();
        p.sub_offset += delta_seconds;
        p.sub_offset
    });
    let v = video();
    if let Some(tracks) = v.text_tracks() {
        for i in 0..tracks.length() {
            if let Some(track) = tracks.get(i) {
                if is_caption_track(&track) && track.mode() == TextTrackMode::Showing {
                    if let Some(cues) = track.cues() {
                        for j in 0..cues.length() {
                            if let Ok(cue_val) = js_sys::Reflect::get_u32(&cues, j) {
                                if let Ok(cue) = cue_val.dyn_into::<TextTrackCue>() {
                                    cue.set_start_time(cue.start_time() + delta_seconds);
                                    cue.set_end_time(cue.end_time() + delta_seconds);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    pill(&format!("مزامنة الترجمة: {:+.2} ثانية", new_offset));
}
fn reset_sub_delay() {
    let offset = STATE.with(|c| c.borrow().sub_offset);
    if offset.abs() > 0.001 {
        adjust_sub_delay(-offset);
    } else {
        pill("مزامنة الترجمة: +0.00 ثانية");
    }
}
pub fn take_snapshot() {
    let v = video();
    let width = v.video_width();
    let height = v.video_height();
    if width == 0 || height == 0 {
        pill("الفيديو غير متاح لالتقاط الصورة");
        return;
    }
    let Ok(canvas) = document().create_element("canvas") else { return; };
    let Ok(canvas) = canvas.dyn_into::<HtmlCanvasElement>() else { return; };
    canvas.set_width(width);
    canvas.set_height(height);
    let Ok(Some(ctx_obj)) = canvas.get_context("2d") else { return; };
    let Ok(ctx) = ctx_obj.dyn_into::<CanvasRenderingContext2d>() else { return; };
    if ctx.draw_image_with_html_video_element(&v, 0., 0.).is_err() {
        pill("تعذر التقاط لقطة الشاشة لمصدر البث");
        return;
    }
    if let Ok(data_url) = canvas.to_data_url() {
        if let Ok(link) = document().create_element("a") {
            let title = STATE.with(|c| {
                c.borrow().episode.as_ref().map(|e| s(e, "title")).unwrap_or_else(|| "ZOET".into())
            });
            let clean_title = title.replace(' ', "_");
            let _ = link.set_attribute("href", &data_url);
            let _ = link.set_attribute("download", &format!("ZOET_{clean_title}_{:.0}.png", v.current_time()));
            if let Ok(link_el) = link.dyn_into::<HtmlElement>() {
                link_el.click();
                pill("تم حفظ لقطة الشاشة");
                return;
            }
        }
    }
    pill("تعذر استخراج صورة المشهد لمصدر البث");
}
pub const FILTERS: [(&str, &str, &str); 4] = [
    ("normal", "عادي", "none"),
    ("anime", "أنمي مشرق", "contrast(1.12) saturate(1.28) brightness(1.04)"),
    ("cinema", "سينما ليلي", "contrast(1.08) brightness(0.92) saturate(1.05)"),
    ("shadow", "مُعادل المشاهد المظلمة", "contrast(1.02) brightness(1.15) saturate(1.02)"),
];

pub fn cycle_video_filter() {
    let current = storage("zoet_video_filter").unwrap_or_else(|| "normal".into());
    let idx = FILTERS.iter().position(|(id, _, _)| *id == current).unwrap_or(0);
    let next_idx = (idx + 1) % FILTERS.len();
    let (id, label, css) = FILTERS[next_idx];
    store("zoet_video_filter", id);
    apply_video_filter(css);
    sync_tool_states();
    pill(&format!("فلتر الصورة: {label}"));
}

pub fn apply_video_filter(filter_css: &str) {
    style("nativePlayer", "filter", filter_css);
}

pub const SUB_COLORS: [(&str, &str, &str); 5] = [
    ("yellow", "أصفر كلاسيكي", "#ffd700"),
    ("white", "أبيض ناصع", "#ffffff"),
    ("cyan", "سماوي نيون", "#00f0ff"),
    ("green", "أخضر فاتح", "#39ff14"),
    ("amber", "كهرماني دافئ", "#ffbf00"),
];

pub fn cycle_sub_color() {
    let current = storage("zoet_sub_color").unwrap_or_else(|| "yellow".into());
    let idx = SUB_COLORS.iter().position(|(id, _, _)| *id == current).unwrap_or(0);
    let next_idx = (idx + 1) % SUB_COLORS.len();
    let (id, label, hex) = SUB_COLORS[next_idx];
    store("zoet_sub_color", id);
    apply_sub_color(hex);
    pill(&format!("لون الترجمة: {label}"));
}

pub fn apply_sub_color(color_hex: &str) {
    style("mobilePlayer", "--sub-cue-color", color_hex);
    style("nativePlayer", "--sub-cue-color", color_hex);
}

pub fn toggle_mute() {
    let v = video();
    let muted = !v.muted();
    v.set_muted(muted);
    pill(if muted { "تم كتم الصوت" } else { "تم تشغيل الصوت" });
}
fn load_subtitle_file(file: web_sys::File) {
    let name = file.name();
    let lower = name.to_lowercase();
    if !lower.ends_with(".srt") && !lower.ends_with(".vtt") {
        pill("يرجى اختيار ملف ترجمة بصيغة .srt أو .vtt");
        return;
    }
    let Ok(reader) = FileReader::new() else { return; };
    let reader_clone = reader.clone();
    let file_name = name.clone();
    let is_srt = lower.ends_with(".srt");
    let onload = Closure::wrap(Box::new(move |_: Event| {
        if let Ok(result) = reader_clone.result() {
            if let Some(text) = result.as_string() {
                let vtt_text = if is_srt {
                    let re = js_sys::RegExp::new(r"(\d{2}:\d{2}:\d{2}),(\d{3})", "g");
                    let converted = js_sys::JsString::from(text.as_str()).replace_by_pattern(&re, "$1.$2");
                    format!("WEBVTT\n\n{}", converted)
                } else {
                    text
                };
                let array = js_sys::Array::new();
                array.push(&JsValue::from_str(&vtt_text));
                let Ok(blob) = web_sys::Blob::new_with_str_sequence(&array) else { return; };
                if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                    let v = video();
                    if let Ok(node) = document().create_element("track") {
                        let _ = node.set_attribute("kind", "subtitles");
                        let _ = node.set_attribute("label", &format!("ترجمة: {}", file_name));
                        let _ = node.set_attribute("srclang", "ar");
                        let _ = node.set_attribute("src", &url);
                        let _ = node.set_attribute("default", "true");
                        let _ = v.append_child(&node);
                        if let Some(tracks) = v.text_tracks() {
                            if let Some(last) = tracks.get(tracks.length() - 1) {
                                last.set_mode(TextTrackMode::Showing);
                            }
                        }
                        show("toggleSubtitlesBtn", true);
                        sync_tool_states();
                        pill(&format!("تم تحميل الترجمة: {}", file_name));
                    }
                }
            }
        }
    }) as Box<dyn FnMut(Event)>);
    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    let _ = reader.read_as_text(&file);
}
pub fn init_sub_drag_and_drop() {
    on("playerCanvas", "dragover", |event| {
        if let Ok(event) = event.dyn_into::<DragEvent>() {
            event.prevent_default();
        }
    });
    on("playerCanvas", "drop", |event| {
        if let Ok(event) = event.dyn_into::<DragEvent>() {
            event.prevent_default();
            let Some(dt) = event.data_transfer() else { return; };
            let Some(files) = dt.files() else { return; };
            if let Some(file) = files.item(0) {
                load_subtitle_file(file);
            }
        }
    });
    on("subFileInput", "change", |_| {
        if let Ok(input) = el("subFileInput").dyn_into::<HtmlInputElement>() {
            if let Some(files) = input.files() {
                if let Some(file) = files.item(0) {
                    load_subtitle_file(file);
                }
            }
            input.set_value("");
        }
    });
}
fn next_episode() {
    let next = STATE.with(|c| {
        let p = c.borrow();
        let current_url = p.episode.as_ref().map(|e| s(e, "url"))?;
        let index = p.episodes.iter().position(|e| s(e, "url") == current_url)?;
        p.episodes.get(index + 1).map(|e| s(e, "url"))
    });
    if let Some(url) = next {
        open_episode(url, None);
    }
}
fn ended() {
    save_history(true);
    let next = STATE.with(|c| {
        let mut p = c.borrow_mut();
        if p.next_cancelled {
            return None;
        }
        let url = p.episode.as_ref().map(|e| s(e, "url"))?;
        let index = p.episodes.iter().position(|e| s(e, "url") == url)?;
        let next = p.episodes.get(index + 1)?.clone();
        p.next_deadline = Some(now() + 5000.);
        Some(next)
    });
    if let Some(next) = next {
        text("autoNextTitle", &s(&next, "title"));
        text("autoNextTimer", "5");
        show("autoNextCard", true);
    }
    show_hud(false);
}
fn tick() {
    if !is_open() {
        return;
    }
    let v = video();
    let time = v.current_time();
    let buffered = v.buffered();
    let mut end = 0.;
    for i in 0..buffered.length() {
        if let (Ok(start), Ok(b_end)) = (buffered.start(i), buffered.end(i)) {
            if time >= start - 1.0 && time <= b_end + 1.0 {
                end = b_end;
                break;
            }
        }
    }
    if end == 0. && buffered.length() > 0 {
        end = buffered.end(buffered.length() - 1).unwrap_or(0.);
    }
    let (failure, next) = STATE.with(|c| {
        let mut p = c.borrow_mut();
        let now = now();
        let next = p.next_deadline.is_some_and(|deadline| now >= deadline);
        if let Some(deadline) = p.next_deadline {
            text(
                "autoNextTimer",
                &format!("{}", ((deadline - now) / 1000.).ceil().max(0.)),
            );
        }
        if next {
            p.next_deadline = None;
        }
        let waiting = p.episode.is_some() && p.embedded.is_none() && !p.queue.is_empty();
        if document().hidden() || (p.ready && v.paused()) {
            p.advanced = now;
            return (false, next);
        }
        if time > p.last_time + 0.1 || end > p.last_buffer + 0.1 {
            p.advanced = now;
        }
        p.last_time = time;
        p.last_buffer = end;
        let is_tv = document()
            .document_element()
            .and_then(|e| e.get_attribute("data-display"))
            .as_deref()
            == Some("tv");
        let startup_limit = if is_tv { 24000. } else { 16000. };
        let stall_limit = if p.ready {
            if is_tv { 22000. } else { 14000. }
        } else {
            if is_tv { 14000. } else { 6500. }
        };
        let failed = waiting
            && ((!p.ready && now - p.started > startup_limit)
                || now - p.advanced > stall_limit);
        (failed, next)
    });
    if next {
        next_episode();
    } else if failure {
        failed();
    }
}
fn is_caption_track(track: &TextTrack) -> bool {
    matches!(track.kind(), TextTrackKind::Subtitles | TextTrackKind::Captions)
}
pub fn toggle_subtitles() {
    let Some(tracks) = video().text_tracks() else {
        return;
    };
    let mut showing = false;
    let mut preferred = None;
    for index in 0..tracks.length() {
        if let Some(track) = tracks.get(index).filter(is_caption_track) {
            showing |= track.mode() == TextTrackMode::Showing;
            if preferred.is_none() || track.language() == "ar" {
                preferred = Some(track.clone());
            }
            track.set_mode(TextTrackMode::Disabled);
        }
    }
    if !showing {
        if let Some(track) = preferred {
            track.set_mode(TextTrackMode::Showing);
        }
    }
    sync_tool_states();
    pill(if showing {
        "الترجمة متوقفة"
    } else {
        "الترجمة مفعّلة"
    });
}
fn fullscreen() {
    if document().fullscreen_element().is_some() {
        document().exit_fullscreen();
        bridge("setLandscape", false.into());
    } else {
        let _ = el("appFrame").request_fullscreen();
        bridge("setLandscape", true.into());
    }
    sync_tool_states();
}
fn pip() {
    let document: JsValue = document().into();
    let active =
        Reflect::get(&document, &"pictureInPictureElement".into()).unwrap_or(JsValue::NULL);
    let result = if active.is_null() || active.is_undefined() {
        call(video().as_ref(), "requestPictureInPicture", &Array::new())
    } else {
        call(&document, "exitPictureInPicture", &Array::new())
    };
    if let Ok(promise) = result {
        if let Ok(promise) = promise.dyn_into::<Promise>() {
            spawn_local(async move {
                let _ = JsFuture::from(promise).await;
            });
        }
    }
}
fn apply_fit_mode(mode: &str) {
    for name in ["contain", "cover", "fill"] {
        class("playerCanvas", &format!("fit-{name}"), name == mode);
    }
    store("otium_fit_mode", mode);
    sync_tool_states();
    pill(match mode {
        "cover" => "ملء الشاشة مع قص الأطراف",
        "fill" => "تمدد الفيديو",
        _ => "إظهار الفيديو بالكامل",
    });
}
pub fn fit() {
    let previous = storage("otium_fit_mode").unwrap_or_else(|| "contain".into());
    let mode = match previous.as_str() {
        "contain" => "cover",
        "cover" => "fill",
        _ => "contain",
    };
    apply_fit_mode(mode);
}
pub fn controls_help() {
    let rows = [
        ("Space / K", "تشغيل وإيقاف"),
        ("J / L", "رجوع وتقديم 10 ثوانٍ"),
        ("← / →", "5 ثوانٍ · مع Shift: 30 ثانية"),
        ("0–9", "الانتقال لنسبة من الفيديو"),
        ("M", "كتم الصوت"),
        ("F / P", "ملء الشاشة / صورة مصغرة"),
        ("T", "وضع المسرح / شاشة عريضة"),
        ("V", "فلاتر الصورة (أنمي / سينما / ليلي)"),
        ("G / H", "مزامنة الترجمة (- / +)"),
        ("Shift+C", "لون الترجمة (أصفر / أبيض / نيون)"),
        ("Shift+S", "التقاط لقطة شاشة"),
        ("Gamepad", "دعم كامل ليد تحكم إكس بوكس وبلايستيشن"),
        ("C / Q", "الترجمة / الجودة"),
        ("E / S", "الحلقات / المصادر"),
        ("[ / ]", "سرعة العرض"),
        ("N", "الحلقة التالية"),
        ("Esc", "إغلاق القائمة أو المشغل"),
    ];
    app::open_sheet("اختصارات التحكم",&format!("<div class=\"controls-guide\"><p>انقر مرتين للتقديم أو الرجوع. اضغط مطوّلاً لسرعة 2×. اسحب عمودياً يمين الفيديو للصوت ويساره للإضاءة. تدعم ZOET يد التحكم (Gamepad) لأجهزة Xbox و PlayStation بالكامل.</p><div class=\"shortcut-list\">{}</div></div>",rows.iter().map(|(key,label)|format!("<div><kbd dir=\"ltr\">{}</kbd><span>{}</span></div>",esc(key),esc(label))).collect::<String>()));
}
fn action(row: Element) {
    let kind = row.get_attribute("data-player-action").unwrap_or_default();
    let url = row.get_attribute("data-url").unwrap_or_default();
    let index = row
        .get_attribute("data-index")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    match kind.as_str() {
        "episode" => {
            app::close_sheet();
            open_episode(url, None);
        }
        "retry" => {
            let url = STATE.with(|c| c.borrow().active_url.clone());
            if !url.is_empty() {
                open(url, Some(snapshot()), true);
            }
        }
        "sources" => open_sources(),
        "provider" => switch_provider(url),
        "discover" => discover(row.get_attribute("data-source").unwrap_or_default()),
        "switch-edition" => {
            let target_edition = row.get_attribute("data-edition").unwrap_or_default();
            let direct_url = row.get_attribute("data-url").filter(|s| !s.is_empty());
            switch_edition(target_edition, direct_url);
        }
        "browse-source" => app::open_series(url),
        "season" => {
            STATE.with(|c| {
                c.borrow_mut().season = row
                    .get_attribute("data-season")
                    .unwrap_or_else(|| "1".into())
            });
            render_drawer();
        }
        "direct" => {
            let embedded = STATE.with(|c| c.borrow().embedded.is_some());
            app::close_sheet();
            if embedded || el("mobilePlayer").class_list().contains("player-error") {
                let settings = snapshot();
                play_direct(None, settings, true);
            }
        }
        "embed" => switch_mirror(index),
        "quality" => {
            let settings = snapshot();
            if let Some(ep) = current_episode() {
                if let Some(stream) = rows(&ep, "directStreams").get(index) {
                    let q = s(stream, "quality");
                    store("krmzi_quality_mode", "manual");
                    store("krmzi_pref_quality", &q);
                    app::close_sheet();
                    pill(&format!("تم تثبيت الجودة على {q}"));
                    play_direct(Some(index), settings, true);
                }
            }
        }
        "quality-auto" => {
            if hls_levels().len() < 2 {
                return;
            }
            store("krmzi_quality_mode", "auto");
            hls_level(-1);
            app::close_sheet();
            pill("تم تفعيل الجودة التلقائية (تكيف ديناميكي)");
        }
        "quality-default" => {
            store("krmzi_quality_mode", "auto");
            app::close_sheet();
            pill("تم تفعيل الجودة الافتراضية");
            play_direct(None, snapshot(), true);
        }
        "quality-hls" => {
            if let Some(height) = hls_levels().get(index) {
                hls_level(index as i32);
                store("krmzi_quality_mode", "manual");
                store("krmzi_pref_quality", &format!("{height}p"));
                app::close_sheet();
                pill(&format!("تم تثبيت الجودة على {height}p"));
            }
        }
        "audio-track" => {
            set_hls_audio_track(index as i32);
            app::close_sheet();
            pill("تم تغيير المسار الصوتي");
        }
        "sub-size" => {
            if let Some(sz) = row.get_attribute("data-size") {
                store("zoet_sub_size", &sz);
                apply_sub_size();
                app::close_sheet();
                pill("تم ضبط حجم الترجمة");
            }
        }
        "sub-color" => {
            if let Some(c) = row.get_attribute("data-color") {
                store("zoet_sub_color", &c);
                if let Some((_, label, hex)) = SUB_COLORS.iter().find(|(id, _, _)| id == &c) {
                    apply_sub_color(hex);
                    pill(&format!("لون الترجمة: {label}"));
                }
                app::close_sheet();
            }
        }
        "speed" => {
            if let Ok(rate) = row.get_attribute("data-rate").unwrap_or_default().parse::<f64>() {
                set_speed(rate);
                open_tools_sheet();
            }
        }
        "fit-mode" => {
            let mode = row.get_attribute("data-mode").unwrap_or_else(|| "contain".into());
            apply_fit_mode(&mode);
            open_tools_sheet();
        }
        "filter-preset" => {
            let fid = row.get_attribute("data-filter").unwrap_or_else(|| "normal".into());
            if let Some((id, label, css)) = FILTERS.iter().find(|(id, _, _)| *id == fid) {
                store("zoet_video_filter", id);
                apply_video_filter(css);
                sync_tool_states();
                pill(&format!("فلتر الصورة: {label}"));
                open_tools_sheet();
            }
        }
        "sub-toggle" => {
            toggle_subtitles();
        }
        "sub-delay" => {
            if let Ok(delta) = row.get_attribute("data-delta").unwrap_or_default().parse::<f64>() {
                adjust_sub_delay(delta);
                open_tools_sheet();
            }
        }
        "sub-delay-reset" => {
            reset_sub_delay();
            open_tools_sheet();
        }
        "sub-import" => {
            app::close_sheet();
            let _ = el("subFileInput").dyn_into::<HtmlElement>().map(|i| i.click());
        }
        "lock-touch" => {
            let locked = STATE.with(|c| c.borrow().locked);
            set_touch_locked(!locked);
        }
        "snapshot" => {
            app::close_sheet();
            take_snapshot();
        }
        "toggle-pip" => {
            app::close_sheet();
            pip();
        }
        "toggle-theater" => {
            app::close_sheet();
            toggle_theater();
        }
        "controls-help" => {
            controls_help();
        }
        _ => {}
    }
}
fn keyboard(event: Event) {
    let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
        return;
    };
    if !is_open() || event.ctrl_key() || event.alt_key() || event.meta_key() {
        return;
    }
    if event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .is_some_and(|e| {
            e.matches("input,textarea,select,[contenteditable=true]")
                .unwrap_or(false)
        })
    {
        return;
    }
    if !el("sheetBackdrop").class_list().contains("hidden") {
        return;
    }
    let key = event.key().to_lowercase();
    let tv = document()
        .document_element()
        .and_then(|e| e.get_attribute("data-display"))
        .as_deref()
        == Some("tv");
    if tv && ["arrowleft", "arrowright", "arrowup", "arrowdown"].contains(&key.as_str()) {
        event.prevent_default();
        remote_arrow(&key);
        return;
    }
    if tv
        && key == "enter"
        && event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|e| e.id() == "playerCanvas")
    {
        event.prevent_default();
        toggle();
        return;
    }
    let v = video();
    let mut handled = true;
    match key.as_str() {
        " " | "k" | "mediaplaypause" => {
            if key == " "
                && event
                    .target()
                    .and_then(|t| t.dyn_into::<Element>().ok())
                    .is_some_and(|e| e.tag_name() == "BUTTON")
            {
                return;
            }
            toggle();
        }
        "j" | "mediarewind" => seek(-10.),
        "l" | "mediafastforward" => seek(10.),
        "arrowleft" => seek(if event.shift_key() { -30. } else { -5. }),
        "arrowright" => seek(if event.shift_key() { 30. } else { 5. }),
        "g" => adjust_sub_delay(if event.shift_key() { -1.0 } else { -0.25 }),
        "h" => adjust_sub_delay(if event.shift_key() { 1.0 } else { 0.25 }),
        "m" => v.set_muted(!v.muted()),
        "f" => fullscreen(),
        "t" => toggle_theater(),
        "v" => cycle_video_filter(),
        "p" => pip(),
        "c" => {
            if event.shift_key() {
                cycle_sub_color();
            } else {
                toggle_subtitles();
            }
        }
        "q" => open_quality(),
        "s" => {
            if event.shift_key() {
                take_snapshot();
            } else {
                open_sources();
            }
        }
        "e" => all_episodes(),
        "n" | "mediatracknext" => next_episode(),
        "a" => fit(),
        "[" | "]" => {
            let rate = (v.playback_rate() + if key == "]" { 0.25 } else { -0.25 }).clamp(0.5, 2.);
            v.set_playback_rate(rate);
            pill(&format!("{rate}×"));
        }
        "?" | "/" => controls_help(),
        _ => {
            if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) && v.duration().is_finite()
            {
                v.set_current_time(key.parse::<f64>().unwrap_or(0.) / 10. * v.duration());
                progress();
            } else {
                handled = false;
            }
        }
    }
    if handled {
        event.prevent_default();
    }
}
pub(crate) fn remote_arrow(key: &str) {
    if el("playerHudTop").class_list().contains("hud-hidden") {
        if key == "arrowleft" || key == "arrowright" {
            seek(if key == "arrowleft" { -10. } else { 10. });
            for id in ["playerHudTop", "playerHudBottom", "hudCenter"] {
                class(id, "hud-hidden", true);
            }
            focus("playerCanvas");
            return;
        }
        show_hud(false);
        focus("centerPlay");
        return;
    }
    let Ok(nodes) =
        el("mobilePlayer").query_selector_all("button,input,select,[tabindex='0'],[role='slider']")
    else {
        return;
    };
    let nodes = (0..nodes.length())
        .filter_map(|index| nodes.item(index)?.dyn_into::<HtmlElement>().ok())
        .filter(|node| {
            node.offset_width() > 0
                && node.offset_height() > 0
                && !node.has_attribute("disabled")
                && node.closest(".hidden,.hud-hidden").ok().flatten().is_none()
        })
        .collect::<Vec<_>>();
    let Some(current) = document().active_element() else {
        focus("centerPlay");
        return;
    };
    if !nodes
        .iter()
        .any(|node| node.unchecked_ref::<Element>() == &current)
    {
        focus("centerPlay");
        return;
    }
    let rect = current.get_bounding_client_rect();
    let horizontal = key == "arrowleft" || key == "arrowright";
    let sign = if key == "arrowleft" || key == "arrowup" {
        -1.
    } else {
        1.
    };
    let mut ranked = nodes
        .iter()
        .filter(|node| node.unchecked_ref::<Element>() != &current)
        .filter_map(|node| {
            let other = node.get_bounding_client_rect();
            let dx = other.x() + other.width() / 2. - rect.x() - rect.width() / 2.;
            let dy = other.y() + other.height() / 2. - rect.y() - rect.height() / 2.;
            let distance = if horizontal { dx } else { dy } * sign;
            if distance <= 2. {
                return None;
            }
            let side = if horizontal { dy.abs() } else { dx.abs() };
            Some((distance + side * 1.5, node))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, next)) = ranked.first() {
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        let _ = next.focus_with_options(&options);
        next.scroll_into_view_with_bool(false);
    }
}
fn pointer_seek(event: &PointerEvent) {
    let rect = el("timelineBar").get_bounding_client_rect();
    let v = video();
    if v.duration().is_finite() && rect.width() > 0. {
        v.set_current_time(
            ((event.client_x() as f64 - rect.left()) / rect.width()).clamp(0., 1.) * v.duration(),
        );
        progress();
        show_hud(true);
    }
}
fn gestures() {
    on("gesturesLayer", "pointerdown", |event| {
        let Ok(event) = event.dyn_into::<PointerEvent>() else {
            return;
        };
        if STATE.with(|c| c.borrow().embedded.is_some()) {
            return;
        }
        if STATE.with(|c| c.borrow().locked) {
            reveal_unlock_pill();
            return;
        }
        let v = video();
        let gesture = Gesture {
            x: event.client_x() as f64,
            y: event.client_y() as f64,
            at: now(),
            moved: false,
            hold: false,
            previous_rate: v.playback_rate(),
        };
        let hold = Timeout::new(350, || {
            let enable = STATE.with(|c| {
                let mut p = c.borrow_mut();
                if p.gesture.moved || p.locked || video().paused() {
                    return false;
                }
                p.gesture.hold = true;
                true
            });
            if enable {
                video().set_playback_rate(2.);
                show("speed2xPill", true);
            }
        });
        STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.gesture = gesture;
            p.hold = Some(hold);
        });
        let _ = el("gesturesLayer").set_pointer_capture(event.pointer_id());
    });
    on("gesturesLayer", "pointermove", |event| {
        let Ok(event) = event.dyn_into::<PointerEvent>() else {
            return;
        };
        if !el("gesturesLayer").has_pointer_capture(event.pointer_id()) {
            return;
        }
        let gesture = STATE.with(|c| c.borrow().gesture.clone());
        if gesture.hold {
            return;
        }
        let dx = event.client_x() as f64 - gesture.x;
        let dy = event.client_y() as f64 - gesture.y;
        if !gesture.moved && (dx.abs() > 10. || dy.abs() > 10.) {
            STATE.with(|c| {
                let mut p = c.borrow_mut();
                p.gesture.moved = true;
                p.hold = None;
            });
        }
    });
    for name in ["pointerup", "pointercancel"] {
        let cancel = name == "pointercancel";
        on("gesturesLayer", name, move |event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let _ = el("gesturesLayer").release_pointer_capture(event.pointer_id());
            let gesture = STATE.with(|c| {
                let mut p = c.borrow_mut();
                p.hold = None;
                p.gesture.clone()
            });
            if gesture.hold {
                video().set_playback_rate(gesture.previous_rate);
                show("speed2xPill", false);
                return;
            }
            if cancel || gesture.moved || STATE.with(|c| c.borrow().locked) {
                return;
            }

            if is_hud_visible() {
                // Single tap on visible controls immediately hides everything
                hide_hud();
                STATE.with(|c| {
                    let mut p = c.borrow_mut();
                    p.tap_timer = None;
                    p.last_tap = 0.;
                });
            } else {
                // Controls are hidden: detect double tap to seek or single tap to reveal
                let double = STATE.with(|c| {
                    let mut p = c.borrow_mut();
                    let is_double = gesture.at - p.last_tap < 280. && p.last_tap > 0.;
                    p.last_tap = if is_double { 0. } else { gesture.at };
                    is_double
                });
                if double {
                    STATE.with(|c| {
                        c.borrow_mut().tap_timer = None;
                    });
                    let rect = el("gesturesLayer").get_bounding_client_rect();
                    let forward = gesture.x - rect.left() >= rect.width() / 2.;
                    seek(if forward { 10. } else { -10. });
                } else {
                    let timer = Timeout::new(220, || {
                        show_hud(true);
                    });
                    STATE.with(|c| {
                        c.borrow_mut().tap_timer = Some(timer);
                    });
                }
            }
        });
    }
}
fn bind_video_events() {
    on("nativePlayer", "loadedmetadata", |_| {
        if is_open() {
            metadata();
        }
    });
    on("nativePlayer", "resize", |_| sync_video_quality());
    on("nativePlayer", "canplay", |_| {
        if is_open() {
            ready(false);
        }
    });
    on("nativePlayer", "playing", |_| {
        if is_open() && STATE.with(|c| c.borrow().embedded.is_none()) {
            ready(true);
        }
    });
    on("nativePlayer", "play", |_| {
        html("centerPlay", icons::PAUSE);
        bridge("setVideoPlaying", true.into());
    });
    on("nativePlayer", "pause", |_| {
        html("centerPlay", icons::PLAY);
        bridge("setVideoPlaying", false.into());
    });
    on("nativePlayer", "timeupdate", |_| progress());
    on("nativePlayer", "progress", |_| progress());
    on("nativePlayer", "ended", |_| ended());
    on("nativePlayer", "error", |_| {
        if is_open() && STATE.with(|c| c.borrow().embedded.is_none()) {
            let (session, attempt) = STATE.with(|c| {
                let p = c.borrow();
                (p.session, p.attempt)
            });
            schedule_failure(session, attempt);
        }
    });
    on("nativePlayer", "waiting", |_| {
        if is_open() && !video().paused() && STATE.with(|c| c.borrow().embedded.is_none()) {
            loading("جاري تحميل الفيديو…");
        }
    });
}
pub fn init() {
    on("dismissPlayerBtn", "click", |_| close());
    on("centerPlay", "click", |_| toggle());
    on("centerRewind", "click", |_| {
        seek(-10.);
        show_hud(true);
    });
    on("centerForward", "click", |_| {
        seek(10.);
        show_hud(true);
    });
    on("skipIntroBtn", "click", |_| {
        let v = video();
        if let Some(end) = intro_end(&v) {
            v.set_current_time(end);
            progress();
        }
    });
    on("openServersSheetBtn", "click", |_| open_sources());
    on("openQualitySheetBtn", "click", |_| open_quality());
    on("openEpisodesSheetBtn", "click", |_| all_episodes());
    on("toggleSubtitlesBtn", "click", |_| toggle_subtitles());
    on("toggleTheaterBtn", "click", |_| toggle_theater());
    on("toggleFullscreenBtn", "click", |_| fullscreen());
    on("togglePipBtn", "click", |_| pip());
    on("toggleAspectRatioBtn", "click", |_| fit());
    on("playerMoreToolsBtn", "click", |_| open_tools_sheet());
    on("unlockTouchBtn", "click", |_| set_touch_locked(false));
    listen(document().as_ref(), "fullscreenchange", |_| sync_tool_states());
    on("playerFavBtn", "click", |_| {
        if let Some(ep) = current_episode() {
            app::toggle_favorite(&ep);
            title_ui(&ep);
        }
    });
    on("drawerEpisodeSearch", "input", |_| render_drawer());
    on("autoNextPlayBtn", "click", |_| next_episode());
    on("autoNextCancelBtn", "click", |_| {
        STATE.with(|c| {
            let mut p = c.borrow_mut();
            p.next_deadline = None;
            p.next_cancelled = true;
        });
        show("autoNextCard", false);
    });
    bind_video_events();
    on("timelineBar", "pointerdown", |event| {
        if let Ok(event) = event.dyn_into::<PointerEvent>() {
            event.prevent_default();
            let _ = el("timelineBar").set_pointer_capture(event.pointer_id());
            pointer_seek(&event);
        }
    });
    on("timelineBar", "pointermove", |event| {
        if let Ok(event) = event.dyn_into::<PointerEvent>() {
            let rect = el("timelineBar").get_bounding_client_rect();
            let v = video();
            if v.duration().is_finite() && rect.width() > 0. {
                let ratio = ((event.client_x() as f64 - rect.left()) / rect.width()).clamp(0., 1.);
                let hover_time = ratio * v.duration();
                text("timelineHoverTime", &format_time(hover_time));
                style("timelineHoverTime", "left", &format!("{:.1}%", ratio * 100.));
                show("timelineHoverTime", true);
            }
            if el("timelineBar").has_pointer_capture(event.pointer_id()) {
                pointer_seek(&event);
            }
        }
    });
    on("timelineBar", "pointerleave", |_| {
        show("timelineHoverTime", false);
    });
    on("timelineBar", "pointerup", |event| {
        if let Ok(event) = event.dyn_into::<PointerEvent>() {
            let _ = el("timelineBar").release_pointer_capture(event.pointer_id());
        }
    });
    init_sub_drag_and_drop();
    for event in ["pointerleave", "focusout"] {
        on("playerHudBottom", event, |_| {
            if is_open()
                && !el("playerHudBottom").class_list().contains("hud-hidden")
                && !STATE.with(|c| c.borrow().locked)
            {
                show_hud(true);
            }
        });
    }
    on("playerCanvas", "pointermove", |event| {
        if let Ok(event) = event.dyn_into::<PointerEvent>() {
            if event.pointer_type() == "mouse" {
                show_hud(true);
            }
        }
    });
    on("timelineBar", "keydown", |event| {
        if let Ok(event) = event.dyn_into::<KeyboardEvent>() {
            if ["ArrowLeft", "ArrowRight"].contains(&event.key().as_str()) {
                event.prevent_default();
                event.stop_propagation();
                seek(if event.key() == "ArrowLeft" { -5. } else { 5. });
            }
        }
    });
    listen(document().as_ref(), "click", |event| {
        if let Some(row) = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|e| e.closest("[data-player-action]").ok().flatten())
        {
            event.prevent_default();
            action(row);
        }
    });
    listen(document().as_ref(), "keydown", keyboard);
    listen(window().as_ref(), "pagehide", |_| save_history(true));
    gestures();
    let timer = Interval::new(1000, tick);
    STATE.with(|c| c.borrow_mut().watchdog = Some(timer));
    let fit_mode = storage("otium_fit_mode").unwrap_or_else(|| "contain".into());
    class("playerCanvas", &format!("fit-{fit_mode}"), true);
    if storage("zoet_player_wide").as_deref() == Some("1") {
        class("mobilePlayer", "wide-mode", true);
    }
    apply_sub_size();
    let filter = storage("zoet_video_filter").unwrap_or_else(|| "normal".into());
    if let Some((_, _, css)) = FILTERS.iter().find(|(id, _, _)| id == &filter) {
        apply_video_filter(css);
    }
    let sub_color = storage("zoet_sub_color").unwrap_or_else(|| "yellow".into());
    if let Some((_, _, hex)) = SUB_COLORS.iter().find(|(id, _, _)| id == &sub_color) {
        apply_sub_color(hex);
    }
    sync_tool_states();
}
