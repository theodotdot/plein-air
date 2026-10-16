use std::{
    f64::consts::PI,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::net;

pub struct Location {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
}

#[derive(Serialize)]
pub struct Ctx {
    pub place: String,
    pub lat: f64,
    pub lon: f64,
    pub temp: i64,
    pub weather_code: i64,
    pub weather: &'static str,
    pub season: &'static str,
    pub time: &'static str,
    pub light: &'static str,
    #[serde(skip)]
    pub temp_tags: Vec<&'static str>,
    #[serde(skip)]
    pub wind_tags: Vec<&'static str>,
    #[serde(skip)]
    pub light_tags: Vec<&'static str>,
    #[serde(skip)]
    pub season_tags: Vec<&'static str>,
}

pub fn parse_coords(s: &str) -> Result<(f64, f64)> {
    let (a, b) = s.split_once(',').context("coords must be LAT,LON")?;
    let lat: f64 = a.trim().parse().context("bad latitude")?;
    let lon: f64 = b.trim().parse().context("bad longitude")?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        bail!("coords out of range: {lat},{lon}");
    }
    Ok((lat, lon))
}

pub fn locate_ip() -> Result<Location> {
    let v = net::json("https://ipinfo.io/json", &[])?;
    let (lat, lon) = parse_coords(v["loc"].as_str().context("ipinfo: no loc")?)?;
    Ok(Location { lat, lon, name: v["city"].as_str().unwrap_or("").to_string() })
}

pub fn locate_city(q: &str) -> Result<Location> {
    // Open-Meteo geocoding only matches the bare name, so "Paris, France" → "Paris".
    let name = q.split(',').next().unwrap_or(q).trim();
    let v = net::json(
        "https://geocoding-api.open-meteo.com/v1/search",
        &[("name", name), ("count", "1")],
    )?;
    let r = v["results"].get(0).with_context(|| format!("city not found: {q}"))?;
    Ok(Location {
        lat: r["latitude"].as_f64().context("geocoding: no latitude")?,
        lon: r["longitude"].as_f64().context("geocoding: no longitude")?,
        name: format!(
            "{}, {}",
            r["name"].as_str().unwrap_or(name),
            r["country"].as_str().unwrap_or("")
        ),
    })
}

fn wmo_to_weather(c: i64) -> &'static str {
    match c {
        0 | 1 => "clear",
        2 => "partly_cloudy",
        3 => "overcast",
        45..=48 => "fog",
        51..=65 | 80..=82 => "rain",
        71..=77 | 85..=86 => "snow",
        95.. => "storm",
        _ => "overcast",
    }
}

fn season(month: u32, lat: f64) -> &'static str {
    let north = match month {
        12 | 1 | 2 => "winter",
        3..=5 => "spring",
        6..=8 => "summer",
        _ => "autumn",
    };
    if lat >= 0.0 {
        return north;
    }
    match north {
        "winter" => "summer",
        "spring" => "autumn",
        "summer" => "winter",
        _ => "spring",
    }
}

fn time_bucket(m: i64, sr: Option<i64>, ss: Option<i64>) -> &'static str {
    let (Some(sr), Some(ss)) = (sr, ss) else {
        return match m / 60 {
            5..=6 => "dawn",
            7..=10 => "morning",
            11..=12 => "noon",
            13..=16 => "afternoon",
            17..=19 => "dusk",
            20..=22 => "evening",
            _ => "night",
        };
    };
    if m >= sr - 30 && m < sr + 30 {
        return "dawn";
    }
    if m >= ss - 30 && m < ss + 30 {
        return "dusk";
    }
    if m >= sr && m < ss {
        let frac = (m - sr) as f64 / (ss - sr) as f64;
        return if frac < 0.45 {
            "morning"
        } else if frac < 0.55 {
            "noon"
        } else {
            "afternoon"
        };
    }
    if m >= ss + 30 && m < ss + 120 {
        return "evening";
    }
    "night"
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn year_from_days(z: i64) -> i64 {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let y = yoe + era * 400;
    if mp >= 10 { y + 1 } else { y }
}

/// NOAA approximation. Returns (elevation in degrees, sun rising).
fn solar_position(lat: f64, lon: f64, unix: i64) -> (f64, bool) {
    let days = unix.div_euclid(86400);
    let doy = days - days_from_civil(year_from_days(days), 1, 1) + 1;
    let frac_hour = unix.rem_euclid(86400) as f64 / 3600.0;
    let g = (2.0 * PI / 365.0) * (doy as f64 - 1.0 + (frac_hour - 12.0) / 24.0);
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos()
            - 0.032077 * g.sin()
            - 0.014615 * (2.0 * g).cos()
            - 0.040849 * (2.0 * g).sin());
    let true_solar_min = frac_hour * 60.0 + eqtime + 4.0 * lon;
    let hour_angle = (((true_solar_min / 4.0 - 180.0) % 360.0) + 540.0) % 360.0 - 180.0;
    let (lat_r, ha_r) = (lat.to_radians(), hour_angle.to_radians());
    let elev = (lat_r.sin() * decl.sin() + lat_r.cos() * decl.cos() * ha_r.cos())
        .asin()
        .to_degrees();
    (elev, hour_angle < 0.0)
}

fn light_level(elev: f64, rising: bool) -> &'static str {
    match elev {
        e if e >= 6.0 => "bright",
        e if e >= -4.0 => if rising { "golden_dawn" } else { "golden_dusk" },
        e if e >= -10.0 => if rising { "pre_dawn" } else { "twilight" },
        _ => "night",
    }
}

fn parse_hm(iso: &str) -> Option<i64> {
    let (_, t) = iso.split_once('T')?;
    let (h, m) = t.split_once(':')?;
    Some(h.parse::<i64>().ok()? * 60 + m.get(..2)?.parse::<i64>().ok()?)
}

pub fn conditions(loc: Location) -> Result<Ctx> {
    let (lat, lon) = (loc.lat.to_string(), loc.lon.to_string());
    let v = net::json(
        "https://api.open-meteo.com/v1/forecast",
        &[
            ("latitude", &lat),
            ("longitude", &lon),
            ("current", "temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m"),
            ("daily", "sunrise,sunset"),
            ("timezone", "auto"),
            ("forecast_days", "1"),
        ],
    )?;
    let cur = &v["current"];
    let now = cur["time"].as_str().context("open-meteo: no current.time")?;
    let month: u32 = now.get(5..7).and_then(|s| s.parse().ok()).context("bad current.time")?;
    let minutes = parse_hm(now).context("bad current.time")?;
    let sunrise = v["daily"]["sunrise"][0].as_str().and_then(parse_hm);
    let mut sunset = v["daily"]["sunset"][0].as_str().and_then(parse_hm);
    // Sunset can land after midnight at high latitudes.
    if let (Some(r), Some(s)) = (sunrise, sunset)
        && s < r
    {
        sunset = Some(s + 1440);
    }

    let unix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let (elev, rising) = solar_position(loc.lat, loc.lon, unix);
    let light = light_level(elev, rising);
    let mut time = time_bucket(minutes, sunrise, sunset);
    if time == "night" && elev > -6.0 {
        time = if rising { "dawn" } else { "dusk" };
    }

    let temp = cur["temperature_2m"].as_f64().unwrap_or(15.0).round() as i64;
    let humidity = cur["relative_humidity_2m"].as_f64().unwrap_or(50.0).round() as i64;
    let wind = cur["wind_speed_10m"].as_f64().unwrap_or(0.0).round() as i64;
    let weather_code = cur["weather_code"].as_i64().unwrap_or(3);
    let weather = wmo_to_weather(weather_code);
    let season = season(month, loc.lat);

    let mut temp_tags = match temp {
        t if t > 28 => vec!["hot", "warm"],
        t if t > 18 => vec!["warm", "mild"],
        t if t > 8 => vec!["mild"],
        t if t > 0 => vec!["cold"],
        _ => vec!["cold", "freezing"],
    };
    if humidity > 80 {
        temp_tags.push("humid");
    }
    if humidity < 35 {
        temp_tags.push("dry");
    }
    let wind_tags = match wind {
        w if w > 40 => vec!["windy", "restless"],
        w if w > 18 => vec!["breezy", "restless"],
        _ => vec!["still", "calm"],
    };
    let mut light_tags = match (weather, time) {
        ("clear", "dawn") => vec!["golden", "soft", "misty", "peaceful"],
        ("clear", "morning") => vec!["bright", "golden", "fresh"],
        ("clear", "noon") => vec!["harsh", "bright", "hot"],
        ("clear", "afternoon") => vec!["golden", "warm", "soft"],
        ("clear", "dusk") => vec!["golden", "warm", "dramatic"],
        ("clear", "evening") => vec!["warm", "soft", "dim"],
        ("clear", _) => vec!["cool", "still", "moonlit"],
        ("partly_cloudy", "dawn" | "dusk") => vec!["golden", "soft"],
        ("partly_cloudy", "noon") => vec!["bright", "soft"],
        ("partly_cloudy", _) => vec!["soft", "diffuse"],
        ("overcast", _) => vec!["diffuse", "soft", "cool", "somber"],
        ("fog", _) => vec!["hazy", "soft", "muted", "still", "somber", "isolated"],
        ("rain", _) => vec!["dark", "muted", "restless", "somber", "cool"],
        ("snow", _) => vec!["soft", "cold", "still", "white", "bright"],
        _ => vec!["dark", "dramatic", "restless", "wild"],
    };
    if weather == "overcast" && temp < 5 {
        light_tags.push("cold");
    }
    let season_tags = match season {
        "winter" => vec!["cold", "still", "somber", "isolated"],
        "summer" => vec!["warm", "lush", "bright", "peaceful"],
        "autumn" => vec!["golden", "somber", "cool", "restless"],
        _ => vec!["soft", "peaceful", "fresh", "mild"],
    };

    Ok(Ctx {
        place: loc.name,
        lat: loc.lat,
        lon: loc.lon,
        temp,
        weather_code,
        weather,
        season,
        time,
        light,
        temp_tags,
        wind_tags,
        light_tags,
        season_tags,
    })
}
