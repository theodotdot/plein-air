use anyhow::{Context, Result};
use serde_json::Value;

use super::{ext_painting, join_strs, public_domain_rights, s};
use crate::{
    ctx::Ctx,
    net::{self, ImgOpts},
    select::{Candidate, Painting, select_by_content},
};

fn museum_queries(ctx: &Ctx, language: &str) -> Vec<&'static str> {
    let key = match (language, ctx.weather, ctx.time, ctx.season) {
        ("fr", _, "night", _) => "lune",
        ("fr", _, "dawn", _) => "lever du soleil",
        ("fr", _, "dusk" | "evening", _) => "coucher du soleil",
        ("fr", "snow", _, _) => "neige",
        ("fr", "rain", _, _) => "pluie",
        ("fr", "storm", _, _) => "tempête",
        ("fr", "fog", _, _) => "brouillard",
        ("fr", _, _, "autumn") => "automne",
        ("fr", _, _, "winter") => "hiver",
        ("fr", _, _, "spring") => "printemps",
        ("fr", _, _, _) => "été",
        ("da", _, "night", _) => "måneskin",
        ("da", _, "dawn", _) => "solopgang",
        ("da", _, "dusk" | "evening", _) => "aften",
        ("da", "snow", _, _) => "sne",
        ("da", "rain", _, _) => "regn",
        ("da", "storm", _, _) => "storm",
        ("da", "fog", _, _) => "tåge",
        ("da", _, _, "autumn") => "efterår",
        ("da", _, _, "winter") => "vinter",
        ("da", _, _, "spring") => "forår",
        ("da", _, _, _) => "sommer",
        (_, _, "night", _) => "moonlight",
        (_, _, "dawn", _) => "sunrise",
        (_, _, "dusk" | "evening", _) => "sunset",
        (_, "snow", _, _) => "snow",
        (_, "rain", _, _) => "rain",
        (_, "storm", _, _) => "storm",
        (_, "fog", _, _) => "mist",
        (_, _, _, season) => season,
    };
    let landscape = match language {
        "fr" => "paysage",
        "da" => "landskab",
        _ => "landscape",
    };
    vec![key, landscape]
}

fn smk_record(o: &Value, opts: &ImgOpts) -> Option<Candidate> {
    if o["public_domain"] != true
        || !public_domain_rights(&s(&o["rights"]))
        || !o["object_names"].as_array()?.iter().any(|n| n["name"] == "Maleri")
    {
        return None;
    }
    let (w, h) = (o["image_width"].as_u64()?, o["image_height"].as_u64()?);
    let base = o["image_iiif_id"].as_str().filter(|s| !s.is_empty())?;
    if !opts.fits(w, h) {
        return None;
    }
    let titles = o["titles"].as_array()?;
    let title = titles
        .iter()
        .find(|t| s(&t["language"]).to_lowercase().starts_with("eng"))
        .or_else(|| titles.first())?["title"].as_str()?;
    let content = [
        titles.iter().map(|t| s(&t["title"])).collect::<Vec<_>>().join(" "),
        join_strs(&o["content_subject"]),
        join_strs(&o["tags"]),
        join_strs(&o["techniques"]),
    ].join(" ");
    let artists = o["production"].as_array().map(|p| p.iter().map(|p| s(&p["creator"])).collect::<Vec<_>>().join("; ")).unwrap_or_default();
    Some(ext_painting(
        title,
        &artists,
        s(&o["production_date"][0]["period"]),
        format!("{base}/full/{},/0/default.jpg", u64::from(opts.width).min(w)),
        content,
        "smk",
        format!("smk:{}", o["object_number"].as_str()?),
    ))
}

pub fn smk(ctx: &Ctx, opts: &ImgOpts) -> Result<Vec<Painting>> {
    for query in museum_queries(ctx, "da") {
        let v = net::json(
            "https://api.smk.dk/api/v1/art/search/",
            &[
                ("keys", query),
                ("filters", "[has_image:true],[public_domain:true],[object_names:Maleri],[image_hq:true]"),
                ("rows", "100"),
                ("randomHighlights", &fastrand::alphabetic().to_string()),
            ],
        )?;
        let cands = v["items"].as_array().context("smk: no items")?.iter().filter_map(|o| smk_record(o, opts)).collect();
        let selected = select_by_content(cands, ctx);
        if !selected.is_empty() {
            return Ok(selected);
        }
    }
    Ok(Vec::new())
}

fn linked_text(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(linked_text).collect::<Vec<_>>().join(" "),
        Value::Object(o) => {
            let mut parts: Vec<String> = ["content", "@value", "_label"]
                .iter().filter_map(|k| o.get(*k).and_then(Value::as_str).map(str::to_string)).collect();
            for key in ["identified_by", "notation", "part", "referred_to_by"] {
                if let Some(child) = o.get(key) {
                    parts.push(linked_text(child));
                }
            }
            parts.join(" ")
        }
        _ => String::new(),
    }
}

fn linked_name(v: &Value) -> String {
    let Some(names) = v.as_array() else { return String::new() };
    let english = |n: &&Value| {
        n["language"].as_array().is_some_and(|l| l.iter().any(|l| l["id"] == "http://vocab.getty.edu/aat/300388277"))
    };
    names.iter().filter(|n| n["type"] == "Name").find(english)
        .or_else(|| names.iter().find(|n| n["type"] == "Name"))
        .map(|n| s(&n["content"])).unwrap_or_default()
}

fn rijks_record(o: &Value) -> Option<Candidate> {
    let image = o["subject_of"].as_array()?.iter()
        .flat_map(|s| s["digitally_carried_by"].as_array().into_iter().flatten())
        .filter(|d| d["referred_to_by"].as_array().is_some_and(|a| a.iter().any(|r| public_domain_rights(&s(&r["content"])))))
        .flat_map(|d| d["access_point"].as_array().into_iter().flatten())
        .find_map(|a| a["id"].as_str().filter(|u| u.starts_with("https://iiif.micr.io/") && u.contains("/full/")))?;
    let title = linked_name(&o["identified_by"]);
    let production = &o["produced_by"];
    let artists: Vec<_> = std::iter::once(production)
        .chain(production["part"].as_array().into_iter().flatten())
        .flat_map(|p| p["carried_out_by"].as_array().into_iter().flatten())
        .filter_map(|p| {
            let names = p["notation"].as_array()?;
            names.iter().find(|n| n["@language"] == "en").or_else(|| names.first())?["@value"].as_str()
        })
        .collect();
    let artist = if artists.is_empty() {
        let notes = production["referred_to_by"].as_array();
        notes.and_then(|notes| notes.iter().find(|n| {
            n["language"].as_array().is_some_and(|l| l.iter().any(|l| l["id"] == "http://vocab.getty.edu/aat/300388277"))
        }).or_else(|| notes.first())).map(|n| s(&n["content"])).unwrap_or_default()
    } else {
        artists.join("; ")
    };
    let year = linked_name(&o["produced_by"]["timespan"]["identified_by"]);
    let descriptions: Vec<_> = o["referred_to_by"].as_array().into_iter().flatten()
        .filter(|r| r["classified_as"].as_array().is_some_and(|c| c.iter().any(|c| {
            matches!(c["id"].as_str(), Some("http://vocab.getty.edu/aat/300435452" | "http://vocab.getty.edu/aat/300435429"))
        })))
        .map(linked_text).collect();
    let content = format!("{} {} {}", linked_text(&o["identified_by"]), descriptions.join(" "), linked_text(&o["subject_of"]));
    Some(ext_painting(&title, &artist, year, image.to_string(), content, "rijksmuseum", s(&o["id"])))
}

fn rijks_image(p: &mut Painting, opts: &ImgOpts) -> Result<bool> {
    let (base, _) = p.image.split_once("/full/").context("rijksmuseum: invalid IIIF image URL")?;
    let info = net::json(&format!("{base}/info.json"), &[])?;
    let w = info["width"].as_u64().context("rijksmuseum: missing image width")?;
    let h = info["height"].as_u64().context("rijksmuseum: missing image height")?;
    if !opts.fits(w, h) {
        return Ok(false);
    }
    let mut width = u64::from(opts.width).min(w);
    if let Some(area) = info["maxArea"].as_u64() {
        width = width.min((area as f64 * w as f64 / h as f64).sqrt().floor() as u64);
    }
    p.image = format!("{base}/full/{width},/0/default.jpg");
    Ok(true)
}

pub fn rijksmuseum(ctx: &Ctx, opts: &ImgOpts) -> Result<Vec<Painting>> {
    for query in museum_queries(ctx, "en") {
        let v = net::json(
            "https://data.rijksmuseum.nl/search/collection",
            &[("type", "painting"), ("imageAvailable", "true"), ("description", query)],
        )?;
        let mut ids = v["orderedItems"].as_array().context("rijksmuseum: no orderedItems")?.clone();
        fastrand::shuffle(&mut ids);
        let mut cands = Vec::new();
        for id in ids.iter().take(20) {
            let id = id["id"].as_str().context("rijksmuseum: missing object id")?;
            let o = net::json(id, &[("_profile", "la-framed")])?;
            if let Some(c) = rijks_record(&o) {
                cands.push(c);
            }
        }
        let mut out = Vec::new();
        for mut p in select_by_content(cands, ctx) {
            if rijks_image(&mut p, opts)? {
                out.push(p);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Ok(Vec::new())
}

fn paris_query(term: &str, offset: usize) -> String {
    let pattern = serde_json::to_string(&format!("%{term}%")).expect("string serialization");
    format!(
        r#"{{
          nodeQuery(offset: {offset}, limit: 50, filter: {{conditions: [
            {{field: "type", value: "oeuvre"}},
            {{field: "field_oeuvre_types_objet.entity.field_lref_adlib", value: "4284"}},
            {{field: "field_visuels.entity.field_image_libre", value: "1"}},
            {{field: "title", value: {pattern}, operator: LIKE}}
          ]}}) {{
            count
            entities {{
              ... on NodeOeuvre {{
                entityId title absolutePath
                fieldDateProduction {{ startYear endYear }}
                fieldOeuvreAuteurs {{ entity {{ fieldAuteurAuteur {{ entity {{ name }} }} }} }}
                fieldOeuvreThemeRepresente {{ entity {{ name }} }}
                fieldMateriauxTechnique {{ entity {{ name }} }}
                fieldCommentaireHistorique {{ value }}
                fieldVisuels {{ entity {{ publicUrl fieldImageLibre }} }}
              }}
            }}
          }}
        }}"#
    )
}

fn paris_record(o: &Value) -> Option<Candidate> {
    let url = o["fieldVisuels"].as_array()?.iter().map(|v| &v["entity"])
        .filter(|v| matches!(v["fieldImageLibre"], Value::Bool(true)) || v["fieldImageLibre"] == "1" || v["fieldImageLibre"] == 1)
        .find_map(|v| v["publicUrl"].as_str().filter(|u| u.starts_with("https://") || u.starts_with("http://")))?;
    let title = o["title"].as_str()?;
    let names = |field: &str| o[field].as_array().into_iter().flatten().map(|v| s(&v["entity"]["name"])).collect::<Vec<_>>().join(" ");
    let artist = o["fieldOeuvreAuteurs"].as_array().into_iter().flatten()
        .map(|v| s(&v["entity"]["fieldAuteurAuteur"]["entity"]["name"])).collect::<Vec<_>>().join("; ");
    let year = &o["fieldDateProduction"]["startYear"];
    let year = year.as_str().map(str::to_string).or_else(|| year.as_i64().map(|y| y.to_string())).unwrap_or_default();
    let content = format!("{title} {} {} {}", names("fieldOeuvreThemeRepresente"), names("fieldMateriauxTechnique"), s(&o["fieldCommentaireHistorique"]["value"]));
    let id = o["entityId"].as_str().or_else(|| o["absolutePath"].as_str())?;
    Some(ext_painting(title, &artist, year, url.to_string(), content, "paris", format!("paris:{id}")))
}

pub fn paris(ctx: &Ctx, _opts: &ImgOpts) -> Result<Vec<Painting>> {
    let token = std::env::var("PARIS_MUSEES_API").ok().filter(|v| !v.trim().is_empty())
        .context("paris: set PARIS_MUSEES_API in the environment inherited by plein-air / Noctalia")?;
    for term in museum_queries(ctx, "fr") {
        let mut v = net::paris_graphql(&paris_query(term, 0), token.trim())?;
        let count = v["data"]["nodeQuery"]["count"].as_u64().context("paris: no nodeQuery count")? as usize;
        if count > 50 {
            v = net::paris_graphql(&paris_query(term, fastrand::usize(0..=count - 50)), token.trim())?;
        }
        let cands = v["data"]["nodeQuery"]["entities"].as_array().context("paris: no entities")?
            .iter().filter_map(paris_record).collect();
        let selected = select_by_content(cands, ctx);
        if !selected.is_empty() {
            return Ok(selected);
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opts() -> ImgOpts {
        ImgOpts { width: 3840, min_width: 1920, allow_portrait: false, max_crop_percent: 35 }
    }

    fn ctx() -> Ctx {
        Ctx {
            place: "test".into(), lat: 48.85, lon: 2.35, temp: 12, weather_code: 3,
            weather: "overcast", season: "autumn", time: "afternoon", light: "bright",
            temp_tags: vec![], wind_tags: vec![], light_tags: vec![], season_tags: vec![],
        }
    }

    #[test]
    fn smk_checks_rights_type_dimensions_and_localized_metadata() {
        let mut record = json!({
            "object_number": "KMS-test", "public_domain": true,
            "rights": "https://creativecommons.org/publicdomain/mark/1.0/",
            "object_names": [{"name": "Maleri"}],
            "titles": [{"title": "Hedelandskab. Efterår", "language": "dansk"}],
            "production": [{"creator": "Artist"}],
            "production_date": [{"period": "1889"}],
            "image_width": 6890, "image_height": 4506,
            "image_iiif_id": "https://iip.smk.dk/iiif/test"
        });
        let c = smk_record(&record, &opts()).unwrap();
        assert_eq!(c.painting.title, "Hedelandskab. Efterår");
        assert_eq!(c.painting.year, "1889");
        assert_eq!(c.painting.image, "https://iip.smk.dk/iiif/test/full/3840,/0/default.jpg");
        assert_eq!(select_by_content(vec![c], &ctx()).len(), 1);
        record["rights"] = json!("https://www.smk.dk/section/brug-af-museets-materiale/");
        assert!(smk_record(&record, &opts()).is_none());
        record["rights"] = json!("https://creativecommons.org/publicdomain/mark/1.0/");
        record["object_names"][0]["name"] = json!("Tegning");
        assert!(smk_record(&record, &opts()).is_none());
        record["object_names"][0]["name"] = json!("Maleri");
        record["image_height"] = json!(1000);
        assert!(smk_record(&record, &opts()).is_none());
    }

    #[test]
    fn rijks_checks_image_rights_not_metadata_rights() {
        let mut record = json!({
            "id": "https://id.rijksmuseum.nl/test",
            "identified_by": [
                {"type": "Identifier", "content": "SK-test"},
                {"type": "Name", "content": "Herfstlandschap"},
                {"type": "Name", "content": "Autumn Landscape",
                 "language": [{"id": "http://vocab.getty.edu/aat/300388277"}]}
            ],
            "produced_by": {
                "part": [{"carried_out_by": [{"notation": [{"@language": "en", "@value": "Artist"}]}]}],
                "timespan": {"identified_by": [{"type": "Name", "content": "1880"}]}
            },
            "subject_of": [
                {"digitally_carried_by": [{
                    "access_point": [{"id": "https://iiif.micr.io/test/full/max/0/default.jpg"}],
                    "referred_to_by": [{"content": "https://creativecommons.org/publicdomain/mark/1.0/"}]
                }]},
                {"subject_to": [{"classified_as": [{"id": "https://creativecommons.org/publicdomain/zero/1.0/"}]}]}
            ]
        });
        let c = rijks_record(&record).unwrap();
        assert_eq!(c.painting.title, "Autumn Landscape");
        assert_eq!(c.painting.artist, "Artist");
        assert_eq!(c.painting.year, "1880");
        assert_eq!(select_by_content(vec![c], &ctx()).len(), 1);
        record["subject_of"][0]["digitally_carried_by"][0]["referred_to_by"][0]["content"] = json!("All rights reserved");
        assert!(rijks_record(&record).is_none());
    }

    #[test]
    fn rijks_preserves_attributed_artist_names() {
        let record = json!({
            "id": "https://id.rijksmuseum.nl/test",
            "identified_by": [{"type": "Name", "content": "Landscape"}],
            "produced_by": {"referred_to_by": [
                {"content": "Hendrick Goltzius (eigenhandig gesigneerd)"},
                {"content": "Hendrick Goltzius (signed by artist)",
                 "language": [{"id": "http://vocab.getty.edu/aat/300388277"}]}
            ]},
            "subject_of": [{"digitally_carried_by": [{
                "access_point": [{"id": "https://iiif.micr.io/test/full/max/0/default.jpg"}],
                "referred_to_by": [{"content": "https://creativecommons.org/publicdomain/mark/1.0/"}]
            }]}]
        });
        assert_eq!(rijks_record(&record).unwrap().painting.artist, "Hendrick Goltzius (signed by artist)");
    }

    #[test]
    fn paris_selects_only_free_images_and_preserves_display_metadata() {
        let mut record = json!({
            "entityId": "42", "title": "Paysage d'automne",
            "fieldDateProduction": {"startYear": 1890},
            "fieldOeuvreAuteurs": [{"entity": {"fieldAuteurAuteur": {"entity": {"name": "Artist"}}}}],
            "fieldOeuvreThemeRepresente": [{"entity": {"name": "Rivière"}}],
            "fieldVisuels": [
                {"entity": {"fieldImageLibre": false, "publicUrl": "https://example.org/restricted.jpg"}},
                {"entity": {"fieldImageLibre": "1", "publicUrl": "https://example.org/free.jpg"}}
            ]
        });
        let c = paris_record(&record).unwrap();
        assert_eq!(c.painting.title, "Paysage d'automne");
        assert_eq!(c.painting.artist, "Artist");
        assert_eq!(c.painting.year, "1890");
        assert_eq!(c.painting.image, "https://example.org/free.jpg");
        assert_eq!(select_by_content(vec![c], &ctx()).len(), 1);
        record["fieldVisuels"][1]["entity"]["fieldImageLibre"] = json!(false);
        assert!(paris_record(&record).is_none());
    }

    #[test]
    fn paris_query_limits_type_and_rights_and_escapes_search_text() {
        let q = paris_query("paysage \"automne\"", 7);
        assert!(q.contains(r#"offset: 7"#));
        assert!(q.contains(r#"field_oeuvre_types_objet.entity.field_lref_adlib", value: "4284""#));
        assert!(q.contains(r#"field_visuels.entity.field_image_libre", value: "1""#));
        assert!(q.contains(r#""%paysage \"automne\"%""#));
    }

    #[test]
    fn rejects_text_work_titles_without_rejecting_inscriptions_or_japanese_art() {
        let make = |title: &str| ext_painting(
            title, "Artist", "1800".into(), "https://example.org/art.jpg".into(),
            "autumn river landscape with an inscription".into(), "test", title.into(),
        );
        for title in ["Calligraphy", "Landscape and Sutra", "Autumn manuscript", "Calligraphie"] {
            assert!(crate::select::validate(&make(title).painting, &ctx()).is_err());
            assert!(select_by_content(vec![make(title)], &ctx()).is_empty());
        }
        for title in ["Japanese autumn landscape", "Landscape with an inscription"] {
            assert!(crate::select::validate(&make(title).painting, &ctx()).is_ok());
            assert_eq!(select_by_content(vec![make(title)], &ctx()).len(), 1);
        }
    }

    #[test]
    fn all_new_sources_are_selectable() {
        let only = vec!["smk".into(), "rijksmuseum".into(), "paris".into()];
        let order = super::super::source_order(&only);
        assert_eq!(order.len(), 3);
        for name in &only {
            assert!(order.iter().any(|(n, _)| n == name));
        }
    }

    #[test]
    fn paris_credential_is_optional() {
        if std::env::var_os("PLEIN_AIR_CREDENTIAL_TEST").is_some() {
            let available = std::env::var("PARIS_MUSEES_API").is_ok_and(|v| !v.trim().is_empty());
            let order = super::super::source_order(&[]);
            assert_eq!(order.iter().any(|(name, _)| *name == "paris"), available);
            for source in ["met", "artic", "cleveland", "wikimedia", "smk", "rijksmuseum"] {
                assert!(order.iter().any(|(name, _)| *name == source));
            }
            if !available {
                let error = paris(&ctx(), &opts()).expect_err("missing credential must fail before the request");
                assert!(error.to_string().contains("PARIS_MUSEES_API"));
            }
            return;
        }
        for value in [None, Some(""), Some(" \t"), Some("test-placeholder")] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "sources::museums::tests::paris_credential_is_optional"])
                .env("PLEIN_AIR_CREDENTIAL_TEST", "1")
                .env_remove("PARIS_MUSEES_API");
            if let Some(value) = value {
                command.env("PARIS_MUSEES_API", value);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "credential case {value:?}: {}", String::from_utf8_lossy(&output.stdout));
        }
    }

    fn check_live(fetch: fn(&Ctx, &ImgOpts) -> Result<Vec<Painting>>, source: &str) -> Result<()> {
        let opts = opts();
        let paintings = fetch(&ctx(), &opts)?;
        anyhow::ensure!(!paintings.is_empty(), "{source}: no candidates");
        let p = &paintings[0];
        crate::select::validate(p, &ctx()).map_err(anyhow::Error::msg)?;
        let bytes = net::image(&p.image, &opts)?;
        let size = imagesize::blob_size(&bytes)?;
        println!("{source}: {} — {} ({}x{})", p.artist, p.title, size.width, size.height);
        Ok(())
    }

    #[test]
    #[ignore = "live museum API and image download"]
    fn live_smk() -> Result<()> {
        check_live(smk, "smk")
    }

    #[test]
    #[ignore = "live museum API and image download"]
    fn live_rijksmuseum() -> Result<()> {
        check_live(rijksmuseum, "rijksmuseum")
    }

    #[test]
    #[ignore = "requires PARIS_MUSEES_API and live API/image access"]
    fn live_paris() -> Result<()> {
        check_live(paris, "paris")
    }
}
