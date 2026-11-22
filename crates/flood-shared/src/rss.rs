use crate::model::{AlertLevel, BmkgNowcast, FloodSnapshot, NewsItem};

pub fn parse_bmkg_nowcast_rss(xml: &str) -> Vec<BmkgNowcast> {
    let mut out = Vec::new();
    for raw_item in split_items(xml).into_iter().take(20) {
        let title = extract_tag(raw_item, "title");
        if title.is_empty() {
            continue;
        }
        let province = infer_bmkg_province(&title);
        let desc = clean_whitespace(&strip_html_tags(&extract_tag(raw_item, "description")));
        let summary: String = desc.chars().take(260).collect();
        let link = extract_tag(raw_item, "link");
        let pub_date = extract_tag(raw_item, "pubDate");
        out.push(BmkgNowcast {
            title,
            province,
            summary,
            link,
            pub_date,
        });
    }
    out
}

pub fn parse_flood_news_rss(xml: &str) -> Vec<NewsItem> {
    let mut out = Vec::new();
    for raw_item in split_items(xml).into_iter().take(18) {
        let raw_title = extract_tag(raw_item, "title");
        if raw_title.is_empty() {
            continue;
        }
        let tag_source = extract_tag(raw_item, "source");
        let (title, source) = if !tag_source.is_empty() {
            let cleaned_title = raw_title
                .strip_suffix(&format!(" - {tag_source}"))
                .unwrap_or(&raw_title)
                .trim()
                .to_string();
            (cleaned_title, tag_source)
        } else if let Some((head, tail)) = raw_title.rsplit_once(" - ") {
            (head.trim().to_string(), tail.trim().to_string())
        } else {
            (raw_title.clone(), "Berita Indonesia".to_string())
        };

        let link = extract_tag(raw_item, "link");
        let pub_date = extract_tag(raw_item, "pubDate");
        let desc = extract_tag(raw_item, "description");
        let region_hint = infer_region_hint(&format!("{title} {desc}"));

        out.push(NewsItem {
            title,
            link,
            source,
            pub_date,
            region_hint,
        });
    }
    out
}

pub fn parse_filtered_indonesian_news(xml: &str, default_source: &str) -> (Vec<NewsItem>, Vec<NewsItem>) {
    const HYDRO_KEYWORDS: &[&str] = &[
        "banjir", " rob ", "banjir rob", "pasang rob", "hujan", "sungai", "tanggul",
        "genangan", "bmkg", "cuaca", "longsor", "pasang", "luapan", "bendung",
        "air bah", "gelombang", "hidrometeorologi", "el nino", "la nina", "angin kencang",
    ];
    const EXCLUDE_NON_HYDRO: &[&str] = &["kebakaran", "korupsi", "kpk", "sepak bola", "liga"];

    let mut matched = Vec::new();
    let mut fallback = Vec::new();

    for raw_item in split_items(xml).into_iter().take(40) {
        let title = extract_tag(raw_item, "title");
        if title.is_empty() {
            continue;
        }
        let link = extract_tag(raw_item, "link");
        if link.is_empty() {
            continue;
        }
        let pub_date = extract_tag(raw_item, "pubDate");
        let desc = clean_whitespace(&strip_html_tags(&extract_tag(raw_item, "description")));
        let tag_source = extract_tag(raw_item, "source");
        let source = if tag_source.is_empty() {
            default_source.to_string()
        } else {
            tag_source
        };
        let combined = format!("{title} {desc}").to_ascii_lowercase();
        let region_hint = infer_region_hint(&combined);

        let item = NewsItem {
            title,
            link,
            source,
            pub_date,
            region_hint,
        };

        let excluded = EXCLUDE_NON_HYDRO.iter().any(|ex| combined.contains(ex))
            && !combined.contains("banjir");
        if !excluded && HYDRO_KEYWORDS.iter().any(|kw| combined.contains(kw)) {
            matched.push(item);
        } else if !excluded && fallback.len() < 4 {
            fallback.push(item);
        }
    }

    (matched, fallback)
}

pub fn build_alerts_rss_xml(snapshot: &FloodSnapshot, region_filter: &str, base_url: &str) -> String {
    let rf = region_filter.trim().to_ascii_lowercase();
    let matches_region = |prov: &str, island: &str| -> bool {
        rf.is_empty()
            || rf == "indonesia"
            || rf == "nasional"
            || prov.to_ascii_lowercase().contains(&rf)
            || island.to_ascii_lowercase().contains(&rf)
    };

    let mut items_xml = String::new();
    for st in snapshot.stations.iter().filter(|s| {
        s.alert_level >= AlertLevel::Waspada && matches_region(&s.province, &s.island)
    }) {
        let title = xml_escape(&format!(
            "[{}] {} ({}) — {:.1} m3/s ({:.2}x Q75)",
            st.alert_level.label().to_uppercase(),
            st.name,
            st.province,
            st.current_discharge_m3s,
            st.anomaly_ratio
        ));
        let desc = xml_escape(&format!(
            "{} | {} | Hujan 72j: {:.1} mm. {}",
            st.mechanism.label_id(),
            st.return_period_est,
            st.rain_72h_mm,
            st.public_advice
        ));
        let link = xml_escape(&format!("{base_url}/?basin={}", st.id));
        let guid = xml_escape(&format!("{}-{}-{}", st.id, st.alert_level.label(), snapshot.generated_at_utc));
        items_xml.push_str(&format!(
            "    <item>\n      <title>{title}</title>\n      <link>{link}</link>\n      <guid isPermaLink=\"false\">{guid}</guid>\n      <description>{desc}</description>\n    </item>\n"
        ));
    }

    for rb in snapshot.coastal_rob.iter().filter(|r| {
        r.alert_level >= AlertLevel::Waspada && matches_region(&r.province, &r.island)
    }) {
        let title = xml_escape(&format!(
            "[ROB {}] {} ({}) — Indeks {:.2}x",
            rb.alert_level.label().to_uppercase(),
            rb.name,
            rb.province,
            rb.rob_index
        ));
        let desc = xml_escape(&rb.note);
        let link = xml_escape(&format!("{base_url}/?basin={}", rb.id));
        let guid = xml_escape(&format!("{}-{}-{}", rb.id, rb.alert_level.label(), snapshot.generated_at_utc));
        items_xml.push_str(&format!(
            "    <item>\n      <title>{title}</title>\n      <link>{link}</link>\n      <guid isPermaLink=\"false\">{guid}</guid>\n      <description>{desc}</description>\n    </item>\n"
        ));
    }

    for nc in snapshot
        .bmkg_nowcasts
        .iter()
        .filter(|n| rf.is_empty() || rf == "indonesia" || rf == "nasional" || n.province.to_ascii_lowercase().contains(&rf))
        .take(8)
    {
        let title = xml_escape(&format!("[BMKG Nowcast] {}", nc.title));
        let desc = xml_escape(&nc.summary);
        let link = xml_escape(if nc.link.is_empty() { base_url } else { &nc.link });
        let guid = xml_escape(&format!("bmkg-{}", nc.title));
        items_xml.push_str(&format!(
            "    <item>\n      <title>{title}</title>\n      <link>{link}</link>\n      <guid isPermaLink=\"false\">{guid}</guid>\n      <description>{desc}</description>\n    </item>\n"
        ));
    }

    let channel_title = if region_filter.trim().is_empty() {
        "Hidrosentinel Nusantara — Peringatan Banjir &amp; Rob Nasional".to_string()
    } else {
        xml_escape(&format!("Hidrosentinel Nusantara — Wilayah {}", region_filter.trim()))
    };

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">\n  <channel>\n    <title>{channel_title}</title>\n    <link>{}</link>\n    <description>Feed peringatan dini banjir sungai (GloFAS v4), pasang rob pesisir, dan nowcast cuaca ekstrem BMKG.</description>\n    <language>id</language>\n{items_xml}  </channel>\n</rss>\n",
        xml_escape(base_url)
    )
}

fn split_items(xml: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item") {
        let after_start = &rest[start..];
        let Some(tag_end) = after_start.find('>') else {
            break;
        };
        let body_start = &after_start[tag_end + 1..];
        if let Some(end) = body_start.find("</item>") {
            items.push(&body_start[..end]);
            rest = &body_start[end + 7..];
        } else {
            break;
        }
    }
    items
}

fn extract_tag(item: &str, tag: &str) -> String {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let Some(pos) = item.find(&open_prefix) else {
        return String::new();
    };
    let after_open = &item[pos + open_prefix.len()..];
    // Ensure we matched the exact tag boundary ('>' or whitespace)
    if !after_open.starts_with('>') && !after_open.starts_with(char::is_whitespace) {
        return String::new();
    }
    let Some(gt) = after_open.find('>') else {
        return String::new();
    };
    let content = &after_open[gt + 1..];
    let Some(end) = content.find(&close_tag) else {
        return String::new();
    };
    let mut raw = content[..end].trim();
    if let Some(stripped) = raw.strip_prefix("<![CDATA[") {
        if let Some(end_cdata) = stripped.strip_suffix("]]>") {
            raw = end_cdata.trim();
        }
    }
    decode_entities(raw).trim().to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn clean_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn infer_bmkg_province(title: &str) -> String {
    if let Some((_, prov)) = title.rsplit_once(" di ") {
        prov.trim().to_string()
    } else {
        "Indonesia".to_string()
    }
}

fn infer_region_hint(text: &str) -> String {
    let low = text.to_ascii_lowercase();
    if [
        "jakarta", "jabodetabek", "bogor", "depok", "tangerang", "bekasi", "bandung", "karawang",
        "cirebon", "indramayu", "semarang", "demak", "kudus", "pekalongan", "solo", "surakarta",
        "bojonegoro", "surabaya", "sidoarjo", "malang", "yogyakarta", "bantul", "banten", "jawa",
        "ciliwung", "citarum", "brantas", "serayu",
    ]
    .iter()
    .any(|k| low.contains(k))
    {
        "Jawa".to_string()
    } else if [
        "aceh", "medan", "sumut", "sumatera", "sumbar", "padang", "riau", "pekanbaru", "jambi",
        "batanghari", "palembang", "sumsel", "bengkulu", "lampung", "deli", "musi", "kampar",
    ]
    .iter()
    .any(|k| low.contains(k))
    {
        "Sumatera".to_string()
    } else if [
        "kalimantan", "kalbar", "kalteng", "kalsel", "kaltim", "kalut", "pontianak",
        "banjarmasin", "martapura", "samarinda", "palangka", "kapuas", "barito", "mahakam",
    ]
    .iter()
    .any(|k| low.contains(k))
    {
        "Kalimantan".to_string()
    } else if [
        "sulawesi", "makassar", "sulsel", "palu", "sulteng", "kendari", "sultra", "manado",
        "sulut", "gorontalo", "mamuju", "sulbar",
    ]
    .iter()
    .any(|k| low.contains(k))
    {
        "Sulawesi".to_string()
    } else if ["bali", "lombok", "ntb", "ntt", "kupang", "mataram", "flores"]
        .iter()
        .any(|k| low.contains(k))
    {
        "Bali-Nustra".to_string()
    } else if ["maluku", "ambon", "ternate", "papua", "jayapura", "merauke", "mimika"]
        .iter()
        .any(|k| low.contains(k))
    {
        "Maluku-Papua".to_string()
    } else {
        "Nasional".to_string()
    }
}
