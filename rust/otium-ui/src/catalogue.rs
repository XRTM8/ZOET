use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

fn field(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn digit(char: char) -> char {
    match char {
        '\u{0660}'..='\u{0669}' => char::from_u32('0' as u32 + char as u32 - 0x660).unwrap(),
        '\u{06f0}'..='\u{06f9}' => char::from_u32('0' as u32 + char as u32 - 0x6f0).unwrap(),
        _ => char,
    }
}

/// Exact title normalization deliberately avoids fuzzy matching: choosing the
/// wrong remake, episode or dubbed edition is worse than showing a source list.
pub fn normalized(value: &str) -> String {
    let clean: String = value
        .nfkc()
        .flat_map(char::to_lowercase)
        .filter_map(|char| match char {
            '\u{064b}'..='\u{065f}' | '\u{0670}' | '\u{0640}' => None,
            'أ' | 'إ' | 'آ' | 'ٱ' => Some('ا'),
            'ى' => Some('ي'),
            'ç' => Some('c'),
            'ğ' => Some('g'),
            'ı' => Some('i'),
            'ö' => Some('o'),
            'ş' => Some('s'),
            'ü' => Some('u'),
            char if char.is_alphanumeric() => Some(digit(char)),
            _ => Some(' '),
        })
        .collect();
    let mut words: Vec<&str> = clean.split_whitespace().collect();
    if words
        .first()
        .is_some_and(|word| ["مسلسل", "فيلم", "dizi", "film", "series", "movie"].contains(word))
    {
        words.remove(0);
    }
    if let Some(index) = words.windows(2).position(|words| {
        words[0] == "الحلقة"
            && words[1]
                .chars()
                .next()
                .is_some_and(|char| char.is_ascii_digit())
    }) {
        words.truncate(index);
    }
    while let Some(last) = words.last() {
        if [
            "قرمزي", "كامل", "كاملة", "بالعربية", "للعربية", "عربي", "مترجم", "مترجمة", "ترجمة", "الترجمة",
            "مدبلج", "مدبلجة", "دبلجة", "دبلجه", "الدبلجة", "اونلاين", "اون", "لاين",
            "dubbed", "dub", "subbed", "sub", "turkish", "turkce", "bolum", "dizi",
            "season", "sezon", "part", "موسم", "الموسم", "جزء", "الجزء",
            "الاول", "الأول", "الثاني", "الثالث", "الرابع", "الخامس", "السادس", "السابع", "الثامن", "التاسع", "العاشر",
            "اول", "أول", "ثاني", "ثالث", "رابع", "خامس",
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "0",
            "s1", "s2", "s3", "s4", "s5", "s01", "s02", "s03",
        ]
        .contains(last)
        {
            words.pop();
        } else {
            break;
        }
    }
    let mut result = Vec::new();
    let mut index = 0;
    while index < words.len() {
        if words[index] == "و"
            && words.get(index + 1).is_some_and(|word| {
                word.chars()
                    .next()
                    .is_some_and(|char| ('\u{0621}'..='\u{064a}').contains(&char))
            })
        {
            result.push(format!("و{}", words[index + 1]));
            index += 2;
        } else {
            result.push(words[index].to_owned());
            index += 1;
        }
    }
    result.join(" ")
}

pub fn provider(value: &Value) -> &str {
    value["source"]
        .as_str()
        .filter(|source| !source.is_empty())
        .unwrap_or_else(|| {
            if value["url"].as_str().unwrap_or("").starts_with("cinemana:") {
                "cinemana"
            } else {
                "krmzi"
            }
        })
}

/// Dubbed releases may use different episode cuts and lengths. Keep them in a
/// separate identity even when the Arabic display title and year are identical.
pub fn edition(value: &Value) -> &'static str {
    if value["edition"] == "dubbed" {
        return "dubbed";
    }
    if value["edition"] == "original" {
        return "original";
    }
    let mut descriptions = [
        "title",
        "seriesTitle",
        "ar_title",
        "en_title",
        "other_title",
    ]
    .iter()
    .map(|key| field(value, key))
    .collect::<Vec<_>>();
    if let Some(aliases) = value["aliases"].as_array() {
        descriptions.extend(aliases.iter().filter_map(Value::as_str).map(str::to_owned));
    }
    let dubbed = descriptions.iter().any(|description| {
        let lower = description
            .nfkc()
            .flat_map(char::to_lowercase)
            .collect::<String>();
        lower.contains("مدبلج")
            || lower.contains("مدبلجه")
            || lower.contains("دبلجة")
            || lower.contains("دبلجه")
            || lower.contains("الدبلجة")
            || lower.contains("بالدبلجة")
            || lower
                .split(|char: char| !char.is_alphanumeric())
                .any(|word| word == "dubbed" || word == "dub")
    });
    if dubbed {
        "dubbed"
    } else {
        "original"
    }
}

fn kind(value: &Value) -> &'static str {
    if value["type"] == "series"
        || value["mediaType"] == "series"
        || value["url"].as_str().unwrap_or("").contains("/series/")
    {
        "series"
    } else if value["type"] == "movie" || value["mediaType"] == "movie" {
        "movie"
    } else {
        "episode"
    }
}

fn number(value: &str) -> String {
    let digits: String = value.chars().map(digit).collect();
    let trimmed = digits.trim();
    if trimmed
        .chars()
        .all(|char| char.is_ascii_digit() || char == '.')
    {
        if let Ok(number) = trimmed.parse::<f64>() {
            if number.is_finite() {
                return number.to_string();
            }
        }
    }
    trimmed.to_owned()
}

pub fn episode_number(value: &Value) -> String {
    let explicit = field(value, "episodeNum");
    if !explicit.is_empty() {
        return number(&explicit);
    }
    let title: String = field(value, "title").chars().map(digit).collect();
    title
        .split_once("الحلقة")
        .map(|(_, rest)| {
            number(
                &rest
                    .trim_start()
                    .chars()
                    .take_while(|char| char.is_ascii_digit() || *char == '.')
                    .collect::<String>(),
            )
        })
        .unwrap_or_default()
}

pub fn season_from_text(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for (phrase, num) in [
        ("الموسم العاشر", "10"), ("الموسم التاسع", "9"), ("الموسم الثامن", "8"),
        ("الموسم السابع", "7"), ("الموسم السادس", "6"), ("الموسم الخامس", "5"),
        ("الموسم الرابع", "4"), ("الموسم الثالث", "3"), ("الموسم الثاني", "2"),
        ("الموسم الاول", "1"), ("الموسم الأول", "1"),
        ("الجزء العاشر", "10"), ("الجزء التاسع", "9"), ("الجزء الثامن", "8"),
        ("الجزء السابع", "7"), ("الجزء السادس", "6"), ("الجزء الخامس", "5"),
        ("الجزء الرابع", "4"), ("الجزء الثالث", "3"), ("الجزء الثاني", "2"),
        ("الجزء الاول", "1"), ("الجزء الأول", "1"),
    ] {
        if lower.contains(phrase) {
            return Some(num.to_string());
        }
    }
    for marker in ["الموسم", "موسم", "الجزء", "جزء", "season", "s"] {
        if let Some(pos) = lower.find(marker) {
            let rest = lower[pos + marker.len()..].trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                if let Ok(n) = digits.parse::<u32>() {
                    return Some(n.to_string());
                }
            }
        }
    }
    None
}

pub fn season(value: &Value) -> String {
    let explicit = number(&field(value, "season"));
    if !explicit.is_empty() && explicit != "0" {
        return explicit;
    }
    for key in ["title", "seriesTitle", "ar_title", "en_title"] {
        let val = field(value, key);
        if let Some(s) = season_from_text(&val) {
            return s;
        }
    }
    "1".to_owned()
}

pub fn clean_title(title: &str) -> String {
    let mut clean = title.trim();
    for prefix in [
        "مسلسل ", "مسلسلات ", "سلسلة ", "فيلم ", "افلام ", "برنامج ",
        "dizi ", "film ", "series ", "movie ",
    ] {
        if clean.starts_with(prefix) {
            clean = &clean[prefix.len()..];
            break;
        }
    }
    clean = clean.trim();
    loop {
        let prev = clean;
        if let Some(open) = clean.rfind(['(', '[']) {
            if let Some(close) = clean[open..].find([')', ']']) {
                let inside = &clean[open + 1..open + close];
                let inside_lower = inside.to_lowercase();
                if inside_lower.contains("مدبلج")
                    || inside_lower.contains("دبلج")
                    || inside_lower.contains("مترجم")
                    || inside_lower.contains("ترجم")
                    || inside_lower.contains("dub")
                    || inside_lower.contains("sub")
                    || (inside.len() == 4 && inside.chars().all(|c| c.is_ascii_digit()))
                {
                    clean = clean[..open].trim();
                }
            }
        }
        for suffix in [
            " - قرمزي",
            " - مدبلج بالعربية",
            " - مدبلجة بالعربية",
            " - مدبلج للعربية",
            " - مدبلجة للعربية",
            " - دبلجة عربية",
            " - بالدبلجة العربية",
            " - مدبلج",
            " - مدبلجة",
            " - مترجم للعربية",
            " - مترجمة للعربية",
            " - مترجم بالعربية",
            " - مترجمة بالعربية",
            " - مترجم",
            " - مترجمة",
            " مدبلج بالعربية",
            " مدبلجة بالعربية",
            " مدبلج للعربية",
            " مدبلجة للعربية",
            " دبلجة عربية",
            " بالدبلجة العربية",
            " مدبلج",
            " مدبلجة",
            " مترجم بالعربية",
            " مترجمة بالعربية",
            " مترجم للعربية",
            " مترجمة للعربية",
            " مترجم",
            " مترجمة",
            " (مدبلج)",
            " [مدبلج]",
            " (مترجم)",
            " [مترجم]",
            " (Dubbed)",
            " [Dubbed]",
            " (Subbed)",
            " [Subbed]",
            " (dubbed)",
            " [dubbed]",
            " (subbed)",
            " [subbed]",
            " - Dubbed",
            " - Subbed",
            " - dubbed",
            " - subbed",
            " Dubbed",
            " Subbed",
            " dubbed",
            " subbed",
        ] {
            if clean.ends_with(suffix) {
                clean = &clean[..clean.len() - suffix.len()];
                clean = clean.trim();
                break;
            }
        }
        for sep in [" -", " :", " |", " —", " /", "-", ":", "|", "/"] {
            if clean.ends_with(sep) {
                clean = &clean[..clean.len() - sep.len()];
                clean = clean.trim();
                break;
            }
        }
        if clean == prev {
            break;
        }
    }
    clean.trim().to_owned()
}

pub fn clean_series_title(title: &str) -> String {
    let mut clean = title.trim();
    for marker in ["الحلقة", "حلقة", "bolum", "ep.", "episode"] {
        if let Some((before, _)) = clean.split_once(marker) {
            clean = before.trim();
            break;
        }
    }
    let mut clean_str = clean_title(clean);
    loop {
        let prev = clean_str.clone();
        for s_suffix in [
            " الموسم الاول", " الموسم الأول", " الموسم الثاني", " الموسم الثالث", " الموسم الرابع", " الموسم الخامس",
            " الموسم السادس", " الموسم السابع", " الموسم الثامن", " الموسم التاسع", " الموسم العاشر",
            " الجزء الاول", " الجزء الأول", " الجزء الثاني", " الجزء الثالث", " الجزء الرابع", " الجزء الخامس",
            " الجزء السادس", " الجزء السابع", " الجزء الثامن", " الجزء التاسع", " الجزء العاشر",
            " الموسم", " الجزء", " موسم", " جزء",
        ] {
            if clean_str.ends_with(s_suffix) {
                clean_str = clean_str[..clean_str.len() - s_suffix.len()].trim().to_owned();
                break;
            }
        }
        for s_prefix in [" الموسم ", " موسم ", " الجزء ", " جزء ", " Season ", " season ", " S", " s"] {
            if let Some(pos) = clean_str.rfind(s_prefix) {
                let rest = &clean_str[pos + s_prefix.len()..];
                if rest.chars().all(|c| c.is_ascii_digit()) {
                    clean_str = clean_str[..pos].trim().to_owned();
                    break;
                }
            }
        }
        clean_str = clean_title(&clean_str);
        if clean_str == prev {
            break;
        }
    }
    clean_str.trim().to_owned()
}

pub fn build_series_identity(series: &Value, target_edition: &str) -> Value {
    let raw_title = field(series, "title");
    let clean = clean_series_title(&raw_title);
    let series_title = field(series, "seriesTitle");
    let clean_st = if !series_title.is_empty() {
        clean_series_title(&series_title)
    } else {
        clean.clone()
    };
    let mut aliases: Vec<String> = Vec::new();
    if let Some(arr) = series["aliases"].as_array() {
        for a in arr.iter().filter_map(Value::as_str) {
            let a_clean = clean_series_title(a);
            if !a_clean.is_empty() && !aliases.contains(&a_clean) {
                aliases.push(a_clean);
            }
        }
    }
    for key in ["ar_title", "en_title", "other_title"] {
        let val = field(series, key);
        if !val.is_empty() {
            let v_clean = clean_series_title(&val);
            if !v_clean.is_empty() && !aliases.contains(&v_clean) {
                aliases.push(v_clean);
            }
        }
    }
    let is_movie = kind(series) == "movie";
    json!({
        "title": clean,
        "seriesTitle": clean_st,
        "ar_title": field(series, "ar_title"),
        "en_title": field(series, "en_title"),
        "other_title": field(series, "other_title"),
        "aliases": aliases,
        "year": field(series, "year"),
        "season": season(series),
        "url": field(series, "url"),
        "seriesUrl": field(series, "seriesUrl"),
        "type": if is_movie { "movie" } else { "series" },
        "mediaType": if is_movie { "movie" } else { "series" },
        "edition": target_edition
    })
}

pub fn slug_from_url(url: &str) -> Option<String> {
    if !url.starts_with("http") {
        return None;
    }
    let trimmed = url.trim_end_matches('/');
    let last_seg = trimmed.rsplit('/').next()?;
    if last_seg.is_empty()
        || last_seg.contains('.')
        || last_seg.chars().all(|c| c.is_ascii_digit())
        || ["series", "episode", "movies", "film", "page"].contains(&last_seg)
    {
        return None;
    }
    let with_spaces = last_seg.replace('-', " ").replace('_', " ");
    Some(with_spaces)
}

fn titles(value: &Value) -> Vec<String> {
    let mut titles = vec![normalized(&field(value, "title"))];
    let clean = clean_title(&field(value, "title"));
    let norm_clean = normalized(&clean);
    if !norm_clean.is_empty() {
        titles.push(norm_clean);
    }
    let series_clean = clean_series_title(&field(value, "title"));
    let norm_series = normalized(&series_clean);
    if !norm_series.is_empty() {
        titles.push(norm_series);
    }
    let s_title = field(value, "seriesTitle");
    if !s_title.is_empty() {
        let norm_s = normalized(&s_title);
        if !norm_s.is_empty() {
            titles.push(norm_s);
        }
        let clean_s = clean_series_title(&s_title);
        let norm_clean_s = normalized(&clean_s);
        if !norm_clean_s.is_empty() {
            titles.push(norm_clean_s);
        }
    }
    for key in ["en_title", "ar_title", "other_title"] {
        let val = field(value, key);
        if !val.is_empty() {
            let norm = normalized(&val);
            if !norm.is_empty() {
                titles.push(norm);
            }
            let clean = clean_series_title(&val);
            let norm_clean = normalized(&clean);
            if !norm_clean.is_empty() {
                titles.push(norm_clean);
            }
        }
    }
    let url_str = field(value, "url");
    if let Some(slug) = slug_from_url(&url_str) {
        let norm_slug = normalized(&slug);
        if !norm_slug.is_empty() {
            titles.push(norm_slug);
        }
    }
    if let Some(aliases) = value["aliases"].as_array() {
        titles.extend(aliases.iter().filter_map(Value::as_str).map(normalized));
    }
    if let Some(offers) = value["offers"].as_array() {
        for offer in offers {
            titles.push(normalized(&field(offer, "title")));
            let clean_offer = clean_title(&field(offer, "title"));
            let norm_offer = normalized(&clean_offer);
            if !norm_offer.is_empty() {
                titles.push(norm_offer);
            }
            for key in ["en_title", "ar_title", "other_title"] {
                let val = field(offer, key);
                if !val.is_empty() {
                    let norm = normalized(&val);
                    if !norm.is_empty() {
                        titles.push(norm);
                    }
                }
            }
            let offer_url = field(offer, "url");
            if let Some(slug) = slug_from_url(&offer_url) {
                let norm_slug = normalized(&slug);
                if !norm_slug.is_empty() {
                    titles.push(norm_slug);
                }
            }
            if let Some(aliases) = offer["aliases"].as_array() {
                titles.extend(aliases.iter().filter_map(Value::as_str).map(normalized));
            }
        }
    }
    let mut seen = HashSet::new();
    titles.retain(|title| !title.is_empty() && title != "بدون عنوان" && seen.insert(title.clone()));
    titles
}

fn year(value: &str) -> String {
    let value = number(value);
    if value == "0" {
        String::new()
    } else {
        value
    }
}
fn compatible_year(a: &str, b: &str) -> bool {
    let a = year(a);
    let b = year(b);
    a.is_empty() || b.is_empty() || a == b
}

struct Group {
    kind: &'static str,
    edition: &'static str,
    year: String,
    episode: String,
    season: String,
    offers: Vec<Value>,
    urls: HashSet<String>,
}

fn identity(kind: &str, edition: &str, title: &str, episode: &str, season: &str) -> String {
    if kind == "episode" {
        format!("{kind}\0{edition}\0{title}\0{episode}\0{season}")
    } else if kind == "series" {
        format!("{kind}\0{title}")
    } else {
        format!("{kind}\0{edition}\0{title}")
    }
}

/// Indexed lookup keeps loading later catalogue pages linear in the number of
/// cards; aliases remain provider offers rather than separate duplicate tiles.
pub fn merge(items: &[Value], preferred: &str) -> Vec<Value> {
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    let mut urls: HashMap<String, usize> = HashMap::new();
    for item in items {
        let url = field(item, "url");
        if url.is_empty() {
            continue;
        }
        let kind = kind(item);
        let edition = edition(item);
        let titles = titles(item);
        let year = year(&field(item, "year"));
        let episode = episode_number(item);
        let season = season(item);
        let keys: Vec<String> = titles
            .iter()
            .map(|title| identity(kind, edition, title, &episode, &season))
            .collect();
        let mut candidates: Vec<usize> = urls
            .get(&url)
            .copied()
            .into_iter()
            .chain(
                keys.iter()
                    .flat_map(|key| index.get(key).into_iter().flatten().copied()),
            )
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        let found = candidates.into_iter().find(|&i| {
            let group = &groups[i];
            group.kind == kind
                && (kind == "series" || group.edition == edition)
                && (group.urls.contains(&url) || compatible_year(&group.year, &year))
                && (kind != "episode" || (group.episode == episode && group.season == season))
        });
        let at = found.unwrap_or_else(|| {
            let at = groups.len();
            groups.push(Group {
                kind,
                edition,
                year: year.clone(),
                episode: episode.clone(),
                season: season.clone(),
                offers: Vec::new(),
                urls: HashSet::new(),
            });
            at
        });
        let group = &mut groups[at];
        if group.year.is_empty() && !year.is_empty() {
            group.year = year;
        }
        let offers = item["offers"]
            .as_array()
            .filter(|offers| !offers.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![item.clone()]);
        for mut offer in offers {
            let offer_url = field(&offer, "url");
            if offer_url.is_empty() || !group.urls.insert(offer_url.clone()) {
                continue;
            }
            offer["source"] = json!(provider(&offer));
            offer["edition"] = json!(self::edition(&offer));
            urls.insert(offer_url, at);
            group.offers.push(offer);
        }
        for key in keys {
            let entries = index.entry(key).or_default();
            if !entries.contains(&at) {
                entries.push(at);
            }
        }
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let mut primary = group
                .offers
                .iter()
                .find(|offer| provider(offer) == preferred)
                .or_else(|| group.offers.first())?
                .clone();
            for key in ["year", "rating", "thumbnail", "artwork", "poster", "story"] {
                if field(&primary, key).is_empty() {
                    if let Some(value) = group
                        .offers
                        .iter()
                        .map(|offer| &offer[key])
                        .find(|value| !value.is_null() && value.as_str() != Some(""))
                    {
                        primary[key] = value.clone();
                    }
                }
            }
            let mut merged_aliases: Vec<String> = Vec::new();
            for offer in &group.offers {
                for key in ["title", "ar_title", "en_title"] {
                    let v = field(offer, key);
                    if !v.is_empty() && !merged_aliases.contains(&v) {
                        merged_aliases.push(v);
                    }
                }
                if let Some(arr) = offer["aliases"].as_array() {
                    for alias in arr.iter().filter_map(Value::as_str).map(str::trim) {
                        if !alias.is_empty() && !merged_aliases.iter().any(|e| e == alias) {
                            merged_aliases.push(alias.to_owned());
                        }
                    }
                }
            }
            if !merged_aliases.is_empty() {
                primary["aliases"] = json!(merged_aliases);
            }
            let mut sources: Vec<&str> = Vec::new();
            let mut editions: Vec<&str> = Vec::new();
            for offer in &group.offers {
                let source = provider(offer);
                if !sources.contains(&source) {
                    sources.push(source);
                }
                let ed = edition(offer);
                if !editions.contains(&ed) {
                    editions.push(ed);
                }
            }
            primary["sources"] = json!(sources);
            primary["editions"] = json!(editions);
            let has_both = editions.contains(&"original") && editions.contains(&"dubbed");
            primary["has_both_editions"] = json!(has_both);
            if has_both {
                primary["edition"] = json!("both");
            } else {
                primary["edition"] = json!(group.edition);
            }
            if group.kind == "series" {
                let cleaned = clean_series_title(&field(&primary, "title"));
                if !cleaned.is_empty() {
                    primary["title"] = json!(cleaned);
                }
            }
            primary["offers"] = json!(group.offers);
            Some(primary)
        })
        .collect()
}

pub fn match_episode(current: &Value, episodes: &[Value]) -> Option<Value> {
    let number = episode_number(current);
    if number.is_empty() {
        return None;
    }
    let mut found = episodes.iter().filter(|episode| {
        episode_number(episode) == number
            && season(current) == season(episode)
            && edition(current) == edition(episode)
    });
    let first = found.next()?;
    if found.next().is_some() {
        None
    } else {
        Some(first.clone())
    }
}

pub fn match_episode_cross_season(
    current: &Value,
    current_all: &[Value],
    target_all: &[Value],
) -> Option<Value> {
    if let Some(direct) = match_episode(current, target_all) {
        return Some(direct);
    }
    let cur_num = episode_number(current);
    let cur_season = season(current);
    let cur_num_int = cur_num.parse::<usize>().ok()?;
    let cur_season_int = cur_season.parse::<usize>().unwrap_or(1);

    // Calculate absolute 1-based chronological index K of current episode
    let abs_k = if cur_season_int > 1 {
        let prior_count = current_all
            .iter()
            .filter(|e| season(e).parse::<usize>().unwrap_or(1) < cur_season_int)
            .count();
        if prior_count > 0 {
            prior_count + cur_num_int
        } else {
            current_all
                .iter()
                .position(|e| field(e, "url") == field(current, "url"))
                .map(|p| p + 1)
                .unwrap_or(cur_num_int)
        }
    } else {
        current_all
            .iter()
            .position(|e| field(e, "url") == field(current, "url"))
            .map(|p| p + 1)
            .unwrap_or(cur_num_int)
    };

    // Group target seasons
    let mut target_seasons: Vec<usize> = target_all
        .iter()
        .map(|e| season(e).parse::<usize>().unwrap_or(1))
        .collect();
    target_seasons.sort_unstable();
    target_seasons.dedup();

    if target_seasons.len() > 1 {
        // Target has multiple seasons (e.g. Cinemana)
        let mut rem = abs_k;
        for s_num in target_seasons {
            let season_eps: Vec<&Value> = target_all
                .iter()
                .filter(|e| season(e).parse::<usize>().unwrap_or(1) == s_num)
                .collect();
            let count = season_eps.len();
            if rem <= count {
                let ep_in_season = rem;
                if let Some(target) = season_eps
                    .iter()
                    .find(|e| episode_number(e) == ep_in_season.to_string())
                {
                    return Some((*target).clone());
                }
                if ep_in_season > 0 && ep_in_season <= count {
                    return Some((*season_eps[ep_in_season - 1]).clone());
                }
                break;
            } else {
                rem -= count;
            }
        }
    } else {
        // Target is flat-numbered (e.g. Krmzi)
        if let Some(target) = target_all.iter().find(|e| {
            episode_number(e) == abs_k.to_string()
        }) {
            return Some((*target).clone());
        }
    }

    // Direct match fallback (same season and episode number)
    if let Some(direct) = target_all
        .iter()
        .find(|e| episode_number(e) == cur_num && season(e) == cur_season)
    {
        return Some(direct.clone());
    }

    // Fallback: match by 0-based position in target_all
    if abs_k > 0 && abs_k <= target_all.len() {
        return Some(target_all[abs_k - 1].clone());
    }

    None
}

pub fn find_counterpart(item: &Value, candidates: &[Value]) -> Option<Value> {
    find_counterpart_with_edition(item, candidates, None)
}

pub fn find_counterpart_with_edition(
    item: &Value,
    candidates: &[Value],
    target_edition: Option<&str>,
) -> Option<Value> {
    let item_titles = titles(item);
    if item_titles.is_empty() {
        return None;
    }
    let is_movie = kind(item) == "movie";
    if !is_movie && kind(item) != "series" && field(item, "seriesUrl").is_empty() {
        return None;
    }
    let desired_edition = target_edition.unwrap_or_else(|| edition(item));
    let item_year = field(item, "year");
    let item_season = season(item);

    let candidates_merged = merge(candidates, "cinemana");
    let edition_matched: Vec<Value> = candidates_merged
        .into_iter()
        .filter(|candidate| {
            let ed_matches = edition(candidate) == desired_edition
                || candidate["editions"].as_array().map_or(false, |eds| {
                    eds.iter().any(|e| e.as_str() == Some(desired_edition))
                })
                || candidate["offers"].as_array().map_or(false, |offers| {
                    offers.iter().any(|o| edition(o) == desired_edition)
                });
            if !ed_matches {
                return false;
            }
            if is_movie {
                kind(candidate) == "movie"
            } else {
                kind(candidate) == "series" || !field(candidate, "seriesUrl").is_empty()
            }
        })
        .collect();

    if edition_matched.is_empty() {
        return None;
    }

    let year_compatible: Vec<Value> = edition_matched
        .into_iter()
        .filter(|c| compatible_year(&item_year, &field(c, "year")))
        .collect();

    if year_compatible.is_empty() {
        return None;
    }

    let mut scored: Vec<(i32, Value)> = year_compatible
        .into_iter()
        .filter_map(|candidate| {
            let c_titles = titles(&candidate);
            let mut score = 0;

            let exact = c_titles.iter().any(|ct| item_titles.contains(ct));
            if exact {
                score += 100;
            } else {
                let max_overlap = c_titles.iter().map(|ct| {
                    let c_words: HashSet<&str> = ct.split_whitespace().collect();
                    item_titles.iter().map(|it| {
                        let it_words: HashSet<&str> = it.split_whitespace().collect();
                        let common = c_words.intersection(&it_words).count();
                        if it_words.is_empty() { 0 } else { (common * 60) / it_words.len() }
                    }).max().unwrap_or(0)
                }).max().unwrap_or(0);
                if max_overlap >= 30 {
                    score += max_overlap as i32;
                } else {
                    let sub = c_titles.iter().any(|ct| {
                        item_titles.iter().any(|it| {
                            (it.len() >= 4 && ct.contains(it.as_str()))
                                || (ct.len() >= 4 && it.contains(ct.as_str()))
                        })
                    });
                    if sub {
                        score += 35;
                    }
                }
            }

            if score < 35 {
                return None;
            }

            let c_year = field(&candidate, "year");
            if !item_year.is_empty() && !c_year.is_empty() {
                if item_year == c_year {
                    score += 25;
                } else if compatible_year(&item_year, &c_year) {
                    score += 10;
                }
            }

            if !is_movie {
                let c_season = season(&candidate);
                if !item_season.is_empty() && !c_season.is_empty() {
                    if item_season == c_season {
                        score += 35;
                    } else {
                        score -= 25;
                    }
                }
            }

            Some((score, candidate))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    if scored.len() >= 2 && item_year.is_empty() {
        if scored[0].0 == scored[1].0 {
            let y0 = field(&scored[0].1, "year");
            let y1 = field(&scored[1].1, "year");
            if !y0.is_empty() && !y1.is_empty() && y0 != y1 {
                return None;
            }
        }
    }

    let target = scored.into_iter().next().map(|(_, cand)| cand);

    if let Some(target) = target {
        if let Some(offers) = target["offers"].as_array() {
            if let Some(offer) = offers.iter().find(|o| edition(o) == desired_edition) {
                let mut resolved = offer.clone();
                if field(&resolved, "year").is_empty() && !field(&target, "year").is_empty() {
                    resolved["year"] = target["year"].clone();
                }
                if resolved["offers"].is_null() {
                    resolved["offers"] = target["offers"].clone();
                }
                return Some(resolved);
            }
        }
        Some(target)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dubbed_and_subbed_editions_merge_into_unified_series_card() {
        let original = json!({"url":"https://krmzi.org/series/sea/","title":"مسلسل هذا البحر سوف يفيض","type":"series","year":2025});
        let dubbed = json!({"url":"cinemana:3129341","title":"هذا البحر سوف يفيض","aliases":["Tasacak Bu Deniz Dubbed"],"type":"series","year":2025});
        let alternate = json!({"url":"cinemana:1320095","title":"هذا البحر سوف يفيض","aliases":["Tasacak Bu Deniz"],"type":"series","year":2025});
        let merged = merge(
            &[original.clone(), dubbed.clone(), alternate.clone()],
            "cinemana",
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["has_both_editions"], true);
        assert_eq!(merged[0]["offers"].as_array().unwrap().len(), 3);
        assert_eq!(merged[0]["title"], "هذا البحر سوف يفيض");
        assert_eq!(
            find_counterpart(&original, &[dubbed.clone(), alternate]).unwrap()["url"],
            "cinemana:1320095"
        );
        assert!(find_counterpart(&original, &[dubbed.clone()]).is_none());
        assert_eq!(
            find_counterpart_with_edition(&original, &[dubbed], Some("dubbed")).unwrap()["url"],
            "cinemana:3129341"
        );
        assert_eq!(clean_series_title("مسلسل تحت الارض الحلقة 16"), "تحت الارض");
        assert_eq!(clean_series_title("مسلسل هذا البحر سوف يفيض مترجم الحلقة 5"), "هذا البحر سوف يفيض");
        assert!(match_episode(
            &json!({"episodeNum":2,"edition":"original"}),
            &[json!({"episodeNum":2,"aliases":["Series Dubbed"]})]
        )
        .is_none());
        assert_eq!(edition(&json!({"title":"مسلسل التجربة مدبلج"})), "dubbed");
        assert_eq!(edition(&json!({"en_title":"Show Undubbed"})), "original");
    }
    #[test]
    fn towards_zero_and_complex_edition_counterpart_matching() {
        let towards_zero_subbed = json!({
            "url": "https://krmzi.org/series/towards-zero",
            "title": "Towards Zero",
            "ar_title": "نحو الصفر",
            "type": "series",
            "year": 2025
        });
        let towards_zero_dubbed = json!({
            "url": "cinemana:999888",
            "title": "Towards Zero (مدبلج)",
            "ar_title": "نحو الصفر (مدبلج)",
            "type": "series",
            "year": 2025
        });
        let identity = build_series_identity(&towards_zero_subbed, "dubbed");
        let matched = find_counterpart_with_edition(&identity, &[towards_zero_dubbed], Some("dubbed"));
        assert!(matched.is_some());
        assert_eq!(matched.unwrap()["url"], "cinemana:999888");

        assert_eq!(
            clean_series_title("مسلسل هذا البحر سوف يفيض مدبلج الموسم الاول الحلقة 5"),
            "هذا البحر سوف يفيض"
        );
        assert_eq!(clean_series_title("Towards Zero (Dubbed)"), "Towards Zero");
        assert_eq!(clean_series_title("Towards Zero [مدبلج]"), "Towards Zero");
        assert_eq!(season_from_text("مسلسل الحفرة الموسم الثاني"), Some("2".into()));
        assert_eq!(season_from_text("مسلسل كذا الجزء 4"), Some("4".into()));
    }
    #[test]
    fn aliases_and_arabic_identity_keep_both_sources() {
        let result = merge(
            &[
                json!({"url":"https://krmzi.org/series/a","title":"مسلسل ورود و ذنوب مترجم - قرمزي","type":"series"}),
                json!({"url":"cinemana:12","title":"ورود وذنوب","mediaType":"series","year":2025}),
            ],
            "cinemana",
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["offers"].as_array().unwrap().len(), 2);
        assert_eq!(result[0]["url"], "cinemana:12");
    }
    #[test]
    fn unknown_year_does_not_bridge_known_remakes() {
        let result = merge(
            &[
                json!({"url":"a","title":"The Return","type":"series"}),
                json!({"url":"b","title":"The Return","type":"series","year":2010}),
                json!({"url":"c","title":"The Return","type":"series","year":2025}),
            ],
            "cinemana",
        );
        assert_eq!(result.len(), 2);
    }
    #[test]
    fn counterpart_rejects_movie_namesakes_and_ambiguous_remakes() {
        let item = json!({"title":"مسلسل التجربة","type":"series","aliases":["The Experiment"]});
        assert!(find_counterpart(
            &item,
            &[json!({"url":"cinemana:1","title":"The Experiment","mediaType":"movie"})]
        )
        .is_none());
        assert!(find_counterpart(
            &item,
            &[
                json!({"url":"a","title":"The Experiment","type":"series","year":2025}),
                json!({"url":"b","title":"The Experiment","type":"series","year":1990})
            ]
        )
        .is_none());
        assert_eq!(
            find_counterpart(
                &item,
                &[json!({"url":"cinemana:1","title":"The Experiment","mediaType":"series"})]
            )
            .unwrap()["url"],
            "cinemana:1"
        );
    }
    #[test]
    fn episode_matching_normalizes_digits_without_guessing() {
        let current = json!({"episodeNum":"٠٢","season":"١"});
        assert_eq!(
            match_episode(
                &current,
                &[json!({"url":"yes","episodeNum":"2","season":"01"})]
            )
            .unwrap()["url"],
            "yes"
        );
        assert!(match_episode(
            &current,
            &[
                json!({"url":"a","episodeNum":2}),
                json!({"url":"b","episodeNum":2})
            ]
        )
        .is_none());
        assert!(
            match_episode(&json!({"episodeNum":"2.5"}), &[json!({"episodeNum":"2"})]).is_none()
        );
        assert_eq!(episode_number(&json!({"title":"الحلقة ۰۲"})), "2");
    }
    #[test]
    fn untitled_cards_and_same_title_different_episodes_remain_separate() {
        assert_eq!(
            merge(
                &[
                    json!({"url":"a","type":"series"}),
                    json!({"url":"b","type":"series"})
                ],
                "cinemana"
            )
            .len(),
            2
        );
        assert_eq!(
            merge(
                &[
                    json!({"url":"a","title":"التجربة الحلقة 1","type":"episode"}),
                    json!({"url":"b","title":"التجربة الحلقة 2","type":"episode"})
                ],
                "cinemana"
            )
            .len(),
            2
        );
    }
    #[test]
    fn placeholders_zero_years_and_updated_metadata_do_not_hide_titles() {
        assert_eq!(
            merge(
                &[
                    json!({"url":"a","title":"بدون عنوان","type":"series"}),
                    json!({"url":"b","title":"بدون عنوان","type":"series"})
                ],
                "cinemana"
            )
            .len(),
            2
        );
        let records = [
            json!({"url":"a","title":"The Return","type":"series","year":0}),
            json!({"url":"b","title":"The Return","type":"series","year":2025}),
            json!({"url":"c","title":"The Return","type":"series","year":1990}),
        ];
        assert_eq!(merge(&records, "cinemana").len(), 2);
        assert_eq!(
            merge(
                &[
                    json!({"url":"same","title":"Name","type":"series","year":2025}),
                    json!({"url":"same","title":"Name","type":"series","year":2026})
                ],
                "cinemana"
            )
            .len(),
            1
        );
    }

    #[test]
    fn cross_season_episode_mapping() {
        let krmzi_episodes: Vec<Value> = (1..=20)
            .map(|i| json!({"url": format!("krmzi:{}", i), "episodeNum": i.to_string(), "season": "1"}))
            .collect();
        let mut cinemana_episodes: Vec<Value> = (1..=19)
            .map(|i| json!({"url": format!("cinemana:1:{}", i), "episodeNum": i.to_string(), "season": "1"}))
            .collect();
        cinemana_episodes.push(json!({"url": "cinemana:2:1", "episodeNum": "1", "season": "2"}));

        // 1. Krmzi Ep 20 -> Cinemana Season 2 Ep 1
        let krmzi_20 = &krmzi_episodes[19];
        let matched = match_episode_cross_season(krmzi_20, &krmzi_episodes, &cinemana_episodes).unwrap();
        assert_eq!(matched["url"], "cinemana:2:1");
        assert_eq!(matched["season"], "2");
        assert_eq!(matched["episodeNum"], "1");

        // 2. Cinemana Season 2 Ep 1 -> Krmzi Ep 20
        let cinemana_s2e1 = &cinemana_episodes[19];
        let matched_krmzi = match_episode_cross_season(cinemana_s2e1, &cinemana_episodes, &krmzi_episodes).unwrap();
        assert_eq!(matched_krmzi["url"], "krmzi:20");
        assert_eq!(matched_krmzi["episodeNum"], "20");

        // 3. Normal match within Season 1
        let krmzi_15 = &krmzi_episodes[14];
        let matched_15 = match_episode_cross_season(krmzi_15, &krmzi_episodes, &cinemana_episodes).unwrap();
        assert_eq!(matched_15["url"], "cinemana:1:15");
    }

    #[test]
    fn turkish_and_english_cross_source_matching() {
        // Case 1: Cinemana has Turkish title with diacritics: "Taşacak Bu Deniz" & Arabic: "هذا البحر سوف يفيض"
        let cinemana = json!({
            "url": "cinemana:100",
            "ar_title": "هذا البحر سوف يفيض",
            "en_title": "Taşacak Bu Deniz",
            "mediaType": "series",
            "year": 2025
        });
        // Krmzi has Arabic title and Turkish slug in URL
        let krmzi = json!({
            "url": "https://krmzi.org/series/tasacak-bu-deniz/",
            "title": "مسلسل هذا البحر سوف يفيض مترجم",
            "type": "series",
            "year": 2025
        });
        let merged = merge(&[cinemana, krmzi], "cinemana");
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["offers"].as_array().unwrap().len(), 2);

        // Case 2: One source has ONLY Turkish name with special chars: "Çukur", another has URL slug "cukur" and Arabic "الحفرة"
        let cukur_cinemana = json!({
            "url": "cinemana:200",
            "en_title": "Çukur",
            "mediaType": "series"
        });
        let cukur_krmzi = json!({
            "url": "https://krmzi.org/series/cukur/",
            "title": "مسلسل الحفرة مترجم",
            "type": "series"
        });
        let merged_cukur = merge(&[cukur_cinemana, cukur_krmzi], "cinemana");
        assert_eq!(merged_cukur.len(), 1);
        assert_eq!(merged_cukur[0]["offers"].as_array().unwrap().len(), 2);
    }
}
