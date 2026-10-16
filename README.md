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

Defaults: 3840px wide; rejects images under half that width and portrait images (`--min-width`, `--allow-portrait`).
`--source met|artic|cleveland|wikimedia` limits sources, `--curated-only` skips them. `--set-ext` fixes the output
extension to the downloaded format. Last 20 picks are remembered in `~/.local/state/plein-air/recent.json`.

Set `PLEIN_AIR_CONTACT` to an email or project URL. It goes in the User-Agent, and Wikimedia answers 429 to clients
without contact details (its robot policy), which breaks the curated fallback. The plugin has a **Contact** setting
for it.

Wrapper env: `PLEIN_AIR_CONTACT`, `PLEIN_AIR_DIR`, `PLEIN_AIR_PALETTE` (`none` to skip recoloring), `PLEIN_AIR_LUTGEN_ARGS`.

Noctalia ignores `wallpaper-set` for an unchanged path, so the wrapper alternates between two symlinks to the image.

## License

MIT, see [LICENSE](LICENSE).

## Known issues

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
  [Wikimedia Commons](https://commons.wikimedia.org).
- Rust crates: anyhow, clap, fastrand, imagesize, regex, serde, serde_json, ureq.
