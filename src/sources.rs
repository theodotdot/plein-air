use anyhow::{Context, Result};
use serde_json::Value;

use crate::{
    ctx::Ctx,
    net,
    select::{Candidate, Painting, infer_depicts, select_by_content},
};

pub const CURATED_URL: &str = "https://art.joonas.wtf/paintings.js";

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

fn join_strs(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

fn ext_painting(title: &str, artist: &str, year: String, image: String, content: String, source: &str, key: String) -> Candidate {
    Candidate {
        painting: Painting {
            artist: if artist.is_empty() { "Unknown artist".into() } else { artist.into() },
            title: if title.is_empty() { "Untitled".into() } else { title.into() },
            year: Value::String(year),
            image,
            depicts: Some(infer_depicts(title, &content)),
            description: content.clone(),
            source: source.into(),
            key,
            ..Default::default()
        },
        content,
    }
}

/// Site's query, then shorter ones: Met and Cleveland AND every term, so long queries starve.
pub fn search_queries(ctx: &Ctx) -> Vec<String> {
    let w = match ctx.weather {
        "clear" if ctx.time == "night" => "nocturne night stars moonlight",
        "clear" if ["dawn", "dusk"].contains(&ctx.time) => "sunrise sunset landscape",
        "clear" if ctx.season == "summer" => "summer landscape sunshine",
        "clear" => "landscape clear sky",
        "partly_cloudy" => "landscape clouds",
        "overcast" => "overcast grey landscape",
        "fog" => "fog mist foggy landscape",
        "rain" => "rain shower landscape",
        "snow" => "snow winter landscape",
        "storm" => "storm tempest sea landscape",
        _ => "landscape",
    };
    let key = match ctx.weather {
        "clear" if ctx.time == "night" => "moonlight",
        "clear" if ctx.time == "dawn" => "sunrise",
        "clear" if ctx.time == "dusk" => "sunset",
        "clear" | "partly_cloudy" | "overcast" => "landscape",
        "fog" => "mist",
        w => w,
    };
    let season = match ctx.season {
        "autumn" => "autumn fall",
        s => s,
    };
    let short_season = ctx.season;
    vec![format!("{w} {season}"), format!("{key} {short_season}"), key.to_string()]
}

/// First query with at least `min` hits.
fn first_with_hits<T>(ctx: &Ctx, min: usize, mut run: impl FnMut(&str) -> Result<Vec<T>>) -> Result<Vec<T>> {
    let mut best = Vec::new();
    for q in search_queries(ctx) {
        let hits = run(&q)?;
        if hits.len() >= min {
            return Ok(hits);
        }
        if hits.len() > best.len() {
            best = hits;
        }
    }
    Ok(best)
}

pub fn met(ctx: &Ctx, _width: u32) -> Result<Vec<Painting>> {
    // 11 European, 17 American, 21 Modern, 6 Asian; European weighted like the site.
    let dept = [11, 11, 11, 17, 17, 21, 6][fastrand::usize(0..7)].to_string();
    let mut ids = first_with_hits(ctx, 12, |q| {
        let v = net::json(
            "https://collectionapi.metmuseum.org/public/collection/v1.1/search",
            &[("q", q), ("hasImages", "true"), ("isPublicDomain", "true"), ("medium", "Paintings"), ("departmentId", &dept), ("limit", "100")],
        )?;
        Ok(v["objectIDs"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default())
    })?;
    fastrand::shuffle(&mut ids);
    // Met metadata is sparse, so most objects fail the confidence check; sample wider than the site's 12.
    ids.truncate(30);

    // Few workers: the Met 403s on bursts.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let cands = std::thread::scope(|sc| {
        let workers: Vec<_> = (0..6)
            .map(|_| {
                sc.spawn(|| {
                    let mut got = Vec::new();
                    while let Some(id) = ids.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                        let url = format!("https://collectionapi.metmuseum.org/public/collection/v1/objects/{id}");
                        if let Ok(o) = net::json(&url, &[]) {
                            got.push(o);
                        }
                    }
                    got
                })
            })
            .collect();
        workers.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect::<Vec<_>>()
    });
    let cands = cands
        .into_iter()
        .filter(|o| o["isPublicDomain"].as_bool() == Some(true) && o["classification"] == "Paintings" && !s(&o["primaryImage"]).is_empty())
        .map(|o| {
            let tags = o["tags"].as_array().map(|a| a.iter().map(|t| s(&t["term"])).collect::<Vec<_>>().join(" ")).unwrap_or_default();
            let content = [s(&o["title"]), s(&o["objectName"]), s(&o["objectDate"]), s(&o["classification"]), s(&o["medium"]), tags]
                .join(" ")
                .to_lowercase();
            ext_painting(
                &s(&o["title"]),
                &s(&o["artistDisplayName"]),
                s(&o["objectDate"]),
                s(&o["primaryImage"]),
                content,
                "met",
                format!("met:{}", o["objectID"]),
            )
        })
        .collect();
    Ok(select_by_content(cands, ctx))
}

pub fn artic(ctx: &Ctx, width: u32) -> Result<Vec<Painting>> {
    let q = &search_queries(ctx)[0];
    // Combining two query[term] filters 400s now; filter type locally.
    let v = net::json(
        "https://api.artic.edu/api/v1/artworks/search",
        &[
            ("q", q),
            ("query[term][is_public_domain]", "true"),
            ("fields", "id,artwork_type_title,title,artist_title,date_display,image_id,medium_display,short_description,subject_titles,term_titles,classification_titles,thumbnail"),
            ("limit", "40"),
        ],
    )?;
    let cands = v["data"]
        .as_array()
        .context("artic: no data")?
        .iter()
        .filter(|o| !s(&o["image_id"]).is_empty() && o["artwork_type_title"] == "Painting")
        .map(|o| {
            let content = [
                s(&o["title"]),
                s(&o["medium_display"]),
                s(&o["short_description"]),
                join_strs(&o["subject_titles"]),
                join_strs(&o["term_titles"]),
                join_strs(&o["classification_titles"]),
            ]
            .join(" ")
            .to_lowercase();
            // IIIF won't upscale; cap at the original width.
            let max_w = o["thumbnail"]["width"].as_u64().unwrap_or(u64::MAX).min(width as u64);
            ext_painting(
                &s(&o["title"]),
                &s(&o["artist_title"]),
                s(&o["date_display"]),
                format!("https://www.artic.edu/iiif/2/{}/full/{max_w},/0/default.jpg", s(&o["image_id"])),
                content,
                "artic",
                format!("artic:{}", o["id"]),
            )
        })
        .collect();
    Ok(select_by_content(cands, ctx))
}

pub fn cleveland(ctx: &Ctx, width: u32) -> Result<Vec<Painting>> {
    let items = first_with_hits(ctx, 10, |q| {
        let v = net::json(
            "https://openaccess-api.clevelandart.org/api/artworks/",
            &[("q", q), ("type", "Painting"), ("has_image", "1"), ("cc0", "1"), ("limit", "50")],
        )?;
        Ok(v["data"].as_array().cloned().unwrap_or_default())
    })?;
    let cands = items
        .iter()
        .filter(|o| !s(&o["images"]["web"]["url"]).is_empty())
        .map(|o| {
            let subjects = o["subjects"].as_array().map(|a| a.iter().map(|x| s(&x["description"])).collect::<Vec<_>>().join(" ")).unwrap_or_default();
            let content = [s(&o["title"]), s(&o["wall_description"]), s(&o["description"]), s(&o["tombstone"]), s(&o["technique"]), subjects]
                .join(" ")
                .to_lowercase();
            // web is ~900px; print is ~3400px. Prefer print for large screens.
            let img = &o["images"];
            let print = s(&img["print"]["url"]);
            let image = if width > 900 && !print.is_empty() { print } else { s(&img["web"]["url"]) };
            let creator = s(&o["creators"][0]["description"]);
            let artist = creator.split('(').next().unwrap_or("").trim().to_string();
            ext_painting(&s(&o["title"]), &artist, s(&o["creation_date"]), image, content, "cleveland", format!("cleveland:{}", s(&o["url"])))
        })
        .collect();
    Ok(select_by_content(cands, ctx))
}

fn wikimedia_category(ctx: &Ctx) -> &'static str {
    match (ctx.weather, ctx.time, ctx.season) {
        ("snow", _, _) => "Category:Snow_in_paintings",
        ("rain", _, _) => "Category:Rain_in_paintings",
        ("fog", _, _) => "Category:Fog_in_paintings",
        ("storm", _, _) => "Category:Storms_in_paintings",
        (_, "night", _) => "Category:Nocturne_paintings",
        (_, "dawn", _) => "Category:Sunrises_in_paintings",
        (_, "dusk", _) => "Category:Sunsets_in_paintings",
        (_, _, "winter") => "Category:Winter_in_paintings",
        (_, _, "summer") => "Category:Summer_in_paintings",
        (_, _, "autumn") => "Category:Autumn_in_paintings",
        _ => "Category:Spring_in_paintings",
    }
}

pub fn wikimedia(ctx: &Ctx, width: u32) -> Result<Vec<Painting>> {
    const API: &str = "https://commons.wikimedia.org/w/api.php";
    let cat = wikimedia_category(ctx);
    let v = net::json(
        API,
        &[("action", "query"), ("list", "categorymembers"), ("format", "json"), ("cmtitle", cat), ("cmtype", "file"), ("cmlimit", "200")],
    )?;
    let mut files: Vec<String> = v["query"]["categorymembers"]
        .as_array()
        .context("wikimedia: no members")?
        .iter()
        .map(|f| s(&f["title"]))
        .filter(|t| {
            let t = t.to_lowercase();
            [".jpg", ".jpeg", ".png", ".tif", ".tiff"].iter().any(|e| t.ends_with(e))
        })
        .collect();
    fastrand::shuffle(&mut files);

    let width = width.to_string();
    let cat_l = cat.to_lowercase();
    let mut out = Vec::new();
    // Site picks one file; we try a few so a single bad file doesn't sink the source.
    for file in files.iter().take(6) {
        let v = net::json(
            API,
            &[("action", "query"), ("format", "json"), ("prop", "imageinfo"), ("iiprop", "url|size|extmetadata"), ("iiurlwidth", &width), ("titles", file)],
        )?;
        let Some(page) = v["query"]["pages"].as_object().and_then(|p| p.values().next()) else { continue };
        let info = &page["imageinfo"][0];
        let meta = &info["extmetadata"];
        let title = Some(strip_html(&s(&meta["ObjectName"]["value"])))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| file.trim_start_matches("File:").rsplit_once('.').map_or(file.as_str(), |(n, _)| n).replace('_', " "));
        let desc = strip_html(&s(&meta["ImageDescription"]["value"]));
        let mut d = infer_depicts(&title, &format!("{} {}", desc.to_lowercase(), cat_l));
        let fill = |f: &mut Option<String>, pairs: &[(&str, &str)]| {
            if f.is_none() {
                *f = pairs.iter().find(|(k, _)| cat_l.contains(k)).map(|(_, v)| v.to_string());
            }
        };
        fill(&mut d.season, &[("winter", "winter"), ("summer", "summer"), ("autumn", "autumn"), ("spring", "spring")]);
        fill(&mut d.weather, &[("snow", "snow"), ("storm", "storm"), ("rain", "rain"), ("fog", "fog")]);
        fill(&mut d.time, &[("sunrise", "dawn"), ("sunset", "dusk"), ("nocturne", "night")]);
        let conf = [&d.weather, &d.time, &d.season, &d.scene].iter().filter(|f| f.is_some()).count();
        let is = |f: &Option<String>, v: &str| f.as_deref() == Some(v);
        if conf < 2 || is(&d.weather, "snow") && ctx.temp > 6 || is(&d.season, "winter") && ctx.temp > 12 || is(&d.season, "summer") && ctx.temp < 0 {
            continue;
        }
        // Original is smaller than requested → thumburl is absent; use the original.
        let image = Some(s(&info["thumburl"])).filter(|u| !u.is_empty()).unwrap_or_else(|| s(&info["url"]));
        out.push(Painting {
            artist: Some(strip_html(&s(&meta["Artist"]["value"]))).filter(|a| !a.is_empty()).unwrap_or("Unknown artist".into()),
            title: title.clone(),
            year: Value::String(strip_html(&s(&meta["DateTimeOriginal"]["value"]))),
            image,
            description: desc,
            content_keywords: vec![title.to_lowercase(), cat_l.trim_start_matches("category:").to_string()],
            depicts: Some(d),
            source: "wikimedia".into(),
            key: format!("wikimedia:{file}"),
            ..Default::default()
        });
    }
    Ok(out)
}

/// Wikimedia throttles original-file URLs but serves thumbnails freely, and asks clients to use those.
/// Turns `.../commons/a/ab/File.jpg` into `.../commons/thumb/a/ab/File.jpg/<width>px-File.jpg`.
pub fn wikimedia_thumb(url: &str, width: u32) -> String {
    const BASE: &str = "https://upload.wikimedia.org/wikipedia/commons/";
    let Some(rest) = url.strip_prefix(BASE) else { return url.to_string() };
    let rest = rest.split('?').next().unwrap_or(rest);
    let rest = rest.strip_prefix("thumb/").unwrap_or(rest);
    let mut parts = rest.splitn(3, '/');
    let (Some(a), Some(ab), Some(tail)) = (parts.next(), parts.next(), parts.next()) else {
        return url.to_string();
    };
    // `tail` is the file name, or `File.jpg/3840px-File.jpg` when it was already a thumbnail.
    let file = tail.split('/').next().unwrap_or(tail);
    // Rasterized as a JPEG; tif/tiff thumbnails come out as jpg, so keep the name usable.
    let lower = file.to_lowercase();
    let suffix = if lower.ends_with(".tif") || lower.ends_with(".tiff") { ".jpg" } else { "" };
    format!("{BASE}thumb/{a}/{ab}/{file}/{width}px-{file}{suffix}")
}

pub fn curated_list() -> Result<Vec<Painting>> {
    let js = net::text(CURATED_URL)?;
    let start = js.find('[').context("paintings.js: no array")?;
    let end = js.rfind(']').context("paintings.js: no array end")?;
    Ok(serde_json::from_str(&js[start..=end]).context("paintings.js: parse")?)
}

pub type SourceFn = fn(&Ctx, u32) -> Result<Vec<Painting>>;

pub const EXTERNAL: &[(&str, SourceFn, u32)] = &[("met", met, 4), ("artic", artic, 5), ("cleveland", cleveland, 3), ("wikimedia", wikimedia, 2)];

/// Weighted shuffle of source order, mirroring the site.
pub fn source_order(only: &[String]) -> Vec<(&'static str, SourceFn)> {
    let mut pool: Vec<_> = EXTERNAL.iter().filter(|(n, _, _)| only.is_empty() || only.iter().any(|o| o == n)).collect();
    let mut out = Vec::new();
    while !pool.is_empty() {
        let total: u32 = pool.iter().map(|(_, _, w)| w).sum();
        let mut r = fastrand::u32(0..total);
        let i = pool.iter().position(|(_, _, w)| { if r < *w { true } else { r -= w; false } }).unwrap();
        let (n, f, _) = pool.remove(i);
        out.push((*n, *f));
    }
    out
}
