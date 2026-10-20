# plein-air

Wallpaper from public-domain paintings that match the weather, season and time of day where you are. Optionally
recolored to a palette (default Catppuccin Macchiato). Includes a [Noctalia](https://noctalia.dev) plugin for refreshing
from the bar or settings.

Port of the matching logic behind [art.joonas.wtf](https://art.joonas.wtf), which has no API.

## Parts

- `plein-air`: Rust binary. Picks and downloads a painting, prints metadata as JSON.
- `plein-air-wallpaper.sh`: runs the binary, recolors with lutgen, sets the Noctalia wallpaper.
- `noctalia-plugin/plein-air/`: Noctalia plugin (bar widget, control-center tile, background refresh).

## Install

```sh
cargo install --path . --root ~/.local
ln -s "$PWD/plein-air-wallpaper.sh" ~/.local/bin/plein-air-wallpaper
cargo install lutgen-cli --root ~/.local
```

Needs `jq` and `noctalia` on `PATH`. lutgen is optional: recoloring is on by default and switches itself off when the
`lutgen` binary is missing (the plugin panel says so).

Noctalia plugin:

```sh
ln -s "$PWD/noctalia-plugin/plein-air" ~/.local/share/noctalia/plugins/plein-air
noctalia msg plugins enable theodotdot/plein-air
```

Bar widget, in `config.toml` (left click opens a panel with the current painting, **Refresh now** and **Settings**
buttons; right click refreshes):

```toml
[widget.plein_air]
type = "theodotdot/plein-air:widget"
```

## Usage

```sh
plein-air -o wallpaper                  # location from IP
plein-air --city Paris -o wallpaper
plein-air --coords 48.85,2.35 -o wallpaper
plein-air --width 2560 --dry-run -v     # show the pick and why, no download
```

Defaults: 3840px wide; rejects images with less than 1920px usable width after 16:9 fill (`--min-width`).
Images must lose at most **35%** of their area to 16:9 fill (`--max-crop-percent`, also in the plugin's advanced
settings). Portrait images need both `--allow-portrait` and a higher crop limit. These checks also apply to fallback
downloads. Dry runs skip downloads, so sources without image dimensions aren't checked for size or crop loss.
Works titled as calligraphy, sutras, scriptures or manuscripts are excluded; inscriptions alone aren't.

`--source` limits external sources and accepts repeated values. `--curated-only` skips external sources.
`--set-ext` fixes the output extension to the downloaded format.
Last 20 picks are remembered in `~/.local/state/plein-air/recent.json`.

Set `PLEIN_AIR_CONTACT` to an email or project URL. It goes in the User-Agent, and Wikimedia answers 429 to clients
without contact details (its robot policy), which breaks the curated fallback. The plugin has a **Contact** setting
for it.

Wrapper env: `PLEIN_AIR_CONTACT`, `PLEIN_AIR_DIR`, `PLEIN_AIR_PALETTE` (`none` to skip recoloring), `PLEIN_AIR_LUTGEN_ARGS`.

Noctalia ignores `wallpaper-set` for an unchanged path, so the wrapper alternates between two symlinks to the image.

## Sources

| Source | `--source` | API key |
|--------|------------|---------|
| [The Metropolitan Museum of Art](https://metmuseum.github.io/) | `met` | None |
| [Art Institute of Chicago](https://api.artic.edu/docs/) | `artic` | None |
| [Cleveland Museum of Art](https://openaccess-api.clevelandart.org/) | `cleveland` | None |
| [Wikimedia Commons](https://commons.wikimedia.org/w/api.php) | `wikimedia` | None; contact details required in the User-Agent |
| [SMK — National Gallery of Denmark](https://www.smk.dk/en/article/smk-api/) | `smk` | None |
| [Rijksmuseum](https://data.rijksmuseum.nl/docs/) | `rijksmuseum` | None |
| [Paris Musées](https://apicollections.parismusees.paris.fr/) | `paris` | Optional; [get a token](https://apicollections.parismusees.paris.fr/) |

No API key is required to use the app or plugin. Without a Paris token, default selection skips Paris
and keeps the other sources enabled. Source errors don't stop selection; the app tries the next source.

To enable Paris, export `PARIS_MUSEES_API` in the environment inherited by `plein-air` or Noctalia.
The app doesn't load `.env` files. Niri's `environment` block applies to new processes spawned by niri,
not systemd user services. Restart Noctalia after changing its environment.

The new museum sources check image reuse rights, not just metadata rights. French, Danish and Dutch scene
terms are mapped to the English weather and season matcher.

## License

MIT, see [LICENSE](LICENSE).

## Known issues

- Paris Musees requires the account/token permission `execute graphql requests`. Without it, the API returns
  HTTP 403 even with an Auth Tokens token; `-v` shows the server's permission error.
- Art Institute of Chicago image downloads are blocked by Cloudflare for non-browser clients, so that source is
  effectively skipped.
- Without `PLEIN_AIR_CONTACT`, Wikimedia returns 429, so the curated fallback and the Wikimedia source fail.
- Most Wikimedia Commons categories the site uses return no files, so that source rarely yields anything.
- The Met and Cleveland APIs match every search term, so long queries find little; shorter fallbacks are tried.
- Met returns 403 on request bursts; fetches are limited to 6 at a time.

## Credits

- [Plein Air](https://art.joonas.wtf) by joonas: the idea, the scoring and validation rules (ported from its page
  source), and the curated painting list, which is fetched from the site at runtime rather than copied here.
- [lutgen-rs](https://github.com/ozwaldorf/lutgen-rs) by ozwaldorf: palette recoloring.
- [Catppuccin](https://github.com/catppuccin/catppuccin): the Macchiato palette.
- [Noctalia](https://github.com/noctalia-dev/noctalia): the shell and plugin API.
- Weather and geocoding: [Open-Meteo](https://open-meteo.com), data under
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). IP geolocation: [ipinfo.io](https://ipinfo.io).
- Paintings, public domain / open access:
  [The Metropolitan Museum of Art](https://www.metmuseum.org/about-the-met/policies-and-documents/open-access),
  [Art Institute of Chicago](https://api.artic.edu/docs/),
  [Cleveland Museum of Art](https://openaccess-api.clevelandart.org/),
  [SMK](https://www.smk.dk/en/article/smk-api/),
  [Rijksmuseum](https://data.rijksmuseum.nl/docs/),
  [Paris Musees](https://apicollections.parismusees.paris.fr/),
  [Wikimedia Commons](https://commons.wikimedia.org).
- Rust crates: anyhow, clap, fastrand, imagesize, regex, serde, serde_json, ureq.
