use super::*;

#[test]
fn proxy_restricts_authority_and_all_redirect_targets() {
    for url in ["https://vd196.okcdn.ru/v.m3u8", "https://ok6.vkuser.net/x"] {
        assert!(safe_media(url).is_some());
    }
    for url in [
        "http://vd196.okcdn.ru/x",
        "https://okcdn.ru.attacker.test/x",
        "https://localhost/x",
        "https://127.0.0.1/x",
        "https://name@vd196.okcdn.ru/x",
        "https://vd196.okcdn.ru:8443/x",
    ] {
        assert!(safe_media(url).is_none(), "{url}");
    }
}
#[test]
fn playlist_rewrites_children_segments_and_encryption_keys() {
    let result=rewrite_playlist("#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"../key.bin\"\nsegment.ts\nhttps://other.okcdn.ru/child.m3u8",&Url::parse("https://vd196.okcdn.ru/path/master.m3u8").unwrap()).unwrap();
    assert!(result.contains("URI=\"/api/media?url=https%3A%2F%2Fvd196.okcdn.ru%2Fkey.bin\""));
    assert!(result.contains("%2Fpath%2Fsegment.ts"));
    assert!(result.contains("other.okcdn.ru"));
    assert!(rewrite_playlist(
        "#EXTM3U\nhttps://attacker.test/a.ts",
        &Url::parse("https://vd196.okcdn.ru/a.m3u8").unwrap()
    )
    .is_err());
}
#[test]
fn catalogue_and_search_support_bounded_pagination() {
    for path in [
        "latestSeries/level/0/itemsPerPage/24/page/12/",
        "latestMovies/level/0/itemsPerPage/24/page/0/",
        "video/V/2/itemsPerPage/20/video_title_search/test/itemsPerPage/12/pageNumber/13/level/0",
        "allVideoInfo/id/123",
    ] {
        assert!(valid_cinemana_path(path), "{path}");
    }
    for path in [
        "latestSeries/level/0/itemsPerPage/24/page/10000/",
        "../../admin",
        "allVideoInfo/id/123?x=1",
        "video/V/2/itemsPerPage/20/video_title_search/a/b/itemsPerPage/12/pageNumber/0/level/0",
    ] {
        assert!(!valid_cinemana_path(path), "{path}");
    }
    assert!(has_next_page(
        "<a href='/series-list/page/3/'>3</a>",
        2,
        true
    ));
    assert!(has_next_page("<a href='/?paged=3'>3</a>", 2, true));
    assert!(!has_next_page(
        "<a href='/series-list/page/2/'>2</a>",
        2,
        true
    ));
}
#[test]
fn cards_decode_wrappers_entities_and_keep_provider_identity() {
    let url = "https://krmzi.org/episode/test/";
    let wrapper = format!("https://sayyarh.com/?url={}", STANDARD.encode(url));
    let html = format!(
        r#"<div class="block-post"><a href="{wrapper}" title="حكاية &amp; قصة الحلقة 3 - قرمزي"><div style="background:url('https://krmzi.org/wp-content/uploads/poster.jpg')"></div><div class="episodeNum"><span>3</span></div></div>"#
    );
    let cards = parse_cards(&html, "episodes");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["url"], url);
    assert_eq!(cards[0]["episodeNum"], "3");
    assert_eq!(cards[0]["seriesTitle"], "حكاية & قصة");
    assert_eq!(cards[0]["source"], "krmzi");
}
#[test]
fn qesen_payload_without_embedded_http_keeps_embed_fallback() {
    let payload = STANDARD.encode(r#"{"servers":[{"name":"ok","id":"123"}]}"#);
    let html = format!(
        r#"<h1>مسلسل الحلقة 3</h1><a href="https://qesen.net/krmzi?post={payload}">Play</a>"#
    );
    let detail = parse_episode_detail(&html, "https://krmzi.org/episode/test/");
    assert_eq!(detail["episodeNum"], "3");
    assert_eq!(
        detail["servers"][0]["embedUrl"],
        "https://ok.ru/videoembed/123"
    );
    assert_eq!(detail["directStreams"], json!([]));
}

#[test]
fn numeric_mirror_ids_and_plus_in_base64_wrappers_are_preserved() {
    let payload = STANDARD.encode(r#"{"servers":[{"name":"ok","id":123}]}"#);
    let html = format!(r#"<a href="https://qesen.net/krmzi?post={payload}">Play</a>"#);
    let detail = parse_episode_detail(&html, "https://krmzi.org/episode/test/");
    assert_eq!(detail["servers"][0]["id"], "123");
    let (target, payload) = (0..6)
        .map(|n| {
            let target = format!("https://krmzi.org/episode/{}>", "a".repeat(n));
            let payload = STANDARD.encode(&target);
            (target, payload)
        })
        .find(|(_, payload)| payload.contains('+'))
        .expect("Base64 plus fixture");
    let expected = safe_krmzi(&target).unwrap().to_string();
    assert_eq!(
        extract_real_url(&format!("https://wrapper.test/?url={payload}")),
        expected
    );
}
#[test]
fn adaptive_stream_is_first_and_works_without_mp4() {
    for videos in [
        json!([]),
        json!([{"name":"hd","url":"https://video.test/720.mp4"}]),
    ] {
        let meta = json!({"videos":videos,"hlsManifestUrl":"https://video.test/master.m3u8"});
        let options = json!({"flashvars":{"metadata":meta.to_string()}})
            .to_string()
            .replace('"', "&quot;");
        let streams = parse_ok_streams(&format!(r#"<div data-options="{options}"></div>"#));
        assert_eq!(streams[0]["type"], "hls");
        assert_eq!(streams.len(), 1 + videos.as_array().unwrap().len());
    }
}
#[test]
fn episode_order_and_url_number_work_with_percent_encoded_arabic() {
    let html = r#"<h1>حكاية</h1><a href="https://krmzi.org/episode/%D8%A7%D9%84%D8%AD%D9%84%D9%82%D8%A9-10/">10</a><a href="https://krmzi.org/episode/الحلقة-2/">2</a>"#;
    let details = parse_series_detail(html, "https://krmzi.org/series/test/");
    assert_eq!(details["episodes"][0]["episodeNum"], "2");
    assert_eq!(details["episodes"][1]["episodeNum"], "10");
}
#[test]
fn expired_signed_link_is_not_returned_on_refresh_failure() {
    let engine = Engine::new();
    engine.cache.lock().unwrap().insert(
        "signed".into(),
        Entry {
            data: json!({"url":"expired"}),
            expires: Instant::now() - Duration::from_secs(1),
            created: Instant::now(),
        },
    );
    let result = runtime().block_on(engine.cached("signed".into(), 45, false, || async {
        Err("provider down".into())
    }));
    assert!(result.is_err());
}
#[test]
fn concurrent_cache_misses_share_resolution() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let count = AtomicUsize::new(0);
    let engine = Engine::new();
    runtime().block_on(async {
        let producer = || async {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(json!({"ok":true}))
        };
        let (a, b) = tokio::join!(
            engine.cached("same".into(), 45, false, producer),
            engine.cached("same".into(), 45, false, producer)
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(engine.flights.lock().unwrap().is_empty());
    });
}

#[test]
fn concurrent_failures_are_shared_without_caching_an_empty_catalogue() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let count = AtomicUsize::new(0);
    let engine = Engine::new();
    runtime().block_on(async {
        let producer = || async {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Err("provider down".into())
        };
        let (a, b) = tokio::join!(
            engine.cached("failed".into(), 120, false, producer),
            engine.cached("failed".into(), 120, false, producer)
        );
        assert!(a.is_err() && b.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(engine.flights.lock().unwrap().is_empty());
        let degraded = json!({"episodes":{"episodes":[]},"series":{"series":[],"error":"offline"}});
        engine
            .cached("home".into(), 120, false, || async { Ok(degraded.clone()) })
            .await
            .unwrap();
        assert!(!engine.cache.lock().unwrap().contains_key("home"));
    });
}
#[test]
fn subtitle_proxy_accepts_only_expected_vtt_path() {
    assert!(
        safe_subtitle("https://cnth2.shabakaty.com/vascin-translation-files/ar_123.vtt").is_some()
    );
    for url in [
        "https://cnth2.shabakaty.com/other/file.vtt",
        "https://cnth2.shabakaty.com.attacker.test/vascin-translation-files/a.vtt",
        "https://user@cnth2.shabakaty.com/vascin-translation-files/a.vtt",
    ] {
        assert!(safe_subtitle(url).is_none());
    }
}
#[test]
fn byte_stream_is_chunked_and_closes_at_eof() {
    let mut body = StreamBody::Bytes(Cursor::new(b"abcdef".to_vec()));
    runtime().block_on(async {
        assert_eq!(body.read(2).await.unwrap(), b"ab");
        assert_eq!(body.read(20).await.unwrap(), b"cdef");
        assert!(body.read(2).await.unwrap().is_empty());
    });
}

#[test]
fn first_usable_mirror_cancels_a_hanging_mirror_and_ignores_empty_results() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    async fn mirror(id: &str, delay: u64, usable: bool, guard: Dropped) -> (String, Vec<Value>) {
        let _guard = guard;
        tokio::time::sleep(Duration::from_millis(delay)).await;
        (
            id.into(),
            if usable {
                vec![json!({"url":"https://video.test/720.mp4"})]
            } else {
                vec![]
            },
        )
    }
    let slow_dropped = Arc::new(AtomicBool::new(false));
    let pending = FuturesUnordered::new();
    pending.push(mirror("slow", 3000, true, Dropped(slow_dropped.clone())));
    pending.push(mirror(
        "empty",
        0,
        false,
        Dropped(Arc::new(AtomicBool::new(false))),
    ));
    pending.push(mirror(
        "ready",
        10,
        true,
        Dropped(Arc::new(AtomicBool::new(false))),
    ));
    let started = Instant::now();
    let (id, streams) = runtime().block_on(first_playable(pending));
    assert_eq!(id, "ready");
    assert_eq!(streams.len(), 1);
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(slow_dropped.load(Ordering::SeqCst));
}

#[test]
fn dean_edwards_packer_unpacks_hls_streams() {
    let packed = r#"eval(function(p,a,c,k,e,d){while(c--)if(k[c])p=p.replace(new RegExp('\\b'+c.toString(a)+'\\b','g'),k[c]);return p}('0("1").2({3:[{4:"5://6.7/8.9"}]});',10,10,'jwplayer|vplayer|setup|sources|file|https|cdn.test|com|master|m3u8'.split('|')))"#;
    let unpacked = unpack_dean_edwards(packed).expect("Should unpack successfully");
    assert!(unpacked.contains("https://cdn.test.com/master.m3u8"));
    assert!(unpacked.contains("jwplayer(\"vplayer\").setup"));

    // Missing or corrupted packer returns None
    assert!(unpack_dean_edwards("not packed code").is_none());
    assert!(unpack_dean_edwards("eval(function(p,a,c,k,e,d){}(invalid))").is_none());
}
