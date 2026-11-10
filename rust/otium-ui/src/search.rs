//! Smart query normalization, multi-variant expansion, instant local matching,
//! and multi-signal relevance ranking shared across catalogue search, discovery,
//! and episode filtering.

use crate::catalogue;
use serde_json::Value;
use std::collections::HashSet;

pub const STRONG_MATCH_THRESHOLD: i64 = 6_000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub raw: String,
    pub core: String,
    pub intent: Option<&'static str>,
    pub year_hint: Option<String>,
    pub clean_without_year: String,
}

/// Normalizes Arabic and Latin text for tolerant matching:
/// folds hamza forms, ta marbuta, alif maqsura, Arabic-Indic numerals,
/// punctuation, diacritics, and case.
pub fn normalize_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        let mapped = match ch {
            // Strip Arabic diacritics & tatweel
            '\u{0640}' | '\u{064B}'..='\u{065F}' | '\u{0670}' => continue,
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ى' => 'ي',
            'ة' => 'ه',
            'ؤ' => 'و',
            'ئ' => 'ي',
            '٠' | '۰' => '0',
            '١' | '۱' => '1',
            '٢' | '۲' => '2',
            '٣' | '۳' => '3',
            '٤' | '۴' => '4',
            '٥' | '۵' => '5',
            '٦' | '۶' => '6',
            '٧' | '۷' => '7',
            '٨' | '۸' => '8',
            '٩' | '۹' => '9',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if mapped == ' ' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(mapped);
            prev_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

pub fn compact_text(input: &str) -> String {
    normalize_text(input)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

pub fn tokenize(input: &str) -> Vec<String> {
    normalize_text(input)
        .split_whitespace()
        .map(canonical_token)
        .filter(|t| !t.is_empty())
        .collect()
}

fn canonical_token(token: &str) -> String {
    match token {
        "ii" => "2".into(),
        "iii" => "3".into(),
        "iv" => "4".into(),
        "vi" => "6".into(),
        "vii" => "7".into(),
        "viii" => "8".into(),
        "ix" => "9".into(),
        "&" => "and".into(),
        _ => token.to_owned(),
    }
}

fn is_year_token(token: &str) -> bool {
    token.len() == 4
        && token.chars().all(|c| c.is_ascii_digit())
        && (1900..=2035).contains(&token.parse::<u32>().unwrap_or(0))
}

fn filler_intent(token_norm: &str) -> Option<&'static str> {
    match token_norm {
        "فيلم" | "فلم" | "movie" | "film" | "cinema" => Some("movie"),
        "مسلسل" | "انمي" | "مسلسلات" | "series" | "show" | "tv" | "anime" => Some("series"),
        _ => None,
    }
}

/// Extracts intent (`فيلم` / `مسلسل`), optional trailing year hint, and clean search core.
pub fn parse_query(raw: &str) -> ParsedQuery {
    let raw_trimmed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if raw_trimmed.is_empty() {
        return ParsedQuery::default();
    }
    let orig_words: Vec<&str> = raw_trimmed.split_whitespace().collect();
    let mut intent: Option<&'static str> = None;
    let mut kept: Vec<&str> = Vec::with_capacity(orig_words.len());

    // Strip filler words only from leading/trailing positions unless other tokens remain.
    let mut start = 0;
    let mut end = orig_words.len();
    while start < end {
        if let Some(kind) = filler_intent(&normalize_text(orig_words[start])) {
            intent.get_or_insert(kind);
            start += 1;
        } else {
            break;
        }
    }
    while end > start {
        if let Some(kind) = filler_intent(&normalize_text(orig_words[end - 1])) {
            intent.get_or_insert(kind);
            end -= 1;
        } else {
            break;
        }
    }
    if start < end {
        kept.extend_from_slice(&orig_words[start..end]);
    } else {
        kept.extend_from_slice(&orig_words);
    }

    let core = kept.join(" ");
    let mut year_hint = None;
    let mut without_year_words = kept.clone();
    if without_year_words.len() >= 2 {
        let last_norm = normalize_text(without_year_words.last().copied().unwrap_or(""));
        if is_year_token(&last_norm) {
            year_hint = Some(last_norm);
            without_year_words.pop();
        }
    }
    let clean_without_year = without_year_words.join(" ");
    ParsedQuery {
        raw: raw_trimmed,
        core,
        intent,
        year_hint,
        clean_without_year,
    }
}

/// Curated bilingual aliases for high-frequency queries where Cinemana only indexes
/// one spelling/script (verified against live Cinemana responses).
const PHRASE_ALIASES: &[(&str, &[&str])] = &[
    ("اوبنهايمر", &["Oppenheimer"]),
    ("اوبنهايمير", &["Oppenheimer"]),
    ("انسبشن", &["Inception"]),
    ("البدايه", &["Inception"]),
    ("انترستيلار", &["Interstellar"]),
    ("بين النجوم", &["Interstellar"]),
    ("تايتنك", &["Titanic", "تيتانيك"]),
    ("تيتانيك", &["Titanic"]),
    ("سبايدرمان", &["Spider-Man", "سبايدر مان"]),
    ("سبايدر مان", &["Spider-Man", "سبايدرمان"]),
    ("spiderman", &["Spider-Man", "Spider Man"]),
    ("باتمان", &["Batman", "The Batman"]),
    ("ذا باتمان", &["The Batman", "Batman"]),
    ("دارك نايت", &["The Dark Knight", "Dark Knight"]),
    ("فارس الظلام", &["The Dark Knight"]),
    ("بريكنج باد", &["Breaking Bad"]),
    ("بريكينج باد", &["Breaking Bad"]),
    ("بيكي بلايندرز", &["Peaky Blinders"]),
    ("جيم اوف ثرونز", &["Game of Thrones"]),
    ("صراع العروش", &["Game of Thrones"]),
    ("سكويد جيم", &["Squid Game"]),
    ("لعبه الحبار", &["Squid Game"]),
    ("ديدبول", &["Deadpool"]),
    ("ولفرين", &["Wolverine"]),
    ("فولفرين", &["Wolverine"]),
    ("جلادياتور", &["Gladiator"]),
    ("افنجرز", &["Avengers"]),
    ("المنتقمون", &["Avengers"]),
    ("هاري بوتر", &["Harry Potter"]),
    ("سيد الخواتم", &["The Lord of the Rings"]),
    ("ذا لاست اوف اس", &["The Last of Us"]),
    ("ذا بويز", &["The Boys"]),
    ("سترينجر ثينقز", &["Stranger Things"]),
    ("اشياء غريبه", &["Stranger Things"]),
    ("لا كاسا دي بابيل", &["La Casa de Papel", "Money Heist"]),
    ("البروفيسور", &["La Casa de Papel"]),
    ("العراب", &["The Godfather"]),
    ("جودفاذر", &["The Godfather"]),
    ("جون ويك", &["John Wick"]),
    ("جوكر", &["Joker"]),
    ("الجوكر", &["Joker"]),
    ("ماتريكس", &["The Matrix"]),
    ("ديون", &["Dune"]),
    ("باربي", &["Barbie"]),
    ("افاتار", &["Avatar"]),
    ("تشيرنوبل", &["Chernobyl"]),
    ("ديكستر", &["Dexter"]),
    ("فايكنجز", &["Vikings"]),
    ("فيكنجز", &["Vikings"]),
    ("شيرلوك", &["Sherlock"]),
    ("فريندز", &["Friends"]),
    ("بريزون بريك", &["Prison Break"]),
    ("الهروب من شاوشانك", &["The Shawshank Redemption"]),
    ("شاوشانك", &["The Shawshank Redemption"]),
    ("نادي القتال", &["Fight Club"]),
    ("فايت كلوب", &["Fight Club"]),
    ("فينوم", &["Venom"]),
    ("فاست اند فيوريوس", &["Fast & Furious"]),
    ("السرعه والغضب", &["Fast & Furious"]),
    ("توب جان", &["Top Gun"]),
    ("توب غان", &["Top Gun"]),
    ("ذا ويتشر", &["The Witcher"]),
    ("ويتشر", &["The Witcher"]),
    ("ذا ووكينج ديد", &["The Walking Dead"]),
    ("الموتي السائرون", &["The Walking Dead"]),
    ("باراسايت", &["Parasite"]),
    ("طفيلي", &["Parasite"]),
    ("شاتر ايلاند", &["Shutter Island"]),
    ("لا لا لاند", &["La La Land"]),
    ("هجوم العمالقه", &["Attack on Titan"]),
    ("ون بيس", &["One Piece"]),
    ("ناروتو", &["Naruto"]),
    ("قاتل الشياطين", &["Demon Slayer"]),
    ("ديمون سلاير", &["Demon Slayer"]),
    ("جوجوتسو كايسن", &["Jujutsu Kaisen"]),
    ("مذكره الموت", &["Death Note"]),
    ("دث نوت", &["Death Note"]),
    ("القناص", &["Hunter x Hunter"]),
    ("اكس مان", &["X-Men"]),
    ("xmen", &["X-Men"]),
    ("antman", &["Ant-Man"]),
    ("ironman", &["Iron Man"]),
    ("المهمه المستحيله", &["Mission: Impossible"]),
    ("سوبرمان", &["Superman"]),
    ("ووندر وومان", &["Wonder Woman"]),
    ("اكوامان", &["Aquaman"]),
    ("كيرة والجن", &["كيرة والجن", "Kira & El Gin"]),
    ("كيرهه والجن", &["كيرة والجن"]),
    ("اولاد رزق", &["ولاد رزق"]),
    ("أولاد رزق", &["ولاد رزق"]),
    ("العارف", &["العارف"]),
    ("هيبتا", &["هيبتا"]),
];

const WORD_ALIASES: &[(&str, &str)] = &[
    ("ذا", "The"),
    ("سبايدر", "Spider"),
    ("بات", "Bat"),
    ("سوبر", "Super"),
    ("دارك", "Dark"),
    ("نايت", "Knight"),
    ("مان", "Man"),
    ("ستار", "Star"),
    ("وورز", "Wars"),
    ("وور", "War"),
    ("كينج", "King"),
    ("لورد", "Lord"),
    ("جيم", "Game"),
    ("بلاك", "Black"),
    ("ريد", "Red"),
    ("وايت", "White"),
    ("ديد", "Dead"),
    ("لوست", "Lost"),
    ("بويز", "Boys"),
    ("جيرل", "Girl"),
    ("هاوس", "House"),
    ("هوس", "House"),
    ("لايت", "Light"),
    ("بلود", "Blood"),
    ("شادو", "Shadow"),
    ("سيتي", "City"),
    ("وورلد", "World"),
    ("توب", "Top"),
    ("جان", "Gun"),
    ("جون", "John"),
    ("ويك", "Wick"),
    ("مستر", "Mr"),
    ("دكتور", "Doctor"),
    ("سترينج", "Strange"),
    ("فاست", "Fast"),
    ("تايم", "Time"),
    ("لايف", "Life"),
    ("هيرو", "Hero"),
    ("تيم", "Team"),
    ("فاميلي", "Family"),
    ("ماكس", "Max"),
    ("باد", "Bad"),
    ("بريك", "Break"),
    ("ايلاند", "Island"),
];

const HAMZA_WORDS: &[(&str, &str)] = &[
    ("ابراهيم", "إبراهيم"),
    ("ابيض", "أبيض"),
    ("الابيض", "الأبيض"),
    ("احمر", "أحمر"),
    ("الاحمر", "الأحمر"),
    ("اسود", "أسود"),
    ("الاسود", "الأسود"),
    ("ازرق", "أزرق"),
    ("الازرق", "الأزرق"),
    ("اخضر", "أخضر"),
    ("الاخضر", "الأخضر"),
    ("اصفر", "أصفر"),
    ("اولاد", "أولاد"),
    ("ولاد", "أولاد"),
    ("اصحاب", "أصحاب"),
    ("اخر", "آخر"),
    ("ايام", "أيام"),
    ("الايام", "الأيام"),
    ("ارض", "أرض"),
    ("الارض", "الأرض"),
    ("اهل", "أهل"),
    ("امير", "أمير"),
    ("الامير", "الأمير"),
    ("اميره", "أميرة"),
    ("اسد", "أسد"),
    ("الوان", "ألوان"),
    ("الالوان", "الألوان"),
    ("احمد", "أحمد"),
    ("اشباح", "أشباح"),
    ("اسرار", "أسرار"),
    ("ابناء", "أبناء"),
    ("انسان", "إنسان"),
    ("انس", "إنس"),
    ("الانس", "الإنس"),
    ("اسطوره", "أسطورة"),
    ("امبراطور", "إمبراطور"),
    ("اخوه", "إخوة"),
    ("ام", "أم"),
    ("اب", "أب"),
];

fn restore_arabic_spelling(text: &str) -> Option<String> {
    let mut changed = false;
    let mut out = Vec::new();
    for raw_word in text.split_whitespace() {
        let norm = normalize_text(raw_word);
        let mut mapped = HAMZA_WORDS
            .iter()
            .find(|(plain, _)| *plain == norm)
            .map(|(_, restored)| (*restored).to_owned())
            .unwrap_or_else(|| raw_word.to_owned());
        if mapped == raw_word && norm.starts_with("ا") && norm.chars().count() >= 4 {
            // Try common hamza-restored prefix if word starts with plain alif
            let rest: String = raw_word.chars().skip(1).collect();
            if raw_word.starts_with('ا') {
                mapped = format!("أ{rest}");
            }
        }
        // Flip trailing ه <-> ة because Cinemana indexes them literally
        if mapped.ends_with('ة') && mapped == raw_word {
            mapped.pop();
            mapped.push('ه');
        } else if mapped.ends_with('ه') && mapped == raw_word {
            mapped.pop();
            mapped.push('ة');
        }
        if mapped != raw_word {
            changed = true;
        }
        out.push(mapped);
    }
    changed.then(|| out.join(" "))
}

fn translate_word_by_word(core_norm: &str) -> Option<String> {
    let words: Vec<&str> = core_norm.split_whitespace().collect();
    if words.is_empty() || words.len() > 5 {
        return None;
    }
    let mut mapped = Vec::with_capacity(words.len());
    let mut hits = 0usize;
    for w in &words {
        if let Some((_, en)) = WORD_ALIASES.iter().find(|(ar, _)| ar == w) {
            mapped.push(*en);
            hits += 1;
        } else if w.chars().all(|c| c.is_ascii_alphanumeric()) {
            mapped.push(*w);
        } else {
            return None;
        }
    }
    (hits > 0).then(|| mapped.join(" "))
}

/// Generates smart fallback Cinemana queries when primary search yields no strong match.
pub fn fallback_variants(raw: &str, parsed: &ParsedQuery) -> Vec<String> {
    let mut variants = Vec::new();
    let mut seen = HashSet::new();
    let primary_norm = normalize_text(&parsed.core);
    seen.insert(parsed.core.to_lowercase());
    seen.insert(raw.trim().to_lowercase());

    let mut push_variant = |candidate: String| {
        let trimmed = candidate.trim().to_owned();
        if trimmed.chars().count() >= 2 && seen.insert(trimmed.to_lowercase()) {
            variants.push(trimmed);
        }
    };

    if !parsed.clean_without_year.is_empty() && parsed.clean_without_year != parsed.core {
        push_variant(parsed.clean_without_year.clone());
    }

    // Check phrase aliases on both normalized core and normalized clean_without_year
    for needle in [&primary_norm, &normalize_text(&parsed.clean_without_year)] {
        if needle.is_empty() {
            continue;
        }
        for (key, targets) in PHRASE_ALIASES {
            if *key == needle.as_str() {
                for target in *targets {
                    push_variant((*target).to_owned());
                }
            }
        }
    }

    if let Some(word_en) = translate_word_by_word(&primary_norm) {
        push_variant(word_en);
    }

    // Roman numeral <-> digit rewrite (`Gladiator 2` <-> `Gladiator II`)
    let tokens: Vec<&str> = parsed.core.split_whitespace().collect();
    if tokens.len() >= 2 {
        let swapped: Vec<String> = tokens
            .iter()
            .map(|t| match t.to_ascii_lowercase().as_str() {
                "2" => "II".into(),
                "3" => "III".into(),
                "4" => "IV".into(),
                "ii" => "2".into(),
                "iii" => "3".into(),
                "iv" => "4".into(),
                "&" => "and".into(),
                "and" if !tokens.iter().any(|w| w == &"&") => "&".into(),
                _ => (*t).to_owned(),
            })
            .collect();
        let joined = swapped.join(" ");
        if joined != parsed.core {
            push_variant(joined);
        }
    }

    // Arabic bare-alif / ta-marbuta flip OR full hamza strip
    let stripped_ar = normalize_text(&parsed.core);
    if stripped_ar != parsed.core && stripped_ar.chars().any(|c| !c.is_ascii()) {
        push_variant(stripped_ar);
    }
    if parsed.core.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)) {
        if let Some(restored) = restore_arabic_spelling(&parsed.core) {
            push_variant(restored);
        }
    }

    // Multi-word Arabic colloquial prefix (`أولاد` / `اولاد` -> `ولاد`)
    if primary_norm.starts_with("اولاد ") {
        push_variant(parsed.core.replacen("أولاد", "ولاد", 1).replacen("اولاد", "ولاد", 1));
    }

    // Trim long multi-word queries (`>= 4` words) down to first 2-3 distinctive words
    if tokens.len() >= 4 {
        push_variant(tokens[..2].join(" "));
    }

    variants.truncate(4);
    variants
}

fn damerau_within(a: &str, b: &str, max_dist: usize) -> Option<usize> {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let (m, n) = (av.len(), bv.len());
    if m.abs_diff(n) > max_dist {
        return None;
    }
    if m == 0 || n == 0 {
        return Some(m.max(n));
    }
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = usize::from(av[i - 1] != bv[j - 1]);
            let mut best = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && av[i - 1] == bv[j - 2] && av[i - 2] == bv[j - 1] {
                best = best.min(dp[i - 2][j - 2] + 1);
            }
            dp[i][j] = best;
        }
    }
    (dp[m][n] <= max_dist).then_some(dp[m][n])
}

fn contains_subsequence(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

fn score_single_title(cand_raw: &str, q_raw: &str) -> i64 {
    let cand_norm = normalize_text(cand_raw);
    let q_norm = normalize_text(q_raw);
    if cand_norm.is_empty() || q_norm.is_empty() {
        return 0;
    }

    if cand_norm == q_norm {
        return 10_000;
    }
    let cand_compact = compact_text(cand_raw);
    let q_compact = compact_text(q_raw);
    if !cand_compact.is_empty() && cand_compact == q_compact {
        return 9_200;
    }

    let cand_tokens = tokenize(cand_raw);
    let q_tokens = tokenize(q_raw);
    if cand_tokens.is_empty() || q_tokens.is_empty() {
        return 0;
    }
    if cand_tokens == q_tokens {
        return 8_800;
    }

    let cleaned_cand = normalize_text(&catalogue::clean_title(cand_raw));
    if !cleaned_cand.is_empty() && cleaned_cand == q_norm {
        return 8_500;
    }

    let extra_words = cand_tokens.len().saturating_sub(q_tokens.len()) as i64;
    if cand_tokens.starts_with(&q_tokens) {
        return (6_400 - (extra_words * 40).min(500)).max(5_200);
    }

    if contains_subsequence(&cand_tokens, &q_tokens) {
        return (4_800 - (extra_words * 35).min(450)).max(3_800);
    }

    let mut matched = 0usize;
    let mut fuzzy_bonus = 0i64;
    for qt in &q_tokens {
        if cand_tokens.iter().any(|ct| ct == qt) {
            matched += 1;
            fuzzy_bonus += 120;
        } else if qt.chars().count() >= 3
            && cand_tokens
                .iter()
                .any(|ct| ct.starts_with(qt.as_str()) || qt.starts_with(ct.as_str()))
        {
            matched += 1;
            fuzzy_bonus += 70;
        } else {
            let max_d = if qt.chars().count() >= 7 {
                2
            } else if qt.chars().count() >= 4 {
                1
            } else {
                0
            };
            if max_d > 0
                && cand_tokens
                    .iter()
                    .any(|ct| damerau_within(ct, qt, max_d).is_some())
            {
                matched += 1;
                fuzzy_bonus += 45;
            }
        }
    }

    if matched == q_tokens.len() {
        return (3_200 + fuzzy_bonus - (extra_words * 25).min(300)).max(2_500);
    }

    if matched > 0 {
        let ratio = (matched * 1800) / q_tokens.len();
        return ratio as i64 + fuzzy_bonus;
    }

    if cand_compact.contains(&q_compact) && q_compact.chars().count() >= 3 {
        return 1_600;
    }

    0
}

fn candidate_titles(item: &Value) -> Vec<String> {
    let mut titles = Vec::new();
    for key in ["title", "ar_title", "en_title", "rawTitle", "seriesTitle"] {
        if let Some(val) = item[key].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            if !titles.iter().any(|existing: &String| existing == val) {
                titles.push(val.to_owned());
            }
        }
    }
    if let Some(aliases) = item["aliases"].as_array() {
        for alias in aliases.iter().filter_map(Value::as_str).map(str::trim) {
            if !alias.is_empty() && !titles.iter().any(|existing| existing == alias) {
                titles.push(alias.to_owned());
            }
        }
    }
    if let Some(offers) = item["offers"].as_array() {
        for offer in offers {
            for key in ["title", "ar_title", "en_title"] {
                if let Some(val) = offer[key].as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    if !titles.iter().any(|existing| existing == val) {
                        titles.push(val.to_owned());
                    }
                }
            }
            if let Some(aliases) = offer["aliases"].as_array() {
                for alias in aliases.iter().filter_map(Value::as_str).map(str::trim) {
                    if !alias.is_empty() && !titles.iter().any(|existing| existing == alias) {
                        titles.push(alias.to_owned());
                    }
                }
            }
        }
    }
    titles
}

/// Computes a deterministic integer relevance score for sorting search/discovery results.
pub fn score_item(item: &Value, parsed: &ParsedQuery, extra_queries: &[String]) -> i64 {
    let titles = candidate_titles(item);
    let slug = item["url"]
        .as_str()
        .and_then(catalogue::slug_from_url)
        .unwrap_or_default();

    let mut best = 0i64;
    for cand in &titles {
        best = best.max(score_single_title(cand, &parsed.core));
        if !parsed.clean_without_year.is_empty() && parsed.clean_without_year != parsed.core {
            best = best.max(score_single_title(cand, &parsed.clean_without_year));
        }
        for variant in extra_queries {
            let s = score_single_title(cand, variant);
            if s > 0 {
                best = best.max((s - 180).max(0));
            }
        }
    }

    if best == 0 && !slug.is_empty() {
        let slug_score = score_single_title(&slug, &parsed.core);
        if slug_score > 0 {
            best = (slug_score / 4).max(300);
        }
    }

    // Small, bounded tie-breakers so metadata breaks ties cleanly without
    // overriding exact title relevance.
    if let Some(intent) = parsed.intent {
        let kind = item["type"].as_str().unwrap_or("");
        if kind == intent {
            best += 600;
        } else if !kind.is_empty() {
            best -= 250;
        }
    }

    let item_year = item["year"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| item["year"].as_u64().map(|y| y.to_string()))
        .unwrap_or_default();

    if let Some(hint) = &parsed.year_hint {
        if item_year == *hint {
            best += 500;
        }
    }

    if best > 0 {
        if let Some(rating) = item["rating"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| item["rating"].as_f64())
        {
            if rating.is_finite() && rating > 0. {
                best += (rating.min(10.) * 12.) as i64;
            }
        }
        if let Ok(year_num) = item_year.parse::<i64>() {
            best += match year_num {
                2020..=2035 => 40,
                2010..=2019 => 25,
                2000..=2009 => 15,
                _ => 5,
            };
        }
        if item["sources"].as_array().map(|s| s.len()).unwrap_or(0) > 1 {
            best += 50;
        }
    }

    best
}

pub fn sort_by_relevance(items: &mut [Value], parsed: &ParsedQuery, variants: &[String]) {
    items.sort_by(|a, b| {
        let sa = score_item(a, parsed, variants);
        let sb = score_item(b, parsed, variants);
        sb.cmp(&sa)
    });
}

/// Filters locally known catalogue/history/favorite records instantaneously while
/// network search is in flight.
pub fn local_matches(
    pool: &[Value],
    parsed: &ParsedQuery,
    variants: &[String],
    limit: usize,
) -> Vec<Value> {
    if parsed.core.chars().count() < 2 {
        return Vec::new();
    }
    let mut scored: Vec<(i64, Value)> = pool
        .iter()
        .filter_map(|item| {
            let s = score_item(item, parsed, variants);
            (s >= 1_400).then(|| (s, item.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(limit).map(|(_, item)| item).collect()
}

pub fn episode_matches(ep_title: &str, ep_number: &str, raw_query: &str) -> bool {
    let q_norm = normalize_text(raw_query);
    if q_norm.is_empty() {
        return true;
    }
    let hay = normalize_text(&format!("{ep_title} {ep_number}"));
    q_norm.split_whitespace().all(|tok| hay.contains(tok))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_arabic_hamza_diacritics_and_digits() {
        assert_eq!(normalize_text("الفيل الأزرق"), "الفيل الازرق");
        assert_eq!(normalize_text("إِبْرَاهِيمُ الأَبْيَضُ"), "ابراهيم الابيض");
        assert_eq!(normalize_text("الجزيرة ٢"), "الجزيره 2");
        assert_eq!(compact_text("Spider-Man"), "spiderman");
    }

    #[test]
    fn parses_movie_and_series_intent_and_year_cleanly() {
        let p = parse_query("فيلم Inception 2010");
        assert_eq!(p.core, "Inception 2010");
        assert_eq!(p.clean_without_year, "Inception");
        assert_eq!(p.intent, Some("movie"));
        assert_eq!(p.year_hint.as_deref(), Some("2010"));

        let num_only = parse_query("1917");
        assert_eq!(num_only.core, "1917");
        assert_eq!(num_only.year_hint, None);
    }

    #[test]
    fn generates_smart_fallbacks_for_tricky_queries() {
        let spider = parse_query("Spiderman");
        assert!(fallback_variants("Spiderman", &spider)
            .iter()
            .any(|v| v == "Spider-Man"));

        let oppenheimer = parse_query("اوبنهايمر");
        assert!(fallback_variants("اوبنهايمر", &oppenheimer)
            .iter()
            .any(|v| v == "Oppenheimer"));

        let titanic = parse_query("تايتنك");
        let tvars = fallback_variants("تايتنك", &titanic);
        assert!(tvars.iter().any(|v| v == "Titanic"));
        assert!(tvars.iter().any(|v| v == "تيتانيك"));

        let gladiator = parse_query("Gladiator 2");
        assert!(fallback_variants("Gladiator 2", &gladiator)
            .iter()
            .any(|v| v == "Gladiator II"));

        let ibrahim = parse_query("ابراهيم الابيض");
        assert!(fallback_variants("ابراهيم الابيض", &ibrahim)
            .iter()
            .any(|v| v == "إبراهيم الأبيض"));
    }

    #[test]
    fn ranks_exact_movie_above_partial_noise_and_remakes() {
        let parsed = parse_query("The Batman");
        let older = json!({"title": "Batman", "type": "movie", "year": "1989", "rating": "7.5"});
        let exact = json!({"title": "The Batman", "type": "movie", "year": "2022", "rating": "7.8"});
        assert!(score_item(&exact, &parsed, &[]) > score_item(&older, &parsed, &[]));

        let game_q = parse_query("اللعبة");
        let end = json!({"title": "المنتقمون: نهاية اللعبة", "aliases": ["Avengers: Endgame"], "type": "movie", "year": "2019"});
        let game = json!({"title": "اللعبة", "type": "series", "year": "2020"});
        assert!(score_item(&game, &game_q, &[]) > score_item(&end, &game_q, &[]));

        let elephant_q = parse_query("الفيل الازرق");
        let elephant = json!({"title": "الفيل", "type": "movie", "year": "2023"});
        let blue_elephant = json!({"title": "الفيل الأزرق", "type": "movie", "year": "2014"});
        assert!(score_item(&blue_elephant, &elephant_q, &[]) > score_item(&elephant, &elephant_q, &[]));
    }

    #[test]
    fn ranks_bilingual_fallback_matches_cleanly() {
        let parsed = parse_query("اوبنهايمر");
        let variants = fallback_variants("اوبنهايمر", &parsed);
        let movie = json!({"title": "Oppenheimer", "aliases": ["Oppenheimer"], "type": "movie", "year": "2023"});
        let unrelated = json!({"title": "Into the Inferno", "type": "movie", "year": "2016"});
        assert!(score_item(&movie, &parsed, &variants) >= STRONG_MATCH_THRESHOLD);
        assert!(score_item(&movie, &parsed, &variants) > score_item(&unrelated, &parsed, &variants));
    }

    #[test]
    fn local_matches_and_episode_search_handle_arabic_and_digits() {
        assert!(episode_matches("الحلقة 5", "5", "٥"));
        let pool = vec![
            json!({"title": "مسلسل التجربة", "type": "series", "url": "https://krmzi.org/series/exp/"}),
            json!({"title": "Oppenheimer", "aliases": ["أوبنهايمر"], "type": "movie", "url": "cinemana:1065008"}),
        ];
        let p = parse_query("اوبنهايمر");
        let variants = fallback_variants("اوبنهايمر", &p);
        let hits = local_matches(&pool, &p, &variants, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["title"], "Oppenheimer");
    }
}
