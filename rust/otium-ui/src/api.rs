use crate::{catalogue, search};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::future::join_all;
use gloo_timers::callback::Timeout;
use serde_json::{json, Value};
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, AbortSignal, Request, RequestCache, RequestInit, Response};

/// A listener must live until fetch finishes, then be removed from its caller's
/// signal. Dropping a player request never leaves an unbounded network request.
struct AbortLink {
    signal: AbortSignal,
    listener: Closure<dyn FnMut()>,
}
impl Drop for AbortLink {
    fn drop(&mut self) {
        let _ = self
            .signal
            .remove_event_listener_with_callback("abort", self.listener.as_ref().unchecked_ref());
    }
}

struct RequestLifetime(AbortController);
impl Drop for RequestLifetime {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn encoded(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}

// Cinemana's public web client encodes a UTF-8 keyword as unpadded Base64URL.
// A plaintext keyword is accepted by the endpoint but silently returns the
// unfiltered catalogue, making successful HTTP responses misleading.
fn search_keyword(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(value.as_bytes())
}

fn parameter<'a>(params: &'a [(&str, String)], name: &str) -> &'a str {
    params
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn text(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn first_text(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| text(value, key))
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn rows(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .or_else(|| value["data"].as_array())
        .cloned()
        .unwrap_or_default()
}

async fn request(url: &str, signal: Option<&AbortSignal>) -> Result<Value, String> {
    let controller = AbortController::new().map_err(|_| "تعذر بدء الاتصال".to_owned())?;
    let _lifetime = RequestLifetime(controller.clone());
    let _link = if let Some(signal) = signal {
        let linked = controller.clone();
        let listener = Closure::<dyn FnMut()>::new(move || linked.abort());
        signal
            .add_event_listener_with_callback("abort", listener.as_ref().unchecked_ref())
            .map_err(|_| "تعذر بدء الاتصال".to_owned())?;
        if signal.aborted() {
            controller.abort();
        }
        Some(AbortLink {
            signal: signal.clone(),
            listener,
        })
    } else {
        None
    };
    let timed = controller.clone();
    let _timer = Timeout::new(12_000, move || timed.abort());
    let options = RequestInit::new();
    options.set_method("GET");
    options.set_cache(RequestCache::NoStore);
    options.set_signal(Some(&controller.signal()));
    let request = Request::new_with_str_and_init(url, &options)
        .map_err(|_| "رابط الطلب غير صالح".to_owned())?;
    let _ = request.headers().set("Accept", "application/json");
    let window = web_sys::window().ok_or_else(|| "المتصفح غير متاح".to_owned())?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| {
            if signal.is_some_and(AbortSignal::aborted) {
                "cancelled".to_owned()
            } else {
                "تعذر الاتصال بالمصدر أو انتهت مهلة الاتصال".to_owned()
            }
        })?
        .dyn_into::<Response>()
        .map_err(|_| "استجابة غير صالحة".to_owned())?;
    if !response.ok() {
        return Err(format!("المصدر غير متاح ({})", response.status()));
    }
    let body = JsFuture::from(
        response
            .text()
            .map_err(|_| "تعذر قراءة الاستجابة".to_owned())?,
    )
    .await
    .map_err(|_| "تعذر قراءة الاستجابة".to_owned())?
    .as_string()
    .unwrap_or_default();
    let data: Value =
        serde_json::from_str(&body).map_err(|_| "استجابة المصدر غير صالحة".to_owned())?;
    let failed = match &data["error"] {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
        _ => true,
    };
    if data.is_null() || failed || data["status"] == false || data["isSuccess"] == false {
        return Err("المصدر غير متاح على اتصالك الحالي".to_owned());
    }
    Ok(data)
}

pub async fn request_json(url: &str, signal: Option<&AbortSignal>) -> Result<Value, String> {
    request(url, signal).await
}

async fn cinemana_request(path: &str, signal: Option<&AbortSignal>) -> Result<Value, String> {
    request(&format!("/api/cinemana?path={}", encoded(path)), signal).await
}

fn root_id(item: &Value) -> String {
    let root = text(item, "rootSeries");
    if !root.is_empty() && root != "0" {
        root
    } else {
        text(item, "nb")
    }
}

fn artwork(item: &Value) -> String {
    first_text(
        item,
        &["imgObjUrl", "imgMediumThumbObjUrl", "imgThumbObjUrl"],
    )
}

fn title(item: &Value) -> String {
    let name = first_text(item, &["ar_title", "en_title", "other_title"]);
    if name.is_empty() {
        "بدون عنوان".to_owned()
    } else {
        name
    }
}

fn card(item: &Value) -> Value {
    let movie = text(item, "kind") == "1"
        || text(item, "mediaType") == "movie"
        || text(item, "type") == "movie"
        || (text(item, "rootSeries") == "0" && text(item, "kind") != "2");
    let aliases: Vec<String> = ["en_title", "other_title"]
        .iter()
        .map(|key| text(item, key))
        .filter(|value| !value.is_empty())
        .collect();
    json!({
        "url":format!("cinemana:{}",text(item,"nb")), "title":title(item),
        "thumbnail":first_text(item,&["imgMediumThumbObjUrl","imgObjUrl","imgThumbObjUrl"]),
        "artwork":artwork(item),"seriesTitle":title(item),"source":"cinemana","aliases":aliases,"edition":catalogue::edition(item),
        "mediaType":if movie {"movie"} else {"series"},"type":if movie {"episode"} else {"series"},
        "year":text(item,"year"),"rating":text(item,"stars"),
        "seriesUrl":if movie {String::new()} else {format!("cinemana:{}",root_id(item))}
    })
}

fn paginated(items: Vec<Value>, page: u32, key: &str, size: usize) -> Value {
    let has_next = items.len() >= size;
    let mut result =
        json!({"page":page,"hasNext":has_next,"nextPage":if has_next {Some(page+1)} else {None}});
    result[key] = Value::Array(items);
    result
}

pub async fn call(
    endpoint: &str,
    params: &[(&str, String)],
    source: Option<&str>,
) -> Result<Value, String> {
    call_with_signal(endpoint, params, source, None).await
}

/// Provider calls share one contract on Web and Android. All native work runs
/// through the same-origin Rust API; this client never synchronously calls JNI.
pub async fn call_with_signal(
    endpoint: &str,
    params: &[(&str, String)],
    source: Option<&str>,
    signal: Option<&AbortSignal>,
) -> Result<Value, String> {
    let url = parameter(params, "url");
    let inferred = if url.starts_with("cinemana:") {
        "cinemana"
    } else {
        "krmzi"
    };
    if source.unwrap_or(inferred) != "cinemana" {
        let query = params
            .iter()
            .map(|(key, value)| format!("{}={}", encoded(key), encoded(value)))
            .collect::<Vec<_>>()
            .join("&");
        return request(
            &format!(
                "{endpoint}{}{}",
                if query.is_empty() { "" } else { "?" },
                query
            ),
            signal,
        )
        .await;
    }
    let page = parameter(params, "page")
        .parse::<u32>()
        .unwrap_or(1)
        .clamp(1, 10_000);
    let provider_page = page - 1;
    match endpoint {
        "/api/home" => {
            let (series, movies) = futures_util::future::join(
                cinemana_request(
                    &format!("latestSeries/level/0/itemsPerPage/24/page/{provider_page}/"),
                    signal,
                ),
                cinemana_request(
                    &format!("latestMovies/level/0/itemsPerPage/24/page/{provider_page}/"),
                    signal,
                ),
            )
            .await;
            if signal.is_some_and(AbortSignal::aborted) {
                return Err("cancelled".to_owned());
            }
            if series.is_err() && movies.is_err() {
                return Err("سينمانا غير متاح على اتصالك الحالي".to_owned());
            }
            Ok(json!({
                "series":series.map(|data|paginated(rows(&data).iter().map(card).collect(),page,"series",24)).unwrap_or(json!({"series":[],"page":page,"error":"cinemana_unavailable"})),
                "episodes":movies.map(|data|paginated(rows(&data).iter().map(card).collect(),page,"episodes",24)).unwrap_or(json!({"episodes":[],"page":page,"error":"cinemana_unavailable"}))
            }))
        }
        "/api/series" | "/api/episodes" => {
            let movie = endpoint == "/api/episodes";
            let path = format!(
                "{}/level/0/itemsPerPage/24/page/{provider_page}/",
                if movie {
                    "latestMovies"
                } else {
                    "latestSeries"
                }
            );
            let data = cinemana_request(&path, signal).await?;
            Ok(paginated(
                rows(&data).iter().map(card).collect(),
                page,
                if movie { "episodes" } else { "series" },
                24,
            ))
        }
        "/api/search" => {
            let query = parameter(params, "q")
                .trim()
                .chars()
                .take(120)
                .collect::<String>();
            if query.is_empty() {
                return Ok(
                    json!({"query":"","page":page,"results":[],"hasNext":false,"nextPage":null}),
                );
            }
            let parsed = search::parse_query(&query);
            let primary_q = if parsed.core.is_empty() {
                query.clone()
            } else {
                parsed.core.clone()
            };
            let path = format!(
                "video/V/2/itemsPerPage/20/video_title_search/{}/itemsPerPage/12/pageNumber/{provider_page}/level/0",
                search_keyword(&primary_q)
            );
            let data = cinemana_request(&path, signal).await?;
            let primary_rows = rows(&data);
            let mut has_next = primary_rows.len() >= 12;
            let mut items: Vec<Value> = primary_rows.iter().map(card).collect();
            let variants = search::fallback_variants(&query, &parsed);
            if page == 1 && !variants.is_empty() {
                let best_primary = items
                    .iter()
                    .map(|item| search::score_item(item, &parsed, &variants))
                    .max()
                    .unwrap_or(0);
                if items.len() < 4 || best_primary < search::STRONG_MATCH_THRESHOLD {
                    let mut seen_urls: std::collections::HashSet<String> = items
                        .iter()
                        .filter_map(|item| item["url"].as_str().map(str::to_owned))
                        .collect();
                    let extra_futures = variants.iter().take(2).map(|variant| {
                        let v_path = format!(
                            "video/V/2/itemsPerPage/20/video_title_search/{}/itemsPerPage/12/pageNumber/0/level/0",
                            search_keyword(variant)
                        );
                        async move { cinemana_request(&v_path, signal).await }
                    });
                    for extra in join_all(extra_futures).await.into_iter().flatten() {
                        let extra_rows = rows(&extra);
                        has_next |= extra_rows.len() >= 12;
                        for row in extra_rows {
                            let c = card(&row);
                            if let Some(u) = c["url"].as_str() {
                                if seen_urls.insert(u.to_owned()) {
                                    items.push(c);
                                }
                            }
                        }
                    }
                }
            }
            search::sort_by_relevance(&mut items, &parsed, &variants);
            let mut result = paginated(items, page, "results", 12);
            result["hasNext"] = json!(has_next);
            result["nextPage"] = json!(if has_next { Some(page + 1) } else { None });
            result["query"] = json!(query);
            result["cleanedQuery"] = json!(primary_q);
            Ok(result)
        }
        "/api/series/detail" | "/api/episode/detail" => {
            let id = url
                .strip_prefix("cinemana:")
                .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
                .ok_or_else(|| "مرجع سينمانا غير صالح".to_owned())?;
            let series = endpoint == "/api/series/detail";
            let second = if series {
                "videoSeason"
            } else {
                "transcoddedFiles"
            };
            let (info, data) = futures_util::future::try_join(
                cinemana_request(&format!("allVideoInfo/id/{id}"), signal),
                cinemana_request(&format!("{second}/id/{id}"), signal),
            )
            .await?;
            let mut result = card(&info);
            result["poster"] = json!(artwork(&info));
            result["story"] = json!(first_text(&info, &["ar_content", "en_content"]));
            if series {
                let mut episodes: Vec<Value> = rows(&data)
                    .iter()
                    .map(|item| {
                        let mut episode = card(item);
                        let num = text(item, "episodeNummer");
                        let season = first_text(item, &["season"]);
                        episode["episodeNum"] = json!(num);
                        episode["type"] = json!("episode");
                        episode["edition"] = json!(catalogue::edition(&info));
                        episode["season"] = json!(if season.is_empty() { "1" } else { &season });
                        episode["seriesUrl"] = json!(format!("cinemana:{}", root_id(&info)));
                        episode["title"] = json!(format!("الحلقة {num}"));
                        episode
                    })
                    .collect();
                episodes.sort_by(|a, b| {
                    let number =
                        |value: &Value, key: &str| text(value, key).parse::<f64>().unwrap_or(0.0);
                    number(a, "season")
                        .total_cmp(&number(b, "season"))
                        .then(number(a, "episodeNum").total_cmp(&number(b, "episodeNum")))
                });
                result["episodes"] = json!(episodes);
            } else {
                let is_movie = text(&info, "kind") == "1"
                    || text(&info, "mediaType") == "movie"
                    || text(&info, "type") == "movie"
                    || (text(&info, "rootSeries") == "0" && text(&info, "kind") != "2");
                result["type"] = json!("episode");
                result["mediaType"] = if is_movie { json!("movie") } else { json!("series") };
                if is_movie {
                    result["seriesUrl"] = json!("");
                }
                let streams:Vec<Value> = rows(&data).iter().filter_map(|file| {
                    let container = text(file,"container").to_lowercase();
                    let url = text(file,"videoUrl");
                    if !url.starts_with("https://") || !["mp4","m3u8","hls"].contains(&container.as_str()) {return None;}
                    let quality = first_text(file,&["resolution"]);
                    let mut stream = json!({"quality":if quality.is_empty(){"تلقائي"} else {&quality},"url":url,"provider":"cinemana"});
                    if container == "m3u8" || container == "hls" {stream["type"] = json!("hls");}
                    Some(stream)
                }).collect();
                if streams.is_empty() {
                    return Err("روابط سينمانا غير متاحة على شبكتك".to_owned());
                }
                let subtitles: Vec<Value> = rows(&info["translations"])
                    .iter()
                    .filter_map(|track| {
                        let url = text(track, "file");
                        if text(track, "extention").to_lowercase() != "vtt"
                            || !url.starts_with("https://")
                        {
                            return None;
                        }
                        let lang = text(track, "type");
                        let label = match lang.as_str() {
                            "ar" => "العربية".to_owned(),
                            "en" => "English".to_owned(),
                            _ => text(track, "name"),
                        };
                        Some(json!({"url":url,"lang":lang,"label":label}))
                    })
                    .collect();
                result["directStreams"] = json!(streams);
                result["servers"] = json!([]);
                result["subtitles"] = json!(subtitles);
                result["episodeNum"] = json!(text(&info, "episodeNummer"));
                result["season"] = json!(first_text(&info, &["season"]));
                result["isSuccess"] = json!(true);
                if text(&info, "kind") != "1" {
                    result["title"] = json!(format!(
                        "{} · الحلقة {}",
                        title(&info),
                        text(&info, "episodeNummer")
                    ));
                }
            }
            Ok(result)
        }
        _ => Err("طلب سينمانا غير مدعوم".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cinemana_search_uses_unpadded_utf8_base64url() {
        assert_eq!(search_keyword("الخليفة"), "2KfZhNiu2YTZitmB2Kk");
        assert_eq!(search_keyword("The Last of Us"), "VGhlIExhc3Qgb2YgVXM");
        for query in ["الخليفة", "Test / title + 2", "أنت من أحب"] {
            let encoded = search_keyword(query);
            assert!(!encoded.contains(['+', '/', '=']));
            assert_eq!(URL_SAFE_NO_PAD.decode(encoded).unwrap(), query.as_bytes());
        }
    }
}
