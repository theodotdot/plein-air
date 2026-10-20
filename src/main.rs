mod ctx;
mod net;
mod select;
mod sources;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde_json::json;

use crate::{
    ctx::{Location, conditions, locate_city, locate_ip, parse_coords},
    net::ImgOpts,
    select::{Painting, curated_order, validate},
};

/// Download a public-domain painting matching the weather, season and time of day where you are.
/// Port of https://art.joonas.wtf. Prints metadata as JSON on stdout.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// City name (e.g. "Paris"). Default: locate via IP.
    #[arg(long, conflicts_with = "coords")]
    city: Option<String>,
    /// Coordinates as LAT,LON (e.g. 48.85,2.35).
    #[arg(long, allow_hyphen_values = true)]
    coords: Option<String>,
    /// Output file; replaced atomically if it exists.
    #[arg(short, long, default_value = "plein-air.jpg")]
    output: PathBuf,
    /// Replace the output's extension with the downloaded format (jpeg, png, webp).
    #[arg(long)]
    set_ext: bool,
    /// Requested image width in px.
    #[arg(long, default_value_t = 3840)]
    width: u32,
    /// Minimum usable width after 16:9 fill. Default: width / 2.
    #[arg(long)]
    min_width: Option<u32>,
    /// Allow portrait-oriented paintings.
    #[arg(long)]
    allow_portrait: bool,
    /// Maximum image area lost to 16:9 fill, in percent.
    #[arg(long, default_value_t = 35, value_parser = clap::value_parser!(u8).range(0..=100))]
    max_crop_percent: u8,
    /// Restrict external sources. Repeatable.
    #[arg(long = "source", value_parser = ["met", "artic", "cleveland", "wikimedia", "smk", "rijksmuseum", "paris"])]
    sources: Vec<String>,
    /// Skip external sources; use only the curated set.
    #[arg(long)]
    curated_only: bool,
    /// Pick a painting but don't download it.
    #[arg(long)]
    dry_run: bool,
    /// Log rejected candidates to stderr.
    #[arg(short, long)]
    verbose: bool,
}

fn state_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("plein-air/recent.json"))
}

fn load_recent() -> Vec<String> {
    state_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn push_recent(mut recent: Vec<String>, key: String) -> Result<()> {
    let Some(path) = state_path() else { return Ok(()) };
    recent.retain(|k| *k != key);
    recent.push(key);
    let skip = recent.len().saturating_sub(20);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, serde_json::to_string(&recent[skip..])?)?;
    Ok(())
}

fn write_atomic(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let log = |msg: String| {
        if args.verbose {
            eprintln!("{msg}");
        }
    };

    let loc = match (&args.city, &args.coords) {
        (Some(c), _) => locate_city(c)?,
        (_, Some(c)) => {
            let (lat, lon) = parse_coords(c)?;
            Location { lat, lon, name: String::new() }
        }
        _ => locate_ip().context("IP geolocation failed; pass --city or --coords")?,
    };
    let ctx = conditions(loc)?;
    log(format!("context: {}", serde_json::to_string(&ctx)?));

    let opts = ImgOpts {
        width: args.width,
        min_width: args.min_width.unwrap_or(args.width / 2),
        allow_portrait: args.allow_portrait,
        max_crop_percent: args.max_crop_percent,
    };
    let recent = load_recent();

    // Try one candidate: final gate, then download (which checks size/orientation).
    let attempt = |p: &Painting, allow_recent: bool| -> Option<Vec<u8>> {
        if !allow_recent && recent.contains(&p.key()) {
            log(format!("skip recent: {} — {}", p.artist, p.title));
            return None;
        }
        if let Err(why) = validate(p, &ctx) {
            log(format!("reject {} — {}: {why}", p.artist, p.title));
            return None;
        }
        if args.dry_run {
            return Some(Vec::new());
        }
        net::image(&p.image, &opts)
            .inspect_err(|e| log(format!("download failed {} — {}: {e:#}", p.artist, p.title)))
            .ok()
    };

    let mut chosen: Option<(Painting, Vec<u8>)> = None;
    if !args.curated_only {
        'sources: for (name, fetch) in sources::source_order(&args.sources) {
            let cands = match fetch(&ctx, &opts) {
                Ok(c) => c,
                Err(e) => {
                    log(format!("{name}: {e:#}"));
                    continue;
                }
            };
            log(format!("{name}: {} candidates", cands.len()));
            for p in cands {
                if let Some(bytes) = attempt(&p, false) {
                    chosen = Some((p, bytes));
                    break 'sources;
                }
            }
        }
    }
    if chosen.is_none() {
        log("falling back to curated set".into());
        let mut list = sources::curated_list()?;
        for p in &mut list {
            p.image = sources::wikimedia_thumb(&p.image, args.width);
        }
        // Curated scoring already penalizes recent picks, so allow them as a last resort.
        // Capped so a rate-limited host doesn't get hit 156 times.
        let order: Vec<_> = curated_order(&list, &ctx, &[]).into_iter().take(8).collect();
        chosen = order.iter().find_map(|p| attempt(p, false).map(|b| (p.clone(), b)));
        if chosen.is_none() {
            chosen = order.into_iter().take(3).find_map(|p| {
                attempt(&p, true).map(|b| (p, b))
            });
        }
    }
    let Some((p, bytes)) = chosen else { bail!("no painting found") };

    let mut out = args.output.clone();
    if !args.dry_run {
        if args.set_ext {
            out.set_extension(match imagesize::image_type(&bytes)? {
                imagesize::ImageType::Png => "png",
                imagesize::ImageType::Webp => "webp",
                _ => "jpeg",
            });
        }
        write_atomic(&out, &bytes)?;
        push_recent(recent, p.key())?;
    }
    println!(
        "{}",
        json!({
            "artist": p.artist,
            "title": p.title,
            "year": p.year,
            "source": p.source,
            "image_url": p.image,
            "path": if args.dry_run { None } else { Some(fs::canonicalize(&out)?) },
            "depicts": p.depicts,
            "context": ctx,
        })
    );
    Ok(())
}
