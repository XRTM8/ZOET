//! Shared provider engine used directly by Android through JNI and by the web server.
//! The API is read-only. Signed media links are never served stale.
pub mod models;
pub use models::*;

use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use regex::Regex;
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{Cursor, Read},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

const ORIGIN: &str = "https://krmzi.org";
const CINEMANA: &str = "https://cinemana.shabakaty.com/api/android/";
const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 14; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";
const MAX_TEXT: usize = 8 * 1024 * 1024;
const MAX_CACHE: usize = 180;
type Result<T> = std::result::Result<T, String>;
type Flight = Arc<AsyncMutex<Option<Result<Value>>>>;

pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Rust runtime")
    })
}
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::new)
}

#[derive(Clone)]
struct Entry {
    data: Value,
    expires: Instant,
    created: Instant,
}
pub struct Engine {
    client: Client,
    cache: Mutex<HashMap<String, Entry>>,
    flights: Mutex<HashMap<String, Flight>>,
    started: Instant,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
impl Engine {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .pool_max_idle_per_host(12)
            .tcp_nodelay(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("HTTP client");
        Self {
            client,
            cache: Mutex::new(HashMap::new()),
            flights: Mutex::new(HashMap::new()),
            started: Instant::now(),
        }
    }
    pub fn trim_memory(&self, clear: bool) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if clear {
            cache.clear();
            return;
        }
        cache.retain(|_, e| e.expires > Instant::now());
        if cache.len() > 60 {
            let mut order: Vec<_> = cache.iter().map(|(k, e)| (k.clone(), e.created)).collect();
            order.sort_by_key(|(_, t)| *t);
            for (key, _) in order.into_iter().take(cache.len() - 40) {
                cache.remove(&key);
            }
        }
    }
    async fn cached<F, Fut>(&self, key: String, ttl: u64, fresh: bool, producer: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value>>,
    {
        if fresh {
            self.cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&key);
        }
        if let Some(entry) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .filter(|e| e.expires > Instant::now())
        {
            return Ok(entry.data.clone());
        }
        let flight = self
            .flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(key.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(None)))
            .clone();
        let mut guard = flight.lock().await;
        if let Some(result) = guard.as_ref() {
            return result.clone();
        }
        let ready = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .filter(|e| e.expires > Instant::now())
            .map(|entry| entry.data.clone());
        let result = if let Some(data) = ready {
            Ok(data)
        } else {
            producer().await
        };
        if let Some(data) = result.as_ref().ok().filter(|data| cacheable(data)) {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.retain(|_, entry| entry.expires > Instant::now());
            if cache.len() >= MAX_CACHE {
                if let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, e)| e.created)
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest);
                }
            }
            cache.insert(
                key.clone(),
                Entry {
                    data: data.clone(),
                    created: Instant::now(),
                    expires: Instant::now() + Duration::from_secs(ttl),
                },
            );
        }
        // Existing waiters share the result, including failures, without retaining it as stale data.
        *guard = Some(result.clone());
        let mut flights = self.flights.lock().unwrap_or_else(|e| e.into_inner());
        if flights
            .get(&key)
            .map(|existing| Arc::ptr_eq(existing, &flight))
            .unwrap_or(false)
        {
            flights.remove(&key);
        }
        result
    }
    async fn fetch(
        &self,
        mut url: Url,
        timeout: u64,
        allowed: fn(&Url) -> bool,
        range: Option<&str>,
        media: bool,
    ) -> Result<Response> {
        for attempt in 0..=5 {
            if !allowed(&url) {
                return Err("Unsupported upstream URL".into());
            }
            let mut request = self
                .client
                .get(url.clone())
                .header("Accept-Language", "ar,en-US;q=0.9,en;q=0.8");
            if media {
                request = request
                    .header("Origin", "https://ok.ru")
                    .header("Referer", "https://ok.ru/");
            } else {
                request = request.header("Referer", format!("{ORIGIN}/"));
            }
            if let Some(value) = range {
                request = request.header("Range", value);
            }
            let response = tokio::time::timeout(Duration::from_secs(timeout), request.send())
                .await
                .map_err(|_| "Provider timeout".to_owned())?
                .map_err(|_| "Provider unavailable".to_owned())?;
            if response.status().is_redirection() {
                if attempt == 5 {
                    return Err("Too many redirects".into());
                }
                url = url
                    .join(
                        response
                            .headers()
                            .get("Location")
                            .and_then(|v| v.to_str().ok())
                            .ok_or("Invalid redirect")?,
                    )
                    .map_err(|_| "Invalid redirect")?;
                continue;
            }
            if !response.status().is_success() {
                return Err(format!("Provider HTTP {}", response.status().as_u16()));
            }
            return Ok(response);
        }
        Err("Provider unavailable".into())
    }
    async fn text(&self, url: Url, timeout: u64, allowed: fn(&Url) -> bool) -> Result<String> {
        tokio::time::timeout(Duration::from_secs(timeout), async {
            let response = self.fetch(url, timeout, allowed, None, false).await?;
            let bytes = limited_body(response, MAX_TEXT).await?;
            String::from_utf8(bytes).map_err(|_| "Invalid provider encoding".into())
        })
        .await
        .map_err(|_| "Provider timeout".to_owned())?
    }
    pub async fn request(&self, full_url: &str) -> ApiResponse {
        let url = match Url::parse(full_url)
            .or_else(|_| Url::parse(&format!("https://appassets.androidplatform.net{full_url}")))
        {
            Ok(v) => v,
            Err(_) => return ApiResponse::error(400, "Invalid request"),
        };
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let get = |key: &str| params.get(key).map(String::as_str).unwrap_or("");
        let page = get("page").parse::<u32>().unwrap_or(1).clamp(1, 9999);
        let path = url.path();
        if path == "/api/health" {
            return ApiResponse::ok(
                json!({"ok":true,"engine":"rust","version":env!("CARGO_PKG_VERSION"),"uptimeSeconds":self.started.elapsed().as_secs(),"cacheEntries":self.cache.lock().unwrap_or_else(|e|e.into_inner()).len()}),
            );
        }
        let (ttl, key) = match path {
            "/api/home" => (120, format!("home:{page}")),
            "/api/series" => (300, format!("series:{page}")),
            "/api/episodes" => (120, format!("episodes:{page}")),
            "/api/search" => (300, format!("search:{page}:{}", get("q").trim())),
            "/api/series/detail" => (600, format!("series:{}", get("url"))),
            "/api/episode/detail" => (45, format!("episode:{}:{}", get("url"), get("server"))),
            "/api/cinemana" => (0, String::new()),
            _ => return ApiResponse::error(404, "Unknown endpoint"),
        };
        if path.ends_with("/detail") && safe_krmzi(get("url")).is_none() {
            return ApiResponse::error(400, "Invalid provider URL");
        }
        if path == "/api/cinemana" && !valid_cinemana_path(get("path")) {
            return ApiResponse::error(400, "Invalid provider request");
        }
        let producer = || async {
            match path {
                "/api/home" => {
                    // A failed recent-episodes row must not hide the complete series catalogue.
                    let (episodes, series) =
                        tokio::join!(self.list("episodes", page), self.list("series", page));
                    match (episodes, series) {
                        (Err(_), Err(error)) => Err(error),
                        (episodes, series) => Ok(
                            json!({"episodes":episodes.unwrap_or_else(|e|json!({"page":page,"episodes":[],"hasNext":false,"error":e})),"series":series.unwrap_or_else(|e|json!({"page":page,"series":[],"hasNext":false,"error":e})),"isSuccess":true}),
                        ),
                    }
                }
                "/api/series" => self.list("series", page).await,
                "/api/episodes" => self.list("episodes", page).await,
                "/api/search" => self.search(get("q"), page).await,
                "/api/series/detail" => self.series_detail(get("url")).await,
                "/api/episode/detail" => self.episode_detail(get("url"), get("server")).await,
                "/api/cinemana" => {
                    let text = self
                        .text(
                            cinemana_url(get("path")).ok_or("Invalid provider route")?,
                            8,
                            cinemana_host,
                        )
                        .await
                        .map_err(|_| "cinemana_unavailable")?;
                    serde_json::from_str(&text).map_err(|_| "Invalid provider response".into())
                }
                _ => Err("Unknown endpoint".into()),
            }
        };
        let result = if ttl == 0 {
            producer().await
        } else {
            self.cached(key, ttl, get("fresh") == "1", producer).await
        };
        match result {
            Ok(value) => ApiResponse::ok(value),
            Err(error) => {
                ApiResponse::error(if path == "/api/cinemana" { 503 } else { 502 }, &error)
            }
        }
    }
    async fn list(&self, kind: &str, page: u32) -> Result<Value> {
        let base = if kind == "series" {
            format!("{ORIGIN}/series-list/")
        } else {
            format!("{ORIGIN}/")
        };
        let target = if page == 1 {
            base
        } else {
            format!("{base}page/{page}/")
        };
        let html = self
            .text(
                Url::parse(&target).map_err(|_| "Invalid URL")?,
                12,
                krmzi_host,
            )
            .await?;
        let cards = parse_cards(&html, kind);
        let has_next = has_next_page(&html, page, kind == "series");
        Ok(
            json!({"page":page,kind:cards,"hasNext":has_next,"nextPage":if has_next{Some(page+1)}else{None},"isSuccess":true}),
        )
    }
    async fn search(&self, query: &str, page: u32) -> Result<Value> {
        let query = query.trim().chars().take(120).collect::<String>();
        if query.is_empty() {
            return Ok(
                json!({"query":"","page":page,"results":[],"hasNext":false,"nextPage":null}),
            );
        }
        let mut url = Url::parse(&if page == 1 {
            format!("{ORIGIN}/")
        } else {
            format!("{ORIGIN}/page/{page}/")
        })
        .unwrap();
        url.query_pairs_mut().append_pair("s", &query);
        let html = self.text(url, 12, krmzi_host).await?;
        let results = parse_cards(&html, "search");
        let has_next = has_next_page(&html, page, false);
        Ok(
            json!({"query":query,"page":page,"results":results,"hasNext":has_next,"nextPage":if has_next{Some(page+1)}else{None}}),
        )
    }
    async fn series_detail(&self, url: &str) -> Result<Value> {
        let html = self
            .text(safe_krmzi(url).ok_or("Invalid series URL")?, 12, krmzi_host)
            .await?;
        Ok(parse_series_detail(&html, url))
    }
    async fn ok_streams(&self, id: &str) -> Result<Vec<Value>> {
        if !compiled(r"^[0-9]+$").is_match(id) {
            return Err("Invalid mirror ID".into());
        }
        let html = self
            .text(
                Url::parse(&format!("https://ok.ru/videoembed/{id}")).unwrap(),
                6,
                ok_host,
            )
            .await?;
        Ok(parse_ok_streams(&html))
    }
    async fn episode_detail(&self, url: &str, selected_server: &str) -> Result<Value> {
        let html = self
            .text(
                safe_krmzi(url).ok_or("Invalid episode URL")?,
                12,
                krmzi_host,
            )
            .await?;
        let mut result = parse_episode_detail(&html, url);
        let servers = result["servers"].as_array().cloned().unwrap_or_default();
        if !selected_server.is_empty()
            && !servers
                .iter()
                .any(|s| s["id"].as_str() == Some(selected_server))
        {
            return Err("Requested mirror is no longer listed for this episode".into());
        }
        let pending = FuturesUnordered::new();
        for server in servers
            .iter()
            .filter(|s| s["name"].as_str().unwrap_or("").eq_ignore_ascii_case("ok"))
            .filter(|s| selected_server.is_empty() || s["id"].as_str() == Some(selected_server))
            .take(3)
        {
            let id = server["id"].as_str().unwrap_or("").to_owned();
            pending.push(async move {
                let streams = self.ok_streams(&id).await.unwrap_or_default();
                (id, streams)
            });
        }
        let (chosen, streams) = first_playable(pending).await;
        result["directStreams"] = json!(streams);
        if let Some(servers) = result["servers"].as_array_mut() {
            for server in servers.iter_mut() {
                server["hasDirectStream"] =
                    json!(!chosen.is_empty() && server["id"].as_str() == Some(&chosen));
            }
            servers.sort_by_key(|s| !s["hasDirectStream"].as_bool().unwrap_or(false));
        }
        Ok(result)
    }
    pub async fn open(&self, full_url: &str, range: Option<&str>) -> Result<OpenResponse> {
        let request = Url::parse(full_url)
            .or_else(|_| Url::parse(&format!("https://appassets.androidplatform.net{full_url}")))
            .map_err(|_| "Invalid request")?;
        let query: HashMap<String, String> = request.query_pairs().into_owned().collect();
        let target = query.get("url").ok_or("Missing media URL")?;
        if request.path() == "/api/cinemana/subtitle" {
            let url = safe_subtitle(target).ok_or("Invalid subtitle URL")?;
            let response = self.fetch(url, 8, subtitle_host, None, false).await?;
            let bytes = tokio::time::timeout(
                Duration::from_secs(8),
                limited_body(response, 2 * 1024 * 1024),
            )
            .await
            .map_err(|_| "Subtitle timeout")??;
            return Ok(OpenResponse::bytes(200, "text/vtt", bytes));
        }
        if request.path() != "/api/media" {
            return Err("Unsupported stream request".into());
        }
        let url = safe_media(target).ok_or("Invalid media URL")?;
        let response = self.fetch(url, 12, media_host, range, true).await?;
        let status = response.status().as_u16();
        let mime = response
            .headers()
            .get("Content-Type")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("video/mp2t")
            .split(';')
            .next()
            .unwrap_or("video/mp2t")
            .to_owned();
        let mut headers = BTreeMap::from([
            ("Cache-Control".into(), "no-store".into()),
            ("Access-Control-Allow-Origin".into(), "*".into()),
        ]);
        for name in ["Content-Range", "Accept-Ranges"] {
            if let Some(value) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
                headers.insert(name.into(), value.into());
            }
        }
        if mime.to_lowercase().contains("mpegurl") || response.url().path().ends_with(".m3u8") {
            let base = response.url().clone();
            let text = String::from_utf8(
                tokio::time::timeout(Duration::from_secs(12), limited_body(response, MAX_TEXT))
                    .await
                    .map_err(|_| "Playlist timeout")??,
            )
            .map_err(|_| "Invalid playlist")?;
            return Ok(OpenResponse::bytes(
                200,
                "application/vnd.apple.mpegurl",
                rewrite_playlist(&text, &base)?.into_bytes(),
            ));
        }
        Ok(OpenResponse {
            status,
            mime,
            headers,
            body: StreamBody::Network {
                response,
                pending: Vec::new(),
                offset: 0,
            },
        })
    }
}

fn cacheable(value: &Value) -> bool {
    value.get("error").is_none()
        && value.get("isSuccess") != Some(&Value::Bool(false))
        && ["episodes", "series"].iter().all(|key| {
            value
                .get(key)
                .map(|child| !child.is_object() || child.get("error").is_none())
                .unwrap_or(true)
        })
}

pub struct ApiResponse {
    pub status: u16,
    pub value: Value,
}
impl ApiResponse {
    fn ok(value: Value) -> Self {
        Self { status: 200, value }
    }
    fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            value: json!({"error":message,"isSuccess":false}),
        }
    }
}
pub struct OpenResponse {
    pub status: u16,
    pub mime: String,
    pub headers: BTreeMap<String, String>,
    pub body: StreamBody,
}
impl OpenResponse {
    fn bytes(status: u16, mime: &str, bytes: Vec<u8>) -> Self {
        Self {
            status,
            mime: mime.into(),
            headers: BTreeMap::from([
                ("Cache-Control".into(), "no-store".into()),
                ("Access-Control-Allow-Origin".into(), "*".into()),
            ]),
            body: StreamBody::Bytes(Cursor::new(bytes)),
        }
    }
}
pub enum StreamBody {
    Bytes(Cursor<Vec<u8>>),
    Network {
        response: Response,
        pending: Vec<u8>,
        offset: usize,
    },
}
impl StreamBody {
    pub async fn read(&mut self, size: usize) -> Result<Vec<u8>> {
        let size = size.clamp(1, 262144);
        match self {
            StreamBody::Bytes(cursor) => {
                let mut bytes = vec![0; size];
                let n = cursor.read(&mut bytes).map_err(|_| "Read failed")?;
                bytes.truncate(n);
                Ok(bytes)
            }
            StreamBody::Network {
                response,
                pending,
                offset,
            } => {
                if *offset >= pending.len() {
                    match tokio::time::timeout(Duration::from_secs(20), response.chunk())
                        .await
                        .map_err(|_| "Media read timeout")?
                        .map_err(|_| "Media read failed")?
                    {
                        Some(chunk) => {
                            *pending = chunk.to_vec();
                            *offset = 0;
                        }
                        None => return Ok(Vec::new()),
                    }
                }
                let end = (*offset + size).min(pending.len());
                let bytes = pending[*offset..end].to_vec();
                *offset = end;
                Ok(bytes)
            }
        }
    }
}
async fn limited_body(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "Provider read failed")? {
        if result.len() + chunk.len() > limit {
            return Err("Provider response too large".into());
        }
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}

fn allowed_authority(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
}
fn krmzi_host(url: &Url) -> bool {
    allowed_authority(url)
        && matches!(
            url.host_str(),
            Some(
                "krmzi.org"
                    | "www.krmzi.org"
                    | "krmzi.net"
                    | "www.krmzi.net"
                    | "krmzi.tv"
                    | "www.krmzi.tv"
            )
        )
}
fn ok_host(url: &Url) -> bool {
    allowed_authority(url) && matches!(url.host_str(), Some("ok.ru" | "www.ok.ru"))
}
fn cinemana_host(url: &Url) -> bool {
    allowed_authority(url) && url.host_str() == Some("cinemana.shabakaty.com")
}
fn media_host(url: &Url) -> bool {
    allowed_authority(url)
        && url
            .host_str()
            .map(|h| {
                ["okcdn.ru", "vkuser.net"]
                    .iter()
                    .any(|base| h == *base || h.ends_with(&format!(".{base}")))
            })
            .unwrap_or(false)
}
fn subtitle_host(url: &Url) -> bool {
    allowed_authority(url)
        && compiled(r"^cnth[0-9]*\.shabakaty\.com$").is_match(url.host_str().unwrap_or(""))
        && compiled(r"^/vascin-translation-files/[A-Za-z0-9_-]+\.vtt$").is_match(url.path())
}
pub fn safe_krmzi(raw: &str) -> Option<Url> {
    let mut u = Url::parse(raw.trim()).ok()?;
    if u.scheme() == "http" {
        u.set_scheme("https").ok()?;
    }
    if !krmzi_host(&u) {
        return None;
    }
    u.set_fragment(None);
    Some(u)
}
pub fn safe_media(raw: &str) -> Option<Url> {
    Url::parse(raw).ok().filter(media_host)
}
pub fn safe_subtitle(raw: &str) -> Option<Url> {
    Url::parse(raw).ok().filter(subtitle_host)
}
pub fn valid_cinemana_path(path: &str) -> bool {
    let allowed = compiled(r"^(latestSeries|latestMovies)/level/0/itemsPerPage/24/page/[0-9]{1,4}/$").is_match(path)
        || compiled(r"^(allVideoInfo|videoSeason|transcoddedFiles)/id/[0-9]+$").is_match(path)
        || compiled(r"^video/V/2/itemsPerPage/20/video_title_search/[^/?#]+/itemsPerPage/12/pageNumber/[0-9]{1,4}/level/0$").is_match(path);
    allowed
        && Url::parse(&format!("{CINEMANA}{path}"))
            .map(|u| u.path() == format!("/api/android/{path}"))
            .unwrap_or(false)
}
fn cinemana_url(path: &str) -> Option<Url> {
    if !valid_cinemana_path(path) {
        return None;
    }
    Url::parse(&format!("{CINEMANA}{path}")).ok()
}

async fn first_playable<F>(mut pending: FuturesUnordered<F>) -> (String, Vec<Value>)
where
    F: std::future::Future<Output = (String, Vec<Value>)>,
{
    while let Some((id, streams)) = pending.next().await {
        if !streams.is_empty() {
            return (id, streams);
        }
    }
    // Returning drops pending futures, including their in-flight network requests.
    (String::new(), Vec::new())
}
pub fn rewrite_playlist(text: &str, base: &Url) -> Result<String> {
    let proxy = |uri: &str| -> Result<String> {
        let target = base.join(uri).map_err(|_| "Invalid media reference")?;
        if !media_host(&target) {
            return Err("Unsupported media host".into());
        }
        Ok(format!("/api/media?url={}", encode_uri(target.as_str())))
    };
    let regex = compiled(r#"URI="([^"]+)""#);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                return Ok(line.to_owned());
            }
            if !line.starts_with('#') {
                return proxy(line.trim());
            }
            let mut out = String::new();
            let mut start = 0;
            for cap in regex.captures_iter(line) {
                let m = cap.get(0).unwrap();
                out.push_str(&line[start..m.start()]);
                out.push_str(&format!("URI=\"{}\"", proxy(&cap[1])?));
                start = m.end();
            }
            out.push_str(&line[start..]);
            Ok(out)
        })
        .collect::<Result<Vec<_>>>()
        .map(|lines| lines.join("\n"))
}
pub fn encode_uri(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
// All callers provide fixed source patterns; cache their compilation across cards/episodes.
fn compiled(pattern: &'static str) -> Arc<Regex> {
    static PATTERNS: OnceLock<Mutex<HashMap<&'static str, Arc<Regex>>>> = OnceLock::new();
    let mut patterns = PATTERNS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    patterns
        .entry(pattern)
        .or_insert_with(|| Arc::new(Regex::new(pattern).expect("Fixed scraper pattern")))
        .clone()
}
pub fn decode_html(text: &str) -> String {
    compiled(r"&(#x?[0-9a-fA-F]+|[A-Za-z]+);")
        .replace_all(text, |c: &regex::Captures| {
            let e = &c[1];
            if let Some(hex) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                return u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| c[0].into());
            }
            if let Some(n) = e.strip_prefix('#') {
                return n
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| c[0].into());
            }
            match e.to_ascii_lowercase().as_str() {
                "amp" => "&",
                "quot" => "\"",
                "apos" => "'",
                "lt" => "<",
                "gt" => ">",
                "nbsp" => " ",
                "hellip" => "…",
                "laquo" => "«",
                "raquo" => "»",
                "ndash" => "–",
                "mdash" => "—",
                _ => return c[0].into(),
            }
            .to_owned()
        })
        .into_owned()
}
fn strip_tags(text: &str) -> String {
    compiled(r"\s+")
        .replace_all(
            &decode_html(&compiled(r"<[^>]*>").replace_all(text, " ")),
            " ",
        )
        .trim()
        .into()
}
fn clean_title(text: &str) -> String {
    compiled(r"\s*-\s*قرمزي\s*$")
        .replace(&strip_tags(text), "")
        .trim()
        .into()
}
fn capture(pattern: &'static str, text: &str) -> String {
    compiled(pattern)
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().into())
        .unwrap_or_default()
}
fn decode64(text: &str) -> Option<String> {
    [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD]
        .iter()
        .find_map(|engine| {
            engine
                .decode(text)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
}
pub fn extract_real_url(raw: &str) -> String {
    let raw = decode_html(raw);
    if safe_krmzi(&raw).is_some() {
        return raw;
    }
    if let Ok(url) = Url::parse(&raw) {
        for pair in url.query().unwrap_or("").split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if !["url", "link", "target"].contains(&key) {
                continue;
            }
            // Base64 '+' is data here, not an HTML-form space.
            let value = decode_percent(value);
            let value = if value.starts_with("http") {
                value
            } else {
                decode64(&value).unwrap_or_default()
            };
            if let Some(url) = safe_krmzi(&value) {
                return url.into();
            }
        }
    }
    raw
}
fn poster(html: &str) -> String {
    let image = capture(
        r#"url\(['"]?(https://krmzi\.org/wp-content/uploads/[^)'"\s]+)['"]?\)"#,
        html,
    );
    if !image.is_empty() {
        decode_html(&image)
    } else {
        decode_html(&capture(
            r#"<meta[^>]+property=["']og:image["'][^>]+content=["']([^"']+)["']"#,
            html,
        ))
    }
}
pub fn parse_cards(html: &str, kind: &str) -> Vec<Value> {
    let blocks = compiled(r#"(?s)<div\s+class=["']block-post["']>(.*?)</div>\s*</div>"#);
    let links = compiled(r#"(?s)<a\s+[^>]*href=["']([^"']+)["'][^>]*title=["']([^"']+)["']"#);
    let thumbs = compiled(r#"url\(['"]?(https://krmzi\.org/wp-content/uploads/[^)'"\s]+)['"]?\)"#);
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let episode_tail = compiled(r"الحلقة\s*\d+.*");
    for block in blocks.captures_iter(html) {
        let Some(link) = links.captures_iter(&block[1]).find(|c| {
            let url = extract_real_url(&c[1]);
            safe_krmzi(&url).is_some()
                && (kind != "series" || url.contains("/series/"))
                && (kind != "search" || url.contains("/series/") || url.contains("/episode/"))
        }) else {
            continue;
        };
        let url = extract_real_url(&link[1]);
        if !seen.insert(url.clone()) {
            continue;
        }
        let title = clean_title(&link[2]);
        let thumbnail = thumbs
            .captures(&block[1])
            .map(|c| decode_html(&c[1]))
            .unwrap_or_default();
        let number = capture(
            r#"(?s)<div\s+class=["']episodeNum["']>.*?<span>(\d+)</span>"#,
            &block[1],
        );
        let series_title = episode_tail.replace(&title, "").trim().to_owned();
        let media_type = if kind == "series" || url.contains("/series/") {
            "series"
        } else {
            "episode"
        };
        let is_dubbed = block[1].contains("مدبلج") || url.contains("dubbed") || title.contains("مدبلج");
        let edition = if is_dubbed { "dubbed" } else { "original" };
        result.push(json!({"url":url,"title":title,"thumbnail":thumbnail,"source":"krmzi","type":media_type,"seriesTitle":series_title,"episodeNum":number,"edition":edition}));
    }
    result
}
pub fn has_next_page(html: &str, page: u32, series: bool) -> bool {
    let marker = if series {
        format!("series-list/page/{}/", page + 1)
    } else {
        format!("page/{}/", page + 1)
    };
    html.contains(&marker)
        || Regex::new(&format!(r#"[?&](?:amp;)?paged={}([&#"']|$)"#, page + 1))
            .unwrap()
            .is_match(html)
        || compiled(r#"class=["'][^"']*\bnext\b[^"']*["']"#).is_match(html)
}
pub fn parse_series_detail(html: &str, url: &str) -> Value {
    let raw_h1 = capture(r"(?s)<h1[^>]*>(.*?)</h1>", html);
    let title = clean_title(&raw_h1);
    let story = strip_tags(&capture(
        r#"(?s)<div\s+class=["'](?:story|desc)["']>(.*?)</div>"#,
        html,
    ));
    let is_dubbed = html.contains("مدبلج") || url.contains("dubbed") || raw_h1.contains("مدبلج");
    let edition = if is_dubbed { "dubbed" } else { "original" };
    let anchors = compiled(r#"(?s)<a\s+[^>]*href=["']([^"']+)["'][^>]*>(.*?)</a>"#);
    let mut seen = HashSet::new();
    let mut episodes = Vec::new();
    for c in anchors.captures_iter(html) {
        let link = extract_real_url(&c[1]);
        if !link.contains("/episode/") || safe_krmzi(&link).is_none() || !seen.insert(link.clone())
        {
            continue;
        }
        let ep_title = strip_tags(&c[2]);
        let decoded = Url::parse(&link)
            .ok()
            .map(|u| decode_percent(u.path()))
            .unwrap_or_else(|| link.clone());
        let mut num = capture(r"الحلقة[-\s]*(\d+)", &decoded);
        if num.is_empty() {
            num = capture(r"(\d+)", &ep_title);
        }
        episodes.push(json!({
            "url": link,
            "title": if ep_title.is_empty() { format!("الحلقة {num}") } else { ep_title },
            "seriesTitle": &title,
            "episodeNum": num,
            "source": "krmzi",
            "seriesUrl": url,
            "edition": edition,
        }));
    }
    episodes.sort_by_key(|e| {
        e["episodeNum"]
            .as_str()
            .unwrap_or("")
            .parse::<u32>()
            .unwrap_or(0)
    });
    json!({
        "url": url,
        "title": title,
        "poster": poster(html),
        "story": story,
        "episodes": episodes,
        "source": "krmzi",
        "edition": edition,
        "isSuccess": true
    })
}
fn decode_percent(text: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", text.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| text.into())
}
pub fn parse_ok_streams(html: &str) -> Vec<Value> {
    let text = decode_html(&capture(r#"data-options="([^"]+)""#, html));
    let Ok(options) = serde_json::from_str::<Value>(&text) else {
        return vec![];
    };
    let metadata = &options["flashvars"]["metadata"];
    let metadata = metadata
        .as_str()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(|| metadata.clone());
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    if let Some(url) = metadata["hlsManifestUrl"]
        .as_str()
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
    {
        result.push(json!({"quality":"تلقائي","type":"hls","url":url}));
        seen.insert(url.to_owned());
    }
    if let Some(videos) = metadata["videos"].as_array() {
        for (name, label) in [
            ("full", "1080p (Full HD)"),
            ("hd", "720p (HD)"),
            ("sd", "480p (SD)"),
            ("low", "360p"),
            ("lowest", "240p"),
            ("mobile", "144p"),
        ] {
            if let Some(video) = videos.iter().find(|v| v["name"] == name) {
                if let Some(url) = video["url"]
                    .as_str()
                    .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
                {
                    if seen.insert(url.into()) {
                        result.push(json!({"quality":label,"url":url}));
                    }
                }
            }
        }
    }
    result
}
pub fn embed_url(name: &str, id: &str) -> String {
    match name.to_lowercase().as_str() {
        "ok" => format!("https://ok.ru/videoembed/{id}"),
        "arab hd" => format!("https://arabhd.onl/embed-{id}.html"),
        "estream" => format!("https://arabveturk.com/embed-{id}.html"),
        "red hd" => format!("https://iplayerhls.com/e/{id}"),
        "box" => format!("https://youdboox.com/embed-{id}.html"),
        "now" => format!("https://extreamnow.org/embed-{id}.html"),
        "pro hd" => format!("https://embedo.co/e/{id}"),
        "dailymotion" => format!("https://www.dailymotion.com/embed/video/{id}"),
        "youtube" => format!("https://www.youtube.com/embed/{id}"),
        "express" if !id.starts_with("http") => format!("https://cloud.mail.ru/public/{id}"),
        _ if id.starts_with("https://") || id.starts_with("http://") => id.into(),
        _ => String::new(),
    }
}
pub fn parse_episode_detail(html: &str, url: &str) -> Value {
    let raw_h1 = capture(r"(?s)<h1[^>]*>(.*?)</h1>", html);
    let mut title = clean_title(&raw_h1);
    if title.is_empty() {
        title = clean_title(&capture(r"(?s)<title>(.*?)</title>", html));
    }
    let episode = capture(
        r"الحلقة[-\s]*(\d+)",
        &format!("{} {title} {raw_h1}", decode_percent(url)),
    );
    let series = decode_html(&capture(
        r#"<a[^>]+href=["'](https://krmzi\.org/series/[^"']+)["']"#,
        html,
    ));
    let is_dubbed = html.contains("مدبلج") || url.contains("dubbed") || raw_h1.contains("مدبلج");
    let edition = if is_dubbed { "dubbed" } else { "original" };
    let series_title = {
        let re = compiled(r"الحلقة[-\s]*\d+.*");
        let s = re.replace(&title, "");
        let trimmed = s.trim();
        if trimmed.is_empty() {
            title.clone()
        } else {
            trimmed.to_string()
        }
    };
    let qesen = decode_html(&capture(
        r#"href=["']https://qesen\.net/krmzi\?post=([^"']+)["']"#,
        html,
    ));
    let mut raw = decode64(&qesen)
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v["servers"].as_array().cloned())
        .unwrap_or_default();
    if raw.is_empty() {
        for c in compiled(r#"<li[^>]*data-name=["']([^"']*)["'][^>]*data-server=["']([^"']*)["']"#).captures_iter(html) {
            raw.push(json!({"name":decode_html(&c[1]),"id":decode_html(&c[2])}));
        }
    }
    if raw.is_empty() {
        let iframe = decode_html(&capture(r#"<iframe[^>]+src=["']([^"']+)["']"#, html));
        if !iframe.is_empty() {
            raw.push(json!({"name":"سيرفر رئيسي","id":iframe}));
        }
    }
    let mut servers = Vec::new();
    let mut seen = HashSet::new();
    for server in raw {
        let name_value = scalar_string(&server["name"]);
        let id_value = scalar_string(&server["id"]);
        let name = name_value.trim();
        let id = id_value.trim();
        if id.is_empty() || !seen.insert((name.to_owned(), id.to_owned())) {
            continue;
        }
        let embed = embed_url(name, id);
        if embed.is_empty() {
            continue;
        }
        servers.push(json!({"name":name,"id":id,"embedUrl":embed,"hasDirectStream":false}));
    }
    json!({
        "url": url,
        "title": if episode.is_empty() { title.clone() } else { format!("الحلقة {episode}") },
        "seriesTitle": series_title,
        "episodeNum": episode,
        "poster": poster(html),
        "seriesUrl": series,
        "servers": servers,
        "directStreams": [],
        "source": "krmzi",
        "edition": edition,
        "isSuccess": true
    })
}

fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

#[cfg(target_os = "android")]
mod android;

#[cfg(test)]
mod tests;
