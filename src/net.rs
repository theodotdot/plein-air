use std::{io::Read, sync::LazyLock, time::Duration};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Wikimedia's robot policy asks for contact details in the User-Agent. Set PLEIN_AIR_CONTACT
/// to an email or project URL; without it Wikimedia may answer 429.
fn user_agent() -> String {
    match std::env::var("PLEIN_AIR_CONTACT") {
        Ok(c) if !c.trim().is_empty() => format!("plein-air/0.1 (personal wallpaper tool; {})", c.trim()),
        _ => "plein-air/0.1 (personal wallpaper tool)".to_string(),
    }
}

static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::AgentBuilder::new()
        .user_agent(&user_agent())
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
});

// Wikimedia answers 429 to requests that carry no Accept header.
const ACCEPT: &str = "*/*";

pub fn json(url: &str, query: &[(&str, &str)]) -> Result<Value> {
    let mut req = AGENT.get(url).set("Accept", ACCEPT);
    for (k, v) in query {
        req = req.query(k, v);
    }
    let resp = req.call().with_context(|| format!("GET {url}"))?;
    Ok(resp.into_json()?)
}

pub fn text(url: &str) -> Result<String> {
    Ok(AGENT.get(url).set("Accept", ACCEPT).call().with_context(|| format!("GET {url}"))?.into_string()?)
}

pub struct ImgOpts {
    pub width: u32,
    pub min_width: u32,
    pub allow_portrait: bool,
}

impl ImgOpts {
    /// Pre-download check for sources that report original dimensions.
    pub fn fits(&self, w: u64, h: u64) -> bool {
        w >= self.min_width as u64 && (self.allow_portrait || w >= h)
    }
}

pub fn image(url: &str, opts: &ImgOpts) -> Result<Vec<u8>> {
    let resp = AGENT
        .get(url)
        .set("Accept", ACCEPT)
        .timeout(Duration::from_secs(120))
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader().take(150 << 20).read_to_end(&mut buf)?;
    use imagesize::ImageType::*;
    match imagesize::image_type(&buf) {
        Ok(Jpeg | Png | Webp) => {}
        Ok(t) => bail!("unsupported image format {t:?}"),
        Err(_) => bail!("response is not an image"),
    }
    let size = imagesize::blob_size(&buf)?;
    let (w, h) = (size.width as u64, size.height as u64);
    if !opts.fits(w, h) {
        bail!("rejected {w}x{h} (min width {}, portrait allowed: {})", opts.min_width, opts.allow_portrait);
    }
    Ok(buf)
}
