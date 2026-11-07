use crate::{api, catalogue, dom::*, player};
use gloo_timers::callback::Timeout;
use js_sys::{Function, Reflect};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{AbortController, Element, Event, HtmlElement, HtmlInputElement, KeyboardEvent};

#[derive(Default)]
struct Page {
    items: Vec<Value>,
    next: Option<u32>,
    failed: bool,
}
#[derive(Default)]
struct Listing {
    pages: HashMap<String, Page>,
    generation: u64,
    pending: usize,
    abort: Option<AbortController>,
    query: String,
    source: String,
    needs_refresh: bool,
}
struct State {
    view: String,
    source: String,
    main: Listing,
    search: Listing,
    series: Vec<Value>,
    context: Value,
    history: Vec<Value>,
    favorites: Vec<Value>,
    sheet_generation: u64,
    sheet_origin: Option<HtmlElement>,
    search_timer: Option<Timeout>,
    history_saved: f64,
    library_mode: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            view: "series".into(),
            source: storage("zoet_source")
                .or_else(|| storage("otium_source"))
                .filter(|s| ["all", "krmzi", "cinemana"].contains(&s.as_str()))
                .unwrap_or("all".into()),
            main: Listing::default(),
            search: Listing::default(),
            series: vec![],
            context: Value::Null,
            history: read_records("krmzi_hist"),
            favorites: read_records("krmzi_favs"),
            sheet_generation: 0,
            sheet_origin: None,
            search_timer: None,
            history_saved: 0.,
            library_mode: "hist".into(),
        }
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
fn read_records(key: &str) -> Vec<Value> {
    storage(key)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
pub fn source_name(source: &str) -> &str {
    if source == "cinemana" {
        "سينمانا"
    } else {
        "قرمزي"
    }
}
pub fn notify(message: &str) {
    text("appToast", message);
    show("appToast", true);
    Timeout::new(3500, || show("appToast", false)).forget();
}
pub fn open_sheet(title: &str, markup: &str) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if el("sheetBackdrop").class_list().contains("hidden") {
            s.sheet_origin = document().active_element().and_then(|e| e.dyn_into().ok());
        }
    });
    text("sheetTitle", title);
    html("sheetBody", markup);
    show("sheetBackdrop", true);
    focus("closeSheetBtn");
}
pub fn close_sheet() {
    player::sheet_closed();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.sheet_generation += 1;
        show("sheetBackdrop", false);
        if let Some(origin) = s.sheet_origin.take() {
            let _ = origin.focus();
        }
    });
}
pub fn known_series() -> Vec<Value> {
    STATE.with(|state| {
        let state = state.borrow();
        let mut items = state.series.clone();
        items.extend(
            list_items(&state.search)
                .into_iter()
                .filter(|item| s(item, "type") == "series"),
        );
        catalogue::merge(
            &items,
            &storage("otium_preferred_source").unwrap_or("cinemana".into()),
        )
    })
}
pub fn episode_context(url: &str) -> Value {
    STATE.with(|state| {
        let s = state.borrow();
        let cards = list_items(&s.search)
            .into_iter()
            .chain(list_items(&s.main))
            .collect::<Vec<_>>();
        let known = s.context["episodes"]
            .as_array()
            .and_then(|rows| rows.iter().find(|v| self::s(v, "url") == url))
            .cloned()
            .or_else(|| {
                s.history
                    .iter()
                    .chain(&s.favorites)
                    .chain(&cards)
                    .find(|v| self::s(v, "url") == url)
                    .cloned()
            })
            .unwrap_or(json!({"url":url}));
        let mut item = known;
        if s.context["episodes"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|v| self::s(v, "url") == url))
        {
            item["seriesUrl"] = s.context["url"].clone();
            item["seriesTitle"] = s.context["title"].clone();
            let artwork = ["poster", "thumbnail", "artwork"]
                .iter()
                .map(|key| self::s(&s.context, key))
                .find(|value| !value.is_empty());
            if let Some(artwork) = artwork {
                item["thumbnail"] = json!(artwork);
            }
            item["offers"] = s.context["offers"].clone();
        }
        // Series context has current metadata/artwork; history has the newest
        // position. Keep both when an episode is reopened from its own sheet.
        if let Some(progress) = s
            .history
            .iter()
            .find(|record| self::s(record, "url") == url)
        {
            for key in ["currentTime", "duration", "timestamp"] {
                if !progress[key].is_null() {
                    item[key] = progress[key].clone();
                }
            }
        }
        item
    })
}
pub fn save_player_history(item: &Value, time: f64, duration: f64) {
    if time < 1. || !time.is_finite() || !duration.is_finite() {
        return;
    }
    let now = js_sys::Date::now();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.history_saved = now;
        let mut record = stored_item(item);
        record["currentTime"] = json!(time);
        record["duration"] = json!(duration);
        record["timestamp"] = json!(now);
        let url = s(item, "url");
        state.history.retain(|v| s(v, "url") != url);
        state.history.insert(0, record);
        state.history.truncate(60);
        store(
            "krmzi_hist",
            &serde_json::to_string(&state.history).unwrap_or_default(),
        );
    });
}
pub fn is_favorite(item: &Value) -> bool {
    STATE.with(|state| {
        state
            .borrow()
            .favorites
            .iter()
            .any(|v| s(v, "url") == s(item, "url"))
    })
}
fn stored_item(item: &Value) -> Value {
    let mut record = json!({});
    for key in [
        "url",
        "title",
        "seriesTitle",
        "seriesUrl",
        "episodeNum",
        "season",
        "source",
        "mediaType",
        "aliases",
        "year",
        "edition",
    ] {
        if !item[key].is_null() {
            record[key] = item[key].clone();
        }
    }
    record["type"] = json!("episode");
    record["thumbnail"] = item["artwork"].clone();
    record["artwork"] = item["artwork"].clone();
    record
}
pub fn toggle_favorite(item: &Value) {
    let exists = is_favorite(item);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let url = s(item, "url");
        state.favorites.retain(|v| s(v, "url") != url);
        if !exists {
            state.favorites.insert(0, stored_item(item));
        }
        store(
            "krmzi_favs",
            &serde_json::to_string(&state.favorites).unwrap_or_default(),
        );
    });
    notify(if exists {
        "أُزيل من المفضلة"
    } else {
        "أُضيف إلى المفضلة"
    });
}
fn list_items(list: &Listing) -> Vec<Value> {
    let records = ["krmzi", "cinemana"]
        .iter()
        .flat_map(|source| {
            list.pages
                .get(*source)
                .map(|page| page.items.clone())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    catalogue::merge(
        &records,
        &storage("otium_preferred_source").unwrap_or("cinemana".into()),
    )
}
fn card(item: &Value) -> String {
    let kind = if s(item, "type") == "series" {
        "series"
    } else {
        "episode"
    };
    let thumbnail = s(item, "thumbnail");
    let sources = item["sources"]
        .as_array()
        .cloned()
        .unwrap_or(vec![json!(catalogue::provider(item))]);
    let has_both = item["has_both_editions"].as_bool().unwrap_or(false)
        || item["editions"]
            .as_array()
            .map_or(false, |eds| eds.len() > 1);
    let is_dubbed = catalogue::edition(item) == "dubbed";
    let edition_badge = if has_both {
        "<span class=\"edition-badge edition-multi\"><i class=\"fa-solid fa-layer-group\"></i> مترجم ومدبلج</span>"
    } else if is_dubbed {
        "<span class=\"edition-badge edition-dubbed\"><i class=\"fa-solid fa-microphone\"></i> مدبلج</span>"
    } else {
        "<span class=\"edition-badge edition-subbed\"><i class=\"fa-solid fa-closed-captioning\"></i> مترجم</span>"
    };
    let badges = sources
        .iter()
        .filter_map(Value::as_str)
        .map(|source| {
            format!(
                "<span class=\"provider-tag provider-{source}\">{}</span>",
                source_name(source)
            )
        })
        .collect::<String>();
    let year = s(item, "year");
    let rating = s(item, "rating");
    format!("<button class=\"series-tile\" data-action=\"{kind}\" data-url=\"{}\"><span class=\"poster-frame\"><img class=\"series-tile-thumb\" src=\"{}\" alt=\"\" loading=\"lazy\" decoding=\"async\" referrerpolicy=\"no-referrer\">{edition_badge}</span><span class=\"series-tile-body\"><span class=\"series-tile-title\">{}</span><span class=\"series-tile-sub\">{}{}</span><span class=\"catalogue-sources\">{badges}</span></span></button>",esc(&s(item,"url")),esc(&thumbnail),esc(&s(item,"title")),esc(&year),if rating.is_empty(){String::new()}else{format!(" · ★ {}",esc(&rating))})
}
fn render_grid(id: &str, items: &[Value]) {
    let grid = el(id);
    let old = grid.children();
    let mut nodes = HashMap::new();
    for i in 0..old.length() {
        if let Some(child) = old.item(i) {
            if let Some(url) = child.get_attribute("data-url") {
                nodes.insert(url, child);
            }
        }
    }
    let mut retained = HashSet::new();
    let mut previous: Option<Element> = None;
    let holder = document().create_element("div").unwrap();
    for item in items {
        let url = s(item, "url");
        let markup = card(item);
        let node = if let Some(node) = nodes
            .get(&url)
            .filter(|node| node.get_attribute("data-card").as_deref() == Some(&markup))
        {
            node.clone()
        } else {
            holder.set_inner_html(&markup);
            let node = holder.first_element_child().unwrap();
            let _ = node.set_attribute("data-card", &markup);
            if let Some(old) = nodes.get(&url) {
                let focused = document().active_element().as_ref() == Some(old);
                let _ = grid.replace_child(&node, old);
                if focused {
                    let _ = node.clone().dyn_into::<HtmlElement>().unwrap().focus();
                }
            }
            node
        };
        let next = previous
            .as_ref()
            .and_then(Element::next_element_sibling)
            .or_else(|| {
                if previous.is_none() {
                    grid.first_element_child()
                } else {
                    None
                }
            });
        if next.as_ref() != Some(&node) {
            let _ = grid.insert_before(&node, next.as_ref().map(|e| e.as_ref()));
        }
        previous = Some(node);
        retained.insert(url);
    }
    let children = grid.children();
    for i in (0..children.length()).rev() {
        if let Some(child) = children.item(i) {
            if child
                .get_attribute("data-url")
                .is_none_or(|url| !retained.contains(&url))
            {
                child.remove();
            }
        }
    }
    if items.is_empty() {
        grid.set_inner_html("<p class=\"empty-note\">لا توجد نتائج متاحة.</p>");
    }
}
fn render_listing(search: bool) {
    let (items, pending, failed, more, view) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let list = if search { &state.search } else { &state.main };
        let items = list_items(list);
        let pending = list.pending;
        let failed = list
            .pages
            .iter()
            .filter(|(_, p)| p.failed)
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        let more = list.pages.values().any(|p| p.next.is_some());
        if !search && state.view == "series" {
            state.series = items.clone();
        }
        (items, pending, failed, more, state.view.clone())
    });
    let grid = if search { "searchGrid" } else { "seriesGrid" };
    if pending == 0 || !items.is_empty() {
        render_grid(grid, &items);
    } else {
        html(grid,&(0..8).map(|_|"<div class=\"series-tile skeleton-poster skeleton-cell\" aria-hidden=\"true\"></div>").collect::<String>());
    }
    let _ = el(grid).set_attribute("aria-busy", if pending > 0 { "true" } else { "false" });
    let button = if search {
        "loadMoreSearchBtn"
    } else {
        "loadMoreSeriesBtn"
    };
    show(button, more);
    let _ = el(button).toggle_attribute_with_force("disabled", pending > 0);
    text(
        button,
        if pending > 0 {
            "جاري التحميل…"
        } else if !failed.is_empty() {
            "إعادة المحاولة"
        } else {
            "عرض المزيد"
        },
    );
    let status = if pending > 0 {
        "جاري تحميل النتائج…".into()
    } else if !failed.is_empty() {
        format!(
            "تعذّر الاتصال بـ{}. النتائج المتاحة محفوظة.",
            failed
                .iter()
                .map(|s| source_name(s))
                .collect::<Vec<_>>()
                .join(" و")
        )
    } else {
        format!(
            "{} {}{}",
            items.len(),
            if search {
                "نتيجة"
            } else if view == "movies" {
                "فيلم"
            } else {
                "مسلسل"
            },
            if more {
                " · المزيد متاح"
            } else {
                ""
            }
        )
    };
    text(
        if search {
            "searchSourceStatus"
        } else {
            "cataloguePageStatus"
        },
        &status,
    );
    if !search {
        text("seriesCountSub", &items.len().to_string());
        let is_offline = items.is_empty() && pending == 0 && !failed.is_empty();
        show("offlineState", is_offline);
        if is_offline && view == "movies" {
            text("offlineTitle", "أفلام سينمانا غير متاحة على اتصالك");
            text(
                "offlineDesc",
                "تتطلب الأفلام الاتصال بشبكة إيرثلنك أو VPN عراقي. مسلسلات قرمزي تعمل على كافة الشبكات.",
            );
            show("switchToSeriesBtn", true);
        } else if is_offline {
            text("offlineTitle", "تعذّر الوصول إلى المصادر");
            text(
                "offlineDesc",
                "تحقّق من الاتصال وأعد المحاولة. توفّر سينمانا يعتمد على مزوّد الإنترنت.",
            );
            show("switchToSeriesBtn", false);
        }
    }
}
fn load_listing(search: bool, reset: bool) {
    let tasks = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let source = state.source.clone();
        let mode = state.view.clone();
        let list = if search {
            &mut state.search
        } else {
            &mut state.main
        };
        if reset {
            if let Some(abort) = list.abort.take() {
                abort.abort();
            }
            list.generation += 1;
            list.pending = 0;
            list.pages.clear();
            list.abort = AbortController::new().ok();
            list.source = source.clone();
            list.needs_refresh = false;
            let providers = if !search && mode == "movies" {
                vec!["cinemana"]
            } else if source == "all" {
                vec!["krmzi", "cinemana"]
            } else {
                vec![source.as_str()]
            };
            for provider in providers {
                list.pages.insert(
                    provider.into(),
                    Page {
                        next: Some(1),
                        ..Page::default()
                    },
                );
            }
        }
        if list.pending > 0 {
            return vec![];
        }
        let generation = list.generation;
        let query = list.query.clone();
        let signal = list.abort.as_ref().map(|a| a.signal());
        let tasks = list
            .pages
            .iter()
            .filter_map(|(provider, page)| {
                page.next.map(|next| {
                    (
                        provider.clone(),
                        next,
                        generation,
                        query.clone(),
                        signal.clone(),
                        mode.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        list.pending = tasks.len();
        tasks
    });
    render_listing(search);
    for (source, page, generation, query, signal, mode) in tasks {
        spawn_local(async move {
            let endpoint = if search {
                "/api/search"
            } else if mode == "movies" {
                "/api/episodes"
            } else {
                "/api/series"
            };
            let mut params = vec![("page", page.to_string())];
            if search {
                params.push(("q", query));
            }
            let result =
                api::call_with_signal(endpoint, &params, Some(&source), signal.as_ref()).await;
            let accepted = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let list = if search {
                    &mut state.search
                } else {
                    &mut state.main
                };
                if list.generation != generation || signal.as_ref().is_some_and(|s| s.aborted()) {
                    return false;
                }
                list.pending = list.pending.saturating_sub(1);
                let provider = list.pages.get_mut(&source).unwrap();
                let key = if search {
                    "results"
                } else if mode == "movies" {
                    "episodes"
                } else {
                    "series"
                };
                match result {
                    Ok(data) if data[key].is_array() && data["error"].is_null() => {
                        let seen = provider
                            .items
                            .iter()
                            .map(|v| s(v, "url"))
                            .collect::<HashSet<_>>();
                        let added = data[key]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|v| !s(v, "url").is_empty() && !seen.contains(&s(v, "url")))
                            .map(|v| {
                                let mut v = v.clone();
                                v["source"] = json!(source);
                                if mode == "movies" && !search {
                                    v["mediaType"] = json!("movie");
                                }
                                v
                            })
                            .collect::<Vec<_>>();
                        provider.next =
                            if !added.is_empty() && data["hasNext"].as_bool() == Some(true) {
                                Some(
                                    (data["nextPage"].as_u64().unwrap_or((page + 1) as u64) as u32)
                                        .max(page + 1),
                                )
                            } else {
                                None
                            };
                        provider.items.extend(added);
                        // cap stored items per-provider to 480 to prevent unbounded memory growth
                        if provider.items.len() > 480 {
                            provider.items.drain(..provider.items.len() - 480);
                        }
                        provider.failed = false;
                    }
                    _ => {
                        provider.failed = true;
                    }
                }
                true
            });
            if accepted {
                render_listing(search);
            }
        });
    }
}
fn set_view(view: &str) {
    STATE.with(|state| state.borrow_mut().view = view.into());
    show("libraryView", view == "library");
    show("seriesSection", view != "library");
    show("sourceSwitcher", view != "library");
    text(
        "catalogueTitle",
        if view == "movies" {
            "الأفلام"
        } else {
            "المسلسلات"
        },
    );
    let _ = document()
        .document_element()
        .unwrap()
        .set_attribute("data-view", view);
    if let Ok(Some(label)) = el("sourceSwitcher").query_selector(".source-label") {
        label.set_text_content(Some(if view == "movies" {
            "أفلام سينمانا · التوفّر حسب اتصالك"
        } else {
            "المصادر"
        }));
    }
    for id in ["tabSeries", "tabMovies"] {
        let active =
            (id == "tabSeries" && view == "series") || (id == "tabMovies" && view == "movies");
        let _ = el(id).set_attribute("aria-selected", &active.to_string());
        let _ = el(id).class_list().toggle_with_force("active", active);
    }
    if view == "library" {
        render_library();
    } else {
        load_listing(false, true);
    }
    el("contentCanvas").set_scroll_top(0);
}
fn render_library() {
    let records = STATE.with(|state| {
        let state = state.borrow();
        if state.library_mode == "favs" {
            state.favorites.clone()
        } else {
            state.history.clone()
        }
    });
    render_grid("libraryGrid", &records);
    let mode = STATE.with(|state| state.borrow().library_mode.clone());
    if let Ok(buttons) = el("libraryView").query_selector_all("[data-library]") {
        for i in 0..buttons.length() {
            if let Some(button) = buttons.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let active = button.get_attribute("data-library").as_deref() == Some(&mode);
                let _ = button.class_list().toggle_with_force("active", active);
                let _ = button.set_attribute("aria-pressed", &active.to_string());
            }
        }
    }
}
fn search_open() {
    show("searchOverlay", true);
    focus("overlaySearchInput");
    let query = el("overlaySearchInput")
        .dyn_into::<HtmlInputElement>()
        .unwrap()
        .value()
        .trim()
        .to_string();
    let refresh = STATE.with(|state| {
        let state = state.borrow();
        state.search.needs_refresh
            || state.search.source != state.source
            || state.search.query != query
            || state.search.pages.is_empty()
    });
    if !query.is_empty() && refresh {
        schedule_search(true);
    }
}
fn search_close() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let cancelled = state.search_timer.take().is_some() || state.search.pending > 0;
        state.search.needs_refresh |= cancelled;
        state.search.pending = 0;
        state.search.generation += 1;
        if let Some(abort) = state.search.abort.take() {
            abort.abort();
        }
    });
    show("searchOverlay", false);
    focus("openSearchBtn");
}
fn schedule_search(immediate: bool) {
    let query = el("overlaySearchInput")
        .dyn_into::<HtmlInputElement>()
        .unwrap()
        .value()
        .trim()
        .to_string();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.search_timer = None;
        state.search.query = query.clone();
        state.search.needs_refresh = true;
        if let Some(abort) = state.search.abort.take() {
            abort.abort();
        }
        state.search.generation += 1;
        state.search.pending = 0;
    });
    show("searchInitialState", query.is_empty());
    show("searchGrid", !query.is_empty());
    show("loadMoreSearchBtn", false);
    text("searchSourceStatus", "");
    if query.is_empty() {
        html("searchGrid", "");
        return;
    }
    if immediate {
        load_listing(true, true);
    } else {
        STATE.with(|state| {
            state.borrow_mut().search_timer = Some(Timeout::new(260, || load_listing(true, true)))
        });
    }
}
pub fn open_series(url: String) {
    let token = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.sheet_generation += 1;
        state.sheet_generation
    });
    open_sheet("تفاصيل المسلسل","<div class=\"detail-loading\"><div class=\"spinner-ring\"></div><p>جاري تحميل الحلقات…</p></div>");
    spawn_local(async move {
        let response = api::call("/api/series/detail", &[("url", url.clone())], None).await;
        if STATE.with(|state| state.borrow().sheet_generation) != token {
            return;
        }
        match response {
            Ok(mut series) if series["episodes"].is_array() => {
                let group = known_series().into_iter().find(|item| {
                    s(item, "url") == url
                        || item["offers"]
                            .as_array()
                            .is_some_and(|o| o.iter().any(|v| s(v, "url") == url))
                });
                if let Some(group) = group {
                    series["offers"] = group["offers"].clone();
                    if !group["editions"].is_null() {
                        series["editions"] = group["editions"].clone();
                    }
                    if !group["has_both_editions"].is_null() {
                        series["has_both_editions"] = group["has_both_editions"].clone();
                    }
                }
                if s(&series, "url").is_empty() {
                    series["url"] = json!(url);
                }
                STATE.with(|state| state.borrow_mut().context = series.clone());
                render_series_detail(&series);
            }
            _ => open_sheet(
                "تعذّر تحميل الحلقات",
                &format!(
                    "<p class=\"empty-note\">المصدر غير متاح على اتصالك الآن.</p><button class=\"load-more-btn\" data-action=\"series\" data-url=\"{}\">إعادة المحاولة</button>",
                    esc(&url)
                ),
            ),
        }
    });
}
fn render_series_detail(series: &Value) {
    let episodes = series["episodes"].as_array().cloned().unwrap_or_default();
    let mut seasons = episodes
        .iter()
        .map(|ep| {
            let value = s(ep, "season");
            if value.is_empty() {
                "1".into()
            } else {
                value
            }
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    seasons.sort_by_key(|s| s.parse::<u32>().unwrap_or(u32::MAX));
    let tabs = seasons
        .iter()
        .map(|season| {
            format!(
                "<button class=\"season-tab{}\" data-action=\"season\" data-season=\"{}\">الموسم {}</button>",
                if Some(season) == seasons.first() {
                    " active"
                } else {
                    ""
                },
                esc(season),
                esc(season)
            )
        })
        .collect::<String>();
    let offers = series["offers"].as_array().cloned().unwrap_or_default();
    let ed = catalogue::edition(series);
    let is_dubbed = ed == "dubbed";
    let clean_title = catalogue::clean_series_title(&s(series, "title"));

    let subbed_offer = offers.iter().find(|o| catalogue::edition(o) == "original");
    let dubbed_offer = offers.iter().find(|o| catalogue::edition(o) == "dubbed");
    let has_both = (subbed_offer.is_some() && dubbed_offer.is_some())
        || series["has_both_editions"].as_bool().unwrap_or(false);

    let subbed_url = if !is_dubbed {
        s(series, "url")
    } else {
        subbed_offer.map(|o| s(o, "url")).unwrap_or_default()
    };
    let dubbed_url = if is_dubbed {
        s(series, "url")
    } else {
        dubbed_offer.map(|o| s(o, "url")).unwrap_or_default()
    };

    let edition_switcher = format!(
        "<div class=\"series-edition-tabs\">\
            <button class=\"series-edition-tab{}\" data-action=\"series-edition\" data-edition=\"original\" data-url=\"{}\" data-title=\"{}\">\
                <i class=\"fa-solid fa-closed-captioning\"></i> النسخة المترجمة\
            </button>\
            <button class=\"series-edition-tab{}\" data-action=\"series-edition\" data-edition=\"dubbed\" data-url=\"{}\" data-title=\"{}\">\
                <i class=\"fa-solid fa-microphone\"></i> النسخة المدبلجة\
            </button>\
        </div>",
        if !is_dubbed { " active" } else { "" },
        esc(&subbed_url),
        esc(&clean_title),
        if is_dubbed { " active" } else { "" },
        esc(&dubbed_url),
        esc(&clean_title)
    );

    let current_ed_offers: Vec<&Value> = offers
        .iter()
        .filter(|o| (catalogue::edition(o) == "dubbed") == is_dubbed)
        .collect();
    let providers = if current_ed_offers.len() > 1 {
        format!(
            "<div class=\"series-provider-picker\"><span>المصدر</span>{}</div>",
            current_ed_offers
                .iter()
                .map(|offer| {
                    format!(
                        "<button class=\"series-provider-option{}\" data-action=\"series\" data-series-source=\"{}\" data-url=\"{}\">{}</button>",
                        if s(offer, "url") == s(series, "url") { " active" } else { "" },
                        esc(&s(offer, "url")),
                        esc(&s(offer, "url")),
                        source_name(catalogue::provider(offer))
                    )
                })
                .collect::<String>()
        )
    } else {
        String::new()
    };

    let poster = if s(series, "poster").is_empty() {
        s(series, "thumbnail")
    } else {
        s(series, "poster")
    };
    let story_toggle = if s(series, "story").chars().count() > 150 {
        "<button class=\"story-expand\" data-action=\"story\" aria-expanded=\"false\">قراءة القصة</button>"
    } else {
        ""
    };
    let ed_pill = if has_both {
        "<span class=\"series-edition-pill multi\"><i class=\"fa-solid fa-layer-group\"></i> مترجم ومدبلج</span>"
    } else if is_dubbed {
        "<span class=\"series-edition-pill dubbed\"><i class=\"fa-solid fa-microphone\"></i> مدبلج</span>"
    } else {
        "<span class=\"series-edition-pill subbed\"><i class=\"fa-solid fa-closed-captioning\"></i> مترجم</span>"
    };

    open_sheet(
        "تفاصيل المسلسل",
        &format!(
            "<div class=\"series-detail-hero\">\
                <img class=\"series-detail-poster\" src=\"{}\" alt=\"\" referrerpolicy=\"no-referrer\">\
                <div class=\"series-detail-shade\"></div>\
                <div class=\"series-detail-info\">\
                    <div class=\"series-badges-row\">\
                        <span class=\"series-source-tag\">{}</span>\
                        {}\
                    </div>\
                    <h4>{}</h4>\
                    {}\
                </div>\
            </div>\
            {}\
            <p class=\"series-detail-story\" id=\"seriesStory\">{}</p>\
            {}\
            <div class=\"series-episode-heading\"><h5>الحلقات</h5><span>{}</span></div>\
            <div class=\"season-tabs\" id=\"sheetSeasonTabs\">{tabs}</div>\
            <label class=\"episode-search\"><i class=\"fa-solid fa-magnifying-glass\"></i>\
                <input id=\"sheetEpisodeSearch\" type=\"search\" placeholder=\"ابحث عن حلقة\" aria-label=\"ابحث عن حلقة\">\
            </label>\
            <div class=\"sheet-episode-grid episode-card-list\" id=\"seriesEpisodeList\" data-season=\"{}\"></div>",
            esc(&poster),
            source_name(catalogue::provider(series)),
            ed_pill,
            esc(&clean_title),
            edition_switcher,
            providers,
            esc(&s(series, "story")),
            story_toggle,
            episodes.len(),
            esc(seasons.first().map(String::as_str).unwrap_or("1"))
        ),
    );
    render_detail_episodes();
}
fn switch_detail_edition(target_edition: String, title: String) {
    let token = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.sheet_generation += 1;
        state.sheet_generation
    });
    let label = if target_edition == "dubbed" { "المدبلجة" } else { "المترجمة" };
    open_sheet(
        "تفاصيل المسلسل",
        &format!("<div class=\"detail-loading\"><div class=\"spinner-ring\"></div><p>جاري البحث عن النسخة {}…</p></div>", label),
    );
    spawn_local(async move {
        let clean = catalogue::clean_series_title(&title);
        let dummy = json!({"title": clean.clone(), "edition": target_edition, "type": "series"});
        let known = known_series();
        if let Some(found) = catalogue::find_counterpart_with_edition(&dummy, &known, Some(&target_edition)) {
            let url = if !s(&found, "seriesUrl").is_empty() {
                s(&found, "seriesUrl")
            } else {
                s(&found, "url")
            };
            if !url.is_empty() {
                open_series(url);
                return;
            }
        }
        let preferred = storage("otium_preferred_source").unwrap_or("cinemana".into());
        let other = if preferred == "cinemana" { "krmzi" } else { "cinemana" };
        let mut queries = vec![clean.clone()];
        if target_edition == "dubbed" {
            queries.push(format!("{clean} مدبلج"));
            queries.push(format!("مسلسل {clean} مدبلج"));
        } else {
            queries.push(format!("{clean} مترجم"));
            queries.push(format!("مسلسل {clean}"));
        }
        let current_series = STATE.with(|state| state.borrow().context.clone());
        if let Some(aliases) = current_series["aliases"].as_array() {
            for a in aliases.iter().filter_map(Value::as_str) {
                let clean_a = catalogue::clean_series_title(a);
                if !clean_a.is_empty() && !queries.contains(&clean_a) {
                    queries.push(clean_a.clone());
                    if target_edition == "dubbed" {
                        queries.push(format!("{clean_a} مدبلج"));
                    }
                }
            }
        }
        for key in ["en_title", "other_title"] {
            let val = s(&current_series, key);
            if !val.is_empty() {
                let clean_val = catalogue::clean_series_title(&val);
                if !clean_val.is_empty() && !queries.contains(&clean_val) {
                    queries.push(clean_val.clone());
                    if target_edition == "dubbed" {
                        queries.push(format!("{clean_val} مدبلج"));
                    }
                }
            }
        }
        if let Some(slug) = catalogue::slug_from_url(&s(&current_series, "url")) {
            let clean_slug = catalogue::clean_series_title(&slug);
            if !clean_slug.is_empty() && !queries.contains(&clean_slug) {
                queries.push(clean_slug.clone());
                if target_edition == "dubbed" {
                    queries.push(format!("{clean_slug} مدبلج"));
                }
            }
        }
        for src in [&preferred, other] {
            for q in &queries {
                if let Ok(results) = api::call("/api/search", &[("q", q.clone())], Some(src)).await {
                    if STATE.with(|state| state.borrow().sheet_generation) != token {
                        return;
                    }
                    let rows = results["results"].as_array().cloned().unwrap_or_default();
                    if let Some(found) = catalogue::find_counterpart_with_edition(&dummy, &rows, Some(&target_edition)) {
                        let url = if !s(&found, "seriesUrl").is_empty() {
                            s(&found, "seriesUrl")
                        } else {
                            s(&found, "url")
                        };
                        if !url.is_empty() {
                            open_series(url);
                            return;
                        }
                    }
                }
            }
        }
        open_sheet(
            "النسخة غير متاحة",
            &format!("<p class=\"empty-note\">لم يتم العثور على نسخة {} لهذا المسلسل في السيرفرات الحالية.</p>", label),
        );
    });
}
fn render_detail_episodes() {
    let Some(list) = document().get_element_by_id("seriesEpisodeList") else {
        return;
    };
    let season = list.get_attribute("data-season").unwrap_or("1".into());
    let query = document()
        .get_element_by_id("sheetEpisodeSearch")
        .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
        .map(|e| e.value())
        .unwrap_or_default();
    let series = STATE.with(|state| state.borrow().context.clone());
    let episodes = series["episodes"].as_array().cloned().unwrap_or_default();
    let markup=episodes.iter().filter(|ep|{let ep_season=s(ep,"season");(ep_season==season || (ep_season.is_empty()&&season=="1"))&&(query.is_empty()||format!("{} {}",s(ep,"title"),catalogue::episode_number(ep)).contains(&query))}).enumerate().map(|(index,ep)|{
        let number=catalogue::episode_number(ep);
        format!("<button class=\"episode-card sheet-episode-btn\" data-action=\"episode\" data-url=\"{}\"><span class=\"episode-number\">{}</span><span class=\"episode-copy\"><strong>{}</strong><small>{}</small></span><span class=\"episode-play\"><i class=\"fa-solid fa-play\"></i></span></button>",esc(&s(ep,"url")),if number.is_empty(){(index+1).to_string()}else{esc(&number)},esc(&s(ep,"title")),esc(&s(&series,"title")))
    }).collect::<String>();
    list.set_inner_html(if markup.is_empty() {
        "<p class=\"empty-note\">لا توجد حلقات مطابقة.</p>"
    } else {
        &markup
    });
}
fn click(event: Event) {
    let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    if target
        .closest("[data-player-action]")
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    let Some(button) = target
        .closest("[data-action],[data-source],[data-library]")
        .ok()
        .flatten()
    else {
        return;
    };
    if let Some(source) = button.get_attribute("data-source") {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.source = source.clone();
            state.search_timer = None;
            state.search.needs_refresh = true;
            state.search.generation += 1;
            state.search.pending = 0;
            if let Some(abort) = state.search.abort.take() {
                abort.abort();
            }
        });
        store("zoet_source", &source);
        store("otium_source", &source);
        update_source();
        load_listing(false, true);
        if !el("searchOverlay").class_list().contains("hidden") {
            schedule_search(true);
        }
        return;
    }
    if let Some(mode) = button.get_attribute("data-library") {
        STATE.with(|state| state.borrow_mut().library_mode = mode);
        render_library();
        return;
    }
    let url = button.get_attribute("data-url").unwrap_or_default();
    match button.get_attribute("data-action").as_deref() {
        Some("series") => {
            if let Some(source) = button.get_attribute("data-series-source") {
                store(
                    "otium_preferred_source",
                    catalogue::provider(&json!({"url":source})),
                );
            }
            open_series(url);
        }
        Some("series-edition") | Some("switch-detail-edition") => {
            let target_url = button.get_attribute("data-url").unwrap_or_default();
            let target_edition = button.get_attribute("data-edition").unwrap_or_default();
            let title = button.get_attribute("data-title").unwrap_or_default();
            if !target_url.is_empty() {
                open_series(target_url);
            } else {
                switch_detail_edition(target_edition, title);
            }
        }
        Some("episode") => {
            close_sheet();
            search_close_if_open();
            player::open_episode(url, None);
        }
        Some("story") => {
            let expanded = el("seriesStory")
                .class_list()
                .toggle("expanded")
                .unwrap_or(false);
            button.set_text_content(Some(if expanded {
                "اختصار القصة"
            } else {
                "قراءة القصة"
            }));
            let _ = button.set_attribute("aria-expanded", &expanded.to_string());
        }
        Some("download") => {
            let native = native_call("openDownloadUrl", &[url.clone().into()]);
            if native.is_undefined() {
                let _ = window().open_with_url_and_target(&url, "_blank");
            }
        }
        Some("export-backup") => {
            let hist = read_records("krmzi_hist");
            let favs = read_records("krmzi_favs");
            let backup = json!({
                "app": "ZOET",
                "version": 2,
                "history": hist,
                "favorites": favs
            });
            let text_val = serde_json::to_string(&backup).unwrap_or_default();
            if let Some(textarea) = document().get_element_by_id("backupTextarea") {
                let _ = Reflect::set(textarea.as_ref(), &"value".into(), &text_val.clone().into());
            }
            if let Ok(cb) = Reflect::get(window().navigator().as_ref(), &"clipboard".into()) {
                if let Ok(wt) = Reflect::get(&cb, &"writeText".into()) {
                    if let Ok(f) = wt.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&cb, &text_val.into());
                    }
                }
            }
            notify("تم تجهيز النسخة الاحتياطية ونسخها إلى الحافظة");
        }
        Some("import-backup") => {
            if let Some(textarea) = document().get_element_by_id("backupTextarea") {
                let raw = Reflect::get(textarea.as_ref(), &"value".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let raw = raw.trim();
                if let Ok(data) = serde_json::from_str::<Value>(raw) {
                    if let Some(hist) = data["history"].as_array() {
                        store("krmzi_hist", &serde_json::to_string(hist).unwrap_or_default());
                        STATE.with(|s| s.borrow_mut().history = hist.clone());
                    }
                    if let Some(favs) = data["favorites"].as_array() {
                        store("krmzi_favs", &serde_json::to_string(favs).unwrap_or_default());
                        STATE.with(|s| s.borrow_mut().favorites = favs.clone());
                    }
                    render_library();
                    close_sheet();
                    notify("تم استعادة السجل والمفضلة بنجاح");
                } else {
                    notify("صيغة الكود غير صحيحة، يرجى التأكد من نسخه بالكامل");
                }
            }
        }
        Some("season") => {
            if let Some(season) = button.get_attribute("data-season") {
                let _ = el("seriesEpisodeList").set_attribute("data-season", &season);
                if let Ok(buttons) = el("sheetSeasonTabs").query_selector_all("button") {
                    for i in 0..buttons.length() {
                        if let Some(node) =
                            buttons.item(i).and_then(|n| n.dyn_into::<Element>().ok())
                        {
                            let _ = node
                                .class_list()
                                .toggle_with_force("active", node == button);
                        }
                    }
                }
                render_detail_episodes();
            }
        }
        _ => {}
    }
}
fn search_close_if_open() {
    if !el("searchOverlay").class_list().contains("hidden") {
        search_close();
    }
}
fn update_source() {
    let source = STATE.with(|state| state.borrow().source.clone());
    if let Ok(buttons) = document().query_selector_all("[data-source]") {
        for i in 0..buttons.length() {
            if let Some(button) = buttons.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let active = button.get_attribute("data-source").as_deref() == Some(&source);
                let _ = button.class_list().toggle_with_force("active", active);
                let _ = button.set_attribute("aria-pressed", &active.to_string());
            }
        }
    }
}
fn open_backup_sheet() {
    open_sheet(
        "النسخ الاحتياطي ونقل البيانات",
        "<div class=\"backup-dialog\">\
            <p class=\"backup-desc\">احفظ سجلك ومفضلتك بدون أي حساب سحابي، وانقلها لأي هاتف أو شاشة ذكية بسهولة.</p>\
            <div class=\"backup-row\">\
                <button class=\"load-more-btn\" data-action=\"export-backup\"><i class=\"fa-solid fa-copy\"></i> تصدير ونسخ بياناتي</button>\
            </div>\
            <div class=\"backup-import-section\">\
                <label for=\"backupTextarea\">كود النسخة الاحتياطية:</label>\
                <textarea id=\"backupTextarea\" rows=\"4\" placeholder=\"ألصق كود النسخة الاحتياطية هنا...\"></textarea>\
                <button class=\"load-more-btn secondary\" data-action=\"import-backup\"><i class=\"fa-solid fa-file-import\"></i> استعادة البيانات الآن</button>\
            </div>\
        </div>"
    );
}
fn apply_display() {
    let saved_display = storage("zoet_display").or_else(|| storage("otium_display"));
    let tv = saved_display.as_deref() == Some("tv")
        || native_bool("isTelevision")
        || window()
            .navigator()
            .user_agent()
            .unwrap_or_default()
            .contains("Android TV");
    let width = window()
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(390.);
    let _ = document().document_element().unwrap().set_attribute(
        "data-display",
        if tv {
            "tv"
        } else if width >= 1024. {
            "desktop"
        } else {
            "mobile"
        },
    );
    if let Some(select) = document()
        .get_element_by_id("displayModeSelect")
        .and_then(|element| element.dyn_into::<web_sys::HtmlSelectElement>().ok())
    {
        select.set_value(&saved_display.unwrap_or("auto".into()));
    }
}
fn native_bool(method: &str) -> bool {
    native_call(method, &[]).as_bool().unwrap_or(false)
}
pub fn native_call(method: &str, args: &[JsValue]) -> JsValue {
    let bridge =
        Reflect::get(window().as_ref(), &"KrmziBridge".into()).unwrap_or(JsValue::UNDEFINED);
    let Some(function) = Reflect::get(&bridge, &method.into())
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    else {
        return JsValue::UNDEFINED;
    };
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    function
        .apply(&bridge, &array)
        .unwrap_or(JsValue::UNDEFINED)
}
fn back() -> bool {
    if !el("sheetBackdrop").class_list().contains("hidden") {
        close_sheet();
        true
    } else if !el("searchOverlay").class_list().contains("hidden") {
        search_close();
        true
    } else if player::is_open() {
        player::close();
        true
    } else {
        false
    }
}
fn keyboard(event: KeyboardEvent) {
    let editable = event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .is_some_and(|el| {
            el.matches("input,textarea,select,[contenteditable=true]")
                .unwrap_or(false)
        });
    if ["Escape", "BrowserBack", "GoBack"].contains(&event.key().as_str())
        || (event.key() == "Backspace" && !editable)
    {
        if back() {
            event.prevent_default();
        }
        return;
    }
    if event.key() == "Tab" && !el("sheetBackdrop").class_list().contains("hidden") {
        let nodes = el("sheetBackdrop")
            .query_selector_all(
                "button,input,select,textarea,a[href],[tabindex]:not([tabindex='-1'])",
            )
            .unwrap();
        let nodes = (0..nodes.length())
            .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
            .filter(|e| e.offset_height() > 0 && !e.has_attribute("disabled"))
            .collect::<Vec<_>>();
        if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
            let focused = document().active_element();
            let outside = !nodes
                .iter()
                .any(|node| focused.as_ref() == Some(node.as_ref()));
            if event.shift_key() && (outside || focused.as_ref() == Some(first.as_ref())) {
                event.prevent_default();
                let _ = last.focus();
            } else if !event.shift_key() && (outside || focused.as_ref() == Some(last.as_ref())) {
                event.prevent_default();
                let _ = first.focus();
            }
        }
    }
    if !editable && !player::is_open() && event.key() == "/" {
        event.prevent_default();
        search_open();
    }
    if document()
        .document_element()
        .unwrap()
        .get_attribute("data-display")
        .as_deref()
        == Some("tv")
        && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].contains(&event.key().as_str())
        && !editable
    {
        remote_focus(&event);
    }
}
fn remote_focus(event: &KeyboardEvent) {
    if player::is_open() && el("sheetBackdrop").class_list().contains("hidden") {
        return;
    }
    let is_tv = document()
        .document_element()
        .and_then(|e| e.get_attribute("data-display"))
        .as_deref()
        == Some("tv");
    let root = if !el("sheetBackdrop").class_list().contains("hidden") {
        el("sheetBackdrop")
    } else if !el("searchOverlay").class_list().contains("hidden") {
        el("searchOverlay")
    } else {
        el("appFrame")
    };
    let nodes = root
        .query_selector_all("button,input,select,textarea,a[href],[tabindex]:not([tabindex='-1'])")
        .unwrap();
    let nodes = (0..nodes.length())
        .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
        .filter(|n| {
            n.offset_height() > 0
                && !n.has_attribute("disabled")
                && n.closest(".hidden").ok().flatten().is_none()
                && !(is_tv && n.id() == "sheetEpisodeSearch")
        })
        .collect::<Vec<_>>();
    let current = document().active_element();
    let current = match current {
        Some(ref el) if el.tag_name() != "BODY" && el.tag_name() != "HTML" => Some(el),
        _ => None,
    };
    let Some(current) = current else {
        if let Some(first) = nodes.first() {
            event.prevent_default();
            let _ = first.focus();
            first.scroll_into_view_with_bool(event.key() == "ArrowUp");
        }
        return;
    };
    let rect = current.get_bounding_client_rect();
    let horizontal = event.key() == "ArrowLeft" || event.key() == "ArrowRight";
    let sign = if event.key() == "ArrowLeft" || event.key() == "ArrowUp" {
        -1.
    } else {
        1.
    };
    let mut ranked = nodes
        .iter()
        .filter(|node| node.unchecked_ref::<Element>() != current)
        .filter_map(|node| {
            let r = node.get_bounding_client_rect();
            let dx = r.x() + r.width() / 2. - rect.x() - rect.width() / 2.;
            let dy = r.y() + r.height() / 2. - rect.y() - rect.height() / 2.;
            let p = if horizontal { dx } else { dy } * sign;
            let side = if horizontal { dy.abs() } else { dx.abs() };
            if p > 1.5 {
                Some((p + side * 1.5, node))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, next)) = ranked.first() {
        event.prevent_default();
        let _ = next.focus();
        next.scroll_into_view_with_bool(event.key() == "ArrowUp");
    }
}
fn parse_version(v: &str) -> Vec<u32> {
    v.trim_start_matches('v')
        .split('.')
        .map(|n| n.parse::<u32>().unwrap_or(0))
        .collect()
}

fn check_updates(manual: bool) {
    if manual {
        text("checkUpdateBtnText", "جاري الفحص…");
    }
    spawn_local(async move {
        let result = api::request_json(
            "https://api.github.com/repos/XRTM8/ZOET/releases/latest",
            None,
        )
        .await;
        match result {
            Ok(data) => {
                let version = s(&data, "tag_name");
                let remote = parse_version(&version);
                let local = parse_version("2.6.1");
                if remote.is_empty() || remote <= local {
                    if manual {
                        notify("أنت تستخدم آخر إصدار");
                    }
                } else if let Some(url) = data["assets"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|v| s(v, "name").ends_with(".apk")))
                    .map(|v| s(v, "browser_download_url"))
                {
                    open_sheet(
                        "تحديث التطبيق",
                        &format!(
                            "<p>{}</p><p class=\"changelog-container\">{}</p><button class=\"load-more-btn\" data-action=\"download\" data-url=\"{}\">تنزيل وتثبيت التحديث الآن</button>",
                            esc(&version),
                            esc(&s(&data, "body")),
                            esc(&url)
                        ),
                    );
                }
            }
            _ => {
                if manual {
                    notify("تعذّر فحص التحديث الآن");
                }
            }
        }
        if manual {
            text("checkUpdateBtnText", "بحث عن تحديث");
        }
    });
}

fn check_updates_manual() {
    check_updates(true);
}

fn check_updates_auto() {
    let last = storage("zoet_update_checked")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let now = js_sys::Date::now();
    if now - last < 86_400_000.0 {
        return;
    }
    store("zoet_update_checked", &now.to_string());
    check_updates(false);
}
pub fn init() {
    if window().location().hostname().unwrap_or_default() != "appassets.androidplatform.net" {
        spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(
                window().navigator().service_worker().register("sw.js"),
            )
            .await;
        });
    }
    apply_display();
    update_source();
    text("installedVersionText", "v2.6.1");
    on("tabSeries", "click", |_| set_view("series"));
    on("tabMovies", "click", |_| set_view("movies"));
    on("openLibraryBtn", "click", |_| set_view("library"));
    on("brandBtn", "click", |_| set_view("series"));
    on("openSearchBtn", "click", |_| search_open());
    on("closeSearchOverlay", "click", |_| search_close());
    on("overlaySearchInput", "input", |_| schedule_search(false));
    on("overlaySearchInput", "keydown", |event| {
        let event: KeyboardEvent = event.unchecked_into();
        if event.key() == "Enter" {
            event.prevent_default();
            schedule_search(true);
        }
    });
    on("loadMoreSeriesBtn", "click", |_| load_listing(false, false));
    on("loadMoreSearchBtn", "click", |_| load_listing(true, false));
    on("retryBtn", "click", |_| load_listing(false, false));
    on("switchToSeriesBtn", "click", |_| set_view("series"));
    on("closeSheetBtn", "click", |_| close_sheet());
    on("sheetBackdrop", "click", |event| {
        if event.target().as_ref() == Some(el("sheetBackdrop").as_ref()) {
            close_sheet();
        }
    });
    on("manualCheckUpdateBtn", "click", |_| check_updates_manual());
    spawn_local(async move {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 4000);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        check_updates_auto();
    });
    on("backupDataBtn", "click", |_| open_backup_sheet());
    on("clearHistoryBtn", "click", |_| {
        STATE.with(|state| state.borrow_mut().history.clear());
        store("krmzi_hist", "[]");
        render_library();
    });
    on("displayModeSelect", "change", |_| {
        let select = el("displayModeSelect")
            .dyn_into::<web_sys::HtmlSelectElement>()
            .unwrap();
        store("zoet_display", &select.value());
        store("otium_display", &select.value());
        apply_display();
    });
    on("controlsHelpBtn", "click", |_| {
        open_sheet("اختصارات التحكم","<div class=\"controls-guide\"><p>Space / K — تشغيل وإيقاف</p><p>J / L — تقديم وتأخير 10 ثوانٍ</p><p>الأسهم — تقديم وتأخير / الصوت</p><p>F — ملء الشاشة · M — كتم الصوت</p><p>S — المصادر · Q — الجودة · E — الحلقات</p><p>0–9 — الانتقال داخل الحلقة · [ / ] — سرعة التشغيل</p><p>Esc — رجوع · / — بحث</p><p>لمستان على جانبي الفيديو للتقديم أو التأخير.</p></div>")
    });
    listen(document().as_ref(), "click", click);
    listen(document().as_ref(), "input", |event| {
        if event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|e| e.id() == "sheetEpisodeSearch")
        {
            render_detail_episodes();
        }
    });
    listen(document().as_ref(), "keydown", |event| {
        keyboard(event.unchecked_into())
    });
    listen(window().as_ref(), "resize", |_| apply_display());
    // Native Android back dispatch and media controls use Rust exports.
    for (name, kind) in [
        ("krmziClosePlayer", 0),
        ("krmziCloseSheet", 1),
        ("krmziCloseSearch", 2),
        ("zoetClosePlayer", 0),
        ("zoetCloseSheet", 1),
        ("zoetCloseSearch", 2),
    ] {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || match kind {
            0 => player::close(),
            1 => close_sheet(),
            _ => search_close(),
        }) as Box<dyn FnMut()>);
        let _ = Reflect::set(window().as_ref(), &name.into(), closure.as_ref());
        closure.forget();
    }
    for (name, kind) in [
        ("krmziIsPlayerOpen", 0),
        ("krmziIsSheetOpen", 1),
        ("krmziIsSearchOpen", 2),
        ("zoetIsPlayerOpen", 0),
        ("zoetIsSheetOpen", 1),
        ("zoetIsSearchOpen", 2),
    ] {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || match kind {
            0 => player::is_open(),
            1 => !el("sheetBackdrop").class_list().contains("hidden"),
            _ => !el("searchOverlay").class_list().contains("hidden"),
        }) as Box<dyn FnMut() -> bool>);
        let _ = Reflect::set(window().as_ref(), &name.into(), closure.as_ref());
        closure.forget();
    }
    for name in ["krmziHandleBack", "zoetHandleBack"] {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            if !el("sheetBackdrop").class_list().contains("hidden") {
                close_sheet();
                return true;
            }
            if player::is_open() {
                player::close();
                return true;
            }
            if !el("searchOverlay").class_list().contains("hidden") {
                search_close();
                return true;
            }
            let current_view = STATE.with(|s| s.borrow().view.clone());
            if current_view != "series" {
                set_view("series");
                return true;
            }
            false
        }) as Box<dyn FnMut() -> bool>);
        let _ = Reflect::set(window().as_ref(), &name.into(), closure.as_ref());
        closure.forget();
    }
    for name in ["krmziTogglePlayer", "zoetTogglePlayer"] {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            if player::is_open() {
                player::toggle();
            }
        }) as Box<dyn FnMut()>);
        let _ = Reflect::set(window().as_ref(), &name.into(), closure.as_ref());
        closure.forget();
    }
    for name in ["krmziSeekPlayer", "zoetSeekPlayer"] {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |secs: f64| {
            if player::is_open() {
                player::seek(secs);
            }
        }) as Box<dyn FnMut(f64)>);
        let _ = Reflect::set(window().as_ref(), &name.into(), closure.as_ref());
        closure.forget();
    }
    show("bootStatus", false);
    set_view("series");
}
