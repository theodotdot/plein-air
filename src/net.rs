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

pub fn paris_graphql(query: &str, token: &str) -> Result<Value> {
    let response = AGENT
        .post("https://apicollections.parismusees.paris.fr/graphql")
        .query("_format", "json")
        .set("Accept", "application/json")
        .set("auth-token", token)
        .send_json(serde_json::json!({ "query": query }));
    let resp = match response {
        Ok(resp) => resp,
        Err(ureq::Error::Status(status, resp)) => {
            let body: Value = resp.into_json()
                .with_context(|| format!("Paris Musees HTTP {status}: unreadable error response"))?;
            let message = body["message"].as_str()
                .with_context(|| format!("Paris Musees HTTP {status}: {body}"))?;
            bail!("Paris Musees HTTP {status}: {message}");
        }
        Err(e) => return Err(e).context("Paris Musees GraphQL request failed"),
    };
    let v: Value = resp.into_json()?;
    if let Some(errors) = v["errors"].as_array().filter(|e| !e.is_empty()) {
        bail!("Paris Musees GraphQL errors: {}", Value::Array(errors.clone()));
    }
    Ok(v)
}

pub struct ImgOpts {
    pub width: u32,
    pub min_width: u32,
    pub allow_portrait: bool,
    pub max_crop_percent: u8,
}

impl ImgOpts {
    /// Pre-download check for sources that report original dimensions.
    pub fn fits(&self, w: u64, h: u64) -> bool {
        if w == 0 || h == 0 || (!self.allow_portrait && w < h) {
            return false;
        }
        let (w9, h16) = (u128::from(w) * 9, u128::from(h) * 16);
        w >= u64::from(self.min_width)
            && h16 >= u128::from(self.min_width) * 9
            && w9.min(h16) * 100 >= w9.max(h16) * u128::from(100 - self.max_crop_percent)
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
        bail!(
            "rejected {w}x{h} (min width after 16:9 fill {}, max crop {}%, portrait allowed: {})",
            opts.min_width, opts.max_crop_percent, opts.allow_portrait
        );
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ImgOpts {
        ImgOpts { width: 3840, min_width: 1920, allow_portrait: false, max_crop_percent: 35 }
    }

    #[test]
    fn crop_boundary_is_inclusive_on_both_sides_of_16_9() {
        let opts = opts();
        assert!(opts.fits(2080, 1800));
        assert!(!opts.fits(2079, 1800));
        assert!(opts.fits(6400, 2340));
        assert!(!opts.fits(6401, 2340));
    }

    #[test]
    fn rejects_squares_scrolls_small_and_zero_images() {
        let opts = opts();
        assert!(opts.fits(3840, 2160));
        assert!(opts.fits(1920, 1080));
        assert!(opts.fits(3400, 1262));
        assert!(!opts.fits(2000, 2000));
        assert!(!opts.fits(6000, 1000));
        assert!(!opts.fits(1920, 1000));
        assert!(!opts.fits(0, 1080));
        assert!(!opts.fits(3840, 0));
    }

    #[test]
    fn portrait_permission_does_not_bypass_crop_limit() {
        let mut opts = opts();
        opts.allow_portrait = true;
        assert!(!opts.fits(2160, 3840));
        opts.max_crop_percent = 70;
        assert!(opts.fits(2160, 3840));
        opts.allow_portrait = false;
        assert!(!opts.fits(2160, 3840));
    }
}
