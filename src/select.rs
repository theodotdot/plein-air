use std::{cell::RefCell, collections::HashMap};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ctx::Ctx;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Depicts {
    pub weather: Option<String>,
    pub time: Option<String>,
    pub season: Option<String>,
    pub scene: Option<String>,
}

impl Depicts {
    fn confidence(&self) -> usize {
        [&self.weather, &self.time, &self.season, &self.scene].iter().filter(|f| f.is_some()).count()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Painting {
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub year: Value,
    pub image: String,
    #[serde(default)]
    pub wiki: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub depicts: Option<Depicts>,
    #[serde(default)]
    pub visual_description: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub content_keywords: Vec<String>,
    #[serde(default)]
    pub mood_tags: Vec<String>,
    #[serde(default = "curated")]
    pub source: String,
    /// Stable id used for the recently-shown list.
    #[serde(default)]
    pub key: String,
}

fn curated() -> String {
    "curated".into()
}

impl Painting {
    pub fn key(&self) -> String {
        if self.key.is_empty() { format!("{}:{}", self.source, self.image) } else { self.key.clone() }
    }
}

pub fn has(pat: &'static str, text: &str) -> bool {
    thread_local! {
        static CACHE: RefCell<HashMap<&'static str, Regex>> = RefCell::new(HashMap::new());
    }
    CACHE.with(|c| {
        c.borrow_mut().entry(pat).or_insert_with(|| Regex::new(pat).unwrap()).is_match(text)
    })
}

fn opt(s: &str) -> Option<String> {
    Some(s.to_string())
}

/// Guess what an external painting shows from its title and museum metadata.
pub fn infer_depicts(title: &str, content: &str) -> Depicts {
    let blob = format!("{} {}", title.to_lowercase(), content);
    let h = |p: &'static str| has(p, &blob);

    let weather = if h(r"\b(storm|tempest|thunder|lightning|gale|squall|tempestuous)\b") {
        opt("storm")
    } else if h(r"\b(snow|snowy|snowfall|blizzard|frost|frozen|icy|icicle|skater)\b") {
        opt("snow")
    } else if h(r"\b(rain|raining|rainy|shower|downpour|drizzle|wet)\b") {
        opt("rain")
    } else if h(r"\b(fog|mist|misty|foggy|haze|hazy|brume)\b") {
        opt("fog")
    } else if h(r"\b(overcast|grey sky|gray sky|gloomy|leaden|sombre)\b") {
        opt("overcast")
    } else if h(r"\b(sunlit|sunshine|sunny|clear sky|sunlight|brilliant sun|noonday|blue sky)\b") {
        opt("clear")
    } else if h(r"\b(cloud|clouds|partly cloudy|broken cloud)\b") {
        opt("partly_cloudy")
    } else {
        None
    };

    let time = if h(r"\b(nocturne|night|midnight|moonlit|moon|stars?|starry)\b") {
        opt("night")
    } else if h(r"\b(dawn|sunrise|aurora|first light|early morning)\b") {
        opt("dawn")
    } else if h(r"\b(sunset|sundown|dusk|twilight|evening light|gloaming)\b") {
        opt("dusk")
    } else if h(r"\b(noon|midday|high noon|zenith)\b") {
        opt("noon")
    } else if h(r"\b(morning)\b") {
        opt("morning")
    } else if h(r"\b(afternoon)\b") {
        opt("afternoon")
    } else if h(r"\b(evening)\b") {
        opt("evening")
    } else {
        None
    };

    let season = if h(r"\b(winter|january|february|frozen|snow)\b") {
        opt("winter")
    } else if h(r"\b(spring|blossom|blooms?|flowering|easter|april|may flowers)\b") {
        opt("spring")
    } else if h(r"\b(summer|july|august|harvest|haystack|wheatfield|hot afternoon)\b") {
        opt("summer")
    } else if h(r"\b(autumn|fall|foliage|october|november|golden trees)\b") {
        opt("autumn")
    } else {
        None
    };

    let scene = if h(r"\b(cliff|coast|harbor|harbour|beach|shore|seaside|seascape)\b") {
        opt("coastal")
    } else if h(r"\b(sea|ocean|wave|surf|tide|maritime)\b") {
        opt("sea")
    } else if h(r"\b(mountain|peak|alpine|sierra|himalaya|summit)\b") {
        opt("mountain")
    } else if h(r"\b(forest|woods?|grove|copse)\b") {
        opt("forest")
    } else if h(r"\b(river|stream|brook|riverbank|riverside|lake|pond)\b") {
        opt("riverside")
    } else if h(r"\b(garden|park)\b") {
        opt("garden")
    } else if h(r"\b(street|city|paris|london|town|village|boulevard|cathedral|church|station)\b") {
        opt("urban")
    } else if h(r"\b(meadow|field|farm|pastoral|countryside|haystack|wheatfield)\b") {
        opt("pastoral")
    } else if h(r"\b(arctic|polar|tundra|iceberg)\b") {
        opt("arctic")
    } else if h(r"\b(desert|sahara|dune|oasis)\b") {
        opt("desert")
    } else if h(r"\b(tropical|jungle|palm)\b") {
        opt("tropical")
    } else {
        None
    };

    Depicts { weather, time, season, scene }
}

#[derive(PartialEq)]
enum Cls {
    Excluded,
    Preferred,
    Neutral,
}

fn classify_content(text: &str, ctx: &Ctx) -> Cls {
    let exclusions: &[&str] = match ctx.weather {
        "clear" => &[
            "storm", "tempest", "snow", "snowfall", "blizzard", "rain", "rainy", "rainfall", "fog", "mist",
            "misty", "foggy", "overcast", "thunder", "lightning", "tempestuous",
        ],
        "partly_cloudy" => &["storm", "tempest", "snow", "blizzard", "heavy rain", "thunder", "lightning"],
        "overcast" => &["sunshine", "sunlight", "clear sky", "noonday"],
        "fog" => &["storm", "tempest", "sunshine", "sunny", "clear sky"],
        "rain" => &["snow", "snowfall", "blizzard", "sunshine", "clear sky"],
        "snow" => &["rain", "rainy", "sunshine", "tropical", "desert"],
        _ => &["snow", "snowfall", "sunshine", "clear sky", "calm"],
    };
    let weather_affinity: &[&str] = match ctx.weather {
        "clear" => &[
            "clear sky", "sunshine", "sunlight", "sunny", "sunset", "sunrise", "daylight", "noonday", "blue sky",
        ],
        "partly_cloudy" => &["clouds", "cloud", "sky"],
        "overcast" => &["overcast", "clouds", "grey sky", "gray sky", "gloomy", "somber"],
        "fog" => &["fog", "mist", "misty", "foggy", "haze", "vapor", "vapour"],
        "rain" => &["rain", "rainfall", "showers", "rainy", "wet", "puddle", "umbrella"],
        "snow" => &["snow", "snowfall", "snowy", "winter", "ice", "frost", "frozen", "icicle", "snowscape"],
        _ => &["storm", "tempest", "thunder", "lightning", "gale", "squall", "rough sea", "tempestuous", "wreck"],
    };
    let season_affinity: &[&str] = match ctx.season {
        "winter" => &["winter", "snow", "ice", "frost", "bare trees", "january", "february"],
        "spring" => &["spring", "blossom", "blossoms", "bloom", "flowering", "easter", "april", "may"],
        "summer" => &["summer", "harvest", "meadow", "sunshine", "july", "august", "swimmers", "beach"],
        _ => &["autumn", "fall", "foliage", "harvest", "october", "november", "golden trees"],
    };
    // Plain substring matching, same as the site.
    if exclusions.iter().any(|e| text.contains(e)) {
        return Cls::Excluded;
    }
    if weather_affinity.iter().chain(season_affinity).any(|a| text.contains(a)) {
        return Cls::Preferred;
    }
    Cls::Neutral
}

pub struct Candidate {
    pub painting: Painting,
    /// Lowercased searchable text from the museum record.
    pub content: String,
}

/// Drop candidates that clash with the moment, rank the rest, return the top 5 shuffled.
pub fn select_by_content(cands: Vec<Candidate>, ctx: &Ctx) -> Vec<Painting> {
    let ctime = ctx.time;
    let mut scored: Vec<(i32, Painting)> = cands
        .into_iter()
        .filter_map(|c| {
            let cls = classify_content(&c.content, ctx);
            if cls == Cls::Excluded {
                return None;
            }
            let d = c.painting.depicts.clone().unwrap_or_default();
            let is = |f: &Option<String>, v: &str| f.as_deref() == Some(v);
            let dt = d.time.as_deref().unwrap_or("");
            if is(&d.weather, "snow") && ctx.temp > 6
                || is(&d.season, "winter") && ctx.temp > 12
                || is(&d.weather, "storm") && ctx.weather == "clear"
                || is(&d.season, "summer") && ctx.temp < 0
                || dt == "night" && ["morning", "noon", "afternoon"].contains(&ctime)
            {
                return None;
            }
            let blob = &c.content;
            let bright_day = ["noon", "morning", "afternoon"].contains(&dt)
                || has(r"\b(sun|sunlit|sunshine|sunny|sunlight|bright|noonday)\b", blob);
            let dim = ["dawn", "dusk", "evening", "night"].contains(&dt)
                || has(
                    r"\b(evening|dusk|sunset|sundown|sunrise|dawn|twilight|gloaming|nocturne|night|moon|stars?|midnight)\b",
                    blob,
                );
            match ctx.light {
                "night" if bright_day && !dim => return None,
                "bright" if dim && !bright_day => return None,
                "twilight" | "pre_dawn" if bright_day && !dim => return None,
                _ => {}
            }
            if d.confidence() < 2 {
                return None;
            }
            let mut s = 0;
            if is(&d.weather, ctx.weather) {
                s += 4;
            }
            if is(&d.season, ctx.season) {
                s += 3;
            }
            if dt == ctime {
                s += 2;
            }
            if cls == Cls::Preferred {
                s += 2;
            }
            Some((s, c.painting))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(5);
    let mut top: Vec<Painting> = scored.into_iter().map(|(_, p)| p).collect();
    fastrand::shuffle(&mut top);
    top
}

/// Last gate: catches mismatches that only show up in the title or description.
pub fn validate(p: &Painting, ctx: &Ctx) -> Result<(), String> {
    let d = p.depicts.clone().unwrap_or_default();
    let f = |o: &Option<String>| o.clone().unwrap_or_default();
    let blob = [
        p.title.clone(),
        p.visual_description.clone(),
        p.description.clone(),
        p.content_keywords.join(" "),
        p.tags.join(" "),
        p.mood_tags.join(" "),
        format!("{} {} {} {}", f(&d.weather), f(&d.time), f(&d.season), f(&d.scene)),
    ]
    .join(" ")
    .to_lowercase();
    let h = |pat: &'static str| has(pat, &blob);

    let night = h(r"\b(night|nocturne|midnight|moon|moonlit|stars?|starry|starlight)\b");
    let evening = h(r"\b(evening|dusk|sunset|sundown|twilight|gloaming)\b");
    let midday = h(r"\b(noon|midday|noonday|sunlit|sunshine|sunny|bright sun)\b");
    let morning = h(r"\bmorning\b");
    let afternoon = h(r"\bafternoon\b");
    let snow = h(r"\b(snow|snowy|snowfall|blizzard|frost|frozen|icicle|skater)\b");
    let storm = h(r"\b(storm|tempest|thunder|lightning|gale|squall|wreck)\b");
    let fog = h(r"\b(fog|mist|misty|foggy|haze|hazy)\b");
    let winter = h(r"\bwinter\b") || snow;
    let summer = h(r"\b(summer|harvest|haystack|wheat field|sunbathing)\b");
    let (temp, season) = (ctx.temp, ctx.season);

    let fail = |r: &str| Err(r.to_string());
    match ctx.light {
        "bright" if night => return fail("night content in bright daylight"),
        "bright" if evening => return fail("evening/sunset content in bright midday"),
        "night" if midday => return fail("sunlit / midday content at night"),
        "night" if morning => return fail("morning content at night"),
        "night" if afternoon => return fail("afternoon content at night"),
        "twilight" | "pre_dawn" if midday => return fail("midday/sunlit content during twilight"),
        "golden_dawn" | "golden_dusk" if night && !evening => {
            return fail("deep-night content during golden hour");
        }
        _ => {}
    }
    if snow && temp > 6 {
        return Err(format!("snow content at {temp}°C"));
    }
    if storm && ctx.weather == "clear" {
        return fail("storm content under clear sky");
    }
    if fog && ctx.weather == "clear" && !["dawn", "dusk"].contains(&ctx.time) {
        return fail("fog content under clear sky");
    }
    if winter && season == "summer" && temp > 18 {
        return fail("winter content during warm summer");
    }
    if summer && season == "winter" && temp < 5 {
        return fail("summer content during winter cold");
    }

    let title = p.title.to_lowercase();
    let named = |pat: &'static str| has(pat, &title);
    let wrong_season = |s: &str| Err(format!("\"{}\" is a {s} painting (it's {season})", p.title));
    if named(r"\bautumn\b|\bfall\b|\bharvest\b") && season != "autumn" {
        return wrong_season("autumn");
    }
    if named(r"\bspring\b|\bblossom") && season != "spring" {
        return wrong_season("spring");
    }
    if named(r"\bsummer\b") && season != "summer" {
        return wrong_season("summer");
    }
    if named(r"\bwinter\b|\bsnow\b") && season != "winter" && temp > 5 {
        return wrong_season("winter");
    }
    Ok(())
}

fn shared_mood(p: &Painting, ctx: &Ctx) -> bool {
    let mut moods: Vec<&str> = Vec::new();
    match ctx.time {
        "dawn" | "morning" => moods.extend(["soft", "still"]),
        "dusk" | "evening" => moods.extend(["golden", "warm", "nostalgic"]),
        "night" => moods.extend(["still", "peaceful", "melancholy"]),
        "noon" => moods.extend(["bright", "warm"]),
        _ => {}
    }
    match ctx.weather {
        "storm" => moods.extend(["dramatic", "restless", "ominous"]),
        "rain" => moods.extend(["somber", "soft"]),
        "fog" => moods.extend(["still", "isolated", "soft"]),
        "snow" => moods.extend(["cold", "still", "peaceful"]),
        "clear" if ctx.temp > 20 => moods.extend(["bright", "warm"]),
        _ => {}
    }
    match ctx.season {
        "winter" => moods.extend(["cold", "isolated"]),
        "summer" => moods.extend(["warm", "lush"]),
        "autumn" => moods.extend(["golden", "nostalgic"]),
        _ => moods.extend(["soft", "joyful"]),
    }
    p.mood_tags.iter().any(|m| moods.contains(&m.as_str()))
}

fn weather_neighbors(w: &str) -> &'static [&'static str] {
    match w {
        "clear" => &["clear", "partly_cloudy"],
        "partly_cloudy" => &["clear", "partly_cloudy", "overcast"],
        "overcast" => &["overcast", "partly_cloudy", "fog"],
        "fog" => &["fog", "overcast"],
        "rain" => &["rain", "overcast", "storm"],
        "snow" => &["snow"],
        _ => &["storm", "rain"],
    }
}

/// Curated-set scoring. Feedback-based terms from the site are left out (no UI here).
pub fn score(p: &Painting, ctx: &Ctx, recent: bool) -> i32 {
    let tag = |t: &str| p.tags.iter().any(|x| x == t);
    let kw = p.content_keywords.join(" ").to_lowercase();
    let d = p.depicts.clone();
    let dt = d.as_ref().and_then(|d| d.time.clone()).unwrap_or_default();
    let dt = dt.as_str();
    let (temp, ctime) = (ctx.temp, ctx.time);
    let mut s = 0;

    let depicts_snow = d.as_ref().is_some_and(|d| {
        d.season.as_deref() == Some("winter") || d.weather.as_deref() == Some("snow")
    });
    let snowy = depicts_snow
        || has(r"(?i)\b(snow|snowy|snowfall|snowflake|snowscape|ice|icy|icicle|frozen|frost|skater|sled|blizzard)\b", &kw);
    if snowy && temp > 6 {
        s -= 30;
    }
    if snowy && temp > 14 {
        s -= 20;
    }
    if has(r"(?i)\b(tropical|tropics|desert|sweltering|sunbath|bath(?:ers|ing))\b", &kw) && temp < 8 {
        s -= 25;
    }

    let bright_day = ["noon", "morning", "afternoon"].contains(&dt)
        || has(r"(?i)\b(sun|sunlit|sunshine|sunny|sunlight|bright|brilliant|noonday)\b", &kw);
    let golden = ["dawn", "dusk"].contains(&dt)
        || has(r"(?i)\b(sunset|sundown|sunrise|dusk|twilight|gloaming|golden)\b", &kw);
    let nocturnal = ["night", "evening"].contains(&dt)
        || has(r"(?i)\b(night|nocturne|moon|moonlit|stars?|starlight|midnight)\b", &kw);
    let dim = ["dawn", "dusk", "evening", "night"].contains(&dt)
        || has(
            r"(?i)\b(evening|dusk|sunset|sundown|twilight|gloaming|nocturne|night|moon|stars?|midnight|sunrise|dawn)\b",
            &kw,
        );

    match ctx.light {
        "night" => {
            if bright_day && !dim {
                s -= 40;
            } else if golden && !nocturnal {
                s -= 18;
            }
            if nocturnal {
                s += 6;
            }
        }
        "twilight" | "pre_dawn" => {
            if bright_day && !dim {
                s -= 22;
            }
            if golden {
                s += 4;
            }
            if nocturnal {
                s += 2;
            }
        }
        "golden_dawn" | "golden_dusk" => {
            if golden {
                s += 10;
            }
            if dim && !nocturnal {
                s += 4;
            }
            if dt == "noon" {
                s -= 6;
            }
            if nocturnal {
                s -= 8;
            }
        }
        "bright" if dim && !bright_day => s -= if dt == "night" { 30 } else { 25 },
        _ => {}
    }

    if let Some(d) = &d {
        let dw = d.weather.as_deref().unwrap_or("");
        if dw == ctx.weather {
            s += 14;
        } else if weather_neighbors(ctx.weather).contains(&dw) {
            s += 5;
        } else {
            s -= 8;
        }

        let ds = d.season.as_deref().unwrap_or("");
        let cs = ctx.season;
        if ds == cs {
            s += 8;
        } else if ds == "winter" && temp > 8 {
            s -= 12;
        } else if ds == "summer" && temp < 5 {
            s -= 10;
        } else if (ds == "autumn" && cs == "winter")
            || (ds == "winter" && cs == "autumn" && temp < 8)
            || (ds == "spring" && cs == "summer")
            || (ds == "summer" && cs == "spring" && temp > 12)
        {
            s += 1;
        } else {
            s -= 5;
        }

        if dt == ctime {
            s += 6;
        } else {
            let buckets: [&[&str]; 4] =
                [&["dawn", "morning"], &["morning", "noon", "afternoon"], &["dusk", "evening"], &["night"]];
            if buckets.iter().any(|b| b.contains(&dt) && b.contains(&ctime)) {
                s += 2;
            }
            let day = ["morning", "noon", "afternoon"].contains(&ctime);
            if dt == "night" && day {
                s -= 8;
            }
            if dt == "noon" && ctime == "night" {
                s -= 6;
            }
        }

        if shared_mood(p, ctx) {
            s += 2;
        }
    }

    let day_time = ["dawn", "morning", "noon", "afternoon"].contains(&ctime);
    let night_time = ["night", "evening"].contains(&ctime);
    if tag(ctx.weather) {
        s += 4;
    }
    if tag(ctx.season) {
        s += 4;
    }
    if tag(ctime) {
        s += 2;
    }
    if tag("snow") && temp > 4 {
        s -= 10;
    }
    if tag("freezing") && temp > 4 {
        s -= 8;
    }
    if tag("winter") && ctx.season == "summer" {
        s -= 10;
    }
    if tag("winter") && ctx.season == "spring" && temp > 8 {
        s -= 8;
    }
    if tag("summer") && ctx.season == "winter" {
        s -= 6;
    }
    if (tag("hot") || tag("lush")) && temp < 5 {
        s -= 4;
    }
    if tag("night") && day_time {
        s -= 4;
    }
    if tag("noon") && night_time {
        s -= 3;
    }
    if tag("dawn") && night_time {
        s -= 2;
    }
    if tag("cold") && temp > 22 {
        s -= 3;
    }
    s += 2 * ctx.temp_tags.iter().filter(|t| tag(t)).count() as i32;
    for set in [&ctx.light_tags, &ctx.wind_tags, &ctx.season_tags] {
        s += set.iter().filter(|t| tag(t)).count() as i32;
    }
    if tag(ctx.season) && tag(ctx.weather) {
        s += 3;
    }
    if recent {
        s -= 8;
    }
    s
}

/// Site's curated pick: weighted draw from the top 7 positive scores,
/// without replacement, followed by the rest of the positive list so callers
/// always have something to fall through to.
pub fn curated_order(paintings: &[Painting], ctx: &Ctx, recent: &[String]) -> Vec<Painting> {
    let mut ranked: Vec<(i32, &Painting)> =
        paintings.iter().map(|p| (score(p, ctx, recent.contains(&p.key())), p)).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    let positive: Vec<_> = ranked.iter().filter(|(s, _)| *s > 0).cloned().collect();
    if positive.is_empty() {
        return ranked.first().map(|(_, p)| (*p).clone()).into_iter().collect();
    }
    let (top, rest) = positive.split_at(positive.len().min(7));
    let mut pool = top.to_vec();
    let mut out = Vec::new();
    while !pool.is_empty() {
        let total: i32 = pool.iter().map(|(s, _)| (*s).max(1)).sum();
        let mut r = fastrand::i32(0..total);
        let i = pool
            .iter()
            .position(|(s, _)| {
                r -= (*s).max(1);
                r < 0
            })
            .unwrap_or(0);
        out.push(pool.remove(i).1.clone());
    }
    out.extend(rest.iter().map(|(_, p)| (*p).clone()));
    out
}
