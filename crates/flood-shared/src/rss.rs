use crate::model::{BmkgNowcast, FloodSnapshot, NewsItem};

/// Umur maksimum berita yang ditampilkan.
pub const NEWS_MAX_AGE_SECS: i64 = 72 * 3600;

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

/// Kata setelah "banjir" yang menandakan makna kiasan (banjir pesanan, banjir kritik, ...).
const FIGURATIVE_AFTER_BANJIR: &[&str] = &[
    "pesanan",
    "kritik",
    "hadiah",
    "promo",
    "diskon",
    "penonton",
    "pujian",
    "order",
    "dukungan",
    "komentar",
    "tawaran",
    "investasi",
    "peminat",
    "air mata",
    "bonus",
    "cuan",
    "job",
    "rezeki",
    "tamu",
    "pengunjung",
    "pembeli",
    "produk",
    "impor",
    "barang",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Judul berita dianggap tentang banjir bila memuat banjir (bukan kiasan), banjir bandang,
/// rob, genangan, luapan/meluap, atau tanggul jebol.
pub fn is_flood_headline(title: &str) -> bool {
    let w = words(title);
    let lower = w.join(" ");
    for (i, word) in w.iter().enumerate() {
        match word.as_str() {
            "banjir" | "kebanjiran" => {
                let rest = w[i + 1..]
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                if word == "banjir" && FIGURATIVE_AFTER_BANJIR.iter().any(|f| rest.starts_with(f)) {
                    continue;
                }
                return true;
            }
            "rob" | "genangan" | "tergenang" | "luapan" | "meluap" => return true,
            _ => {}
        }
    }
    lower.contains("tanggul jebol") || (lower.contains("tanggul") && lower.contains("jebol"))
}

/// Berita banjir dari RSS media Indonesia: filter judul ketat, umur ≤ 72 jam terhadap
/// `now_epoch_s`, `pub_date` ISO 8601 UTC. Tanpa fallback: daftar kosong itu sah.
pub fn parse_flood_news(xml: &str, default_source: &str, now_epoch_s: i64) -> Vec<NewsItem> {
    let mut out = Vec::new();
    for raw_item in split_items(xml).into_iter().take(60) {
        let title = clean_whitespace(&strip_html_tags(&extract_tag(raw_item, "title")));
        let link = extract_tag(raw_item, "link");
        if title.is_empty() || link.is_empty() || !is_flood_headline(&title) {
            continue;
        }
        let Some(ts) = parse_rss_date(&extract_tag(raw_item, "pubDate")) else {
            continue;
        };
        let age = now_epoch_s - ts;
        if !(-3600..=NEWS_MAX_AGE_SECS).contains(&age) {
            continue;
        }
        let desc = clean_whitespace(&strip_html_tags(&extract_tag(raw_item, "description")));
        out.push(NewsItem {
            region_hint: infer_region_hint(&format!("{title} {desc}")),
            title,
            link,
            source: default_source.to_string(),
            pub_date: epoch_to_iso(ts),
        });
    }
    out
}

/// Gabung, deduplikasi (judul ternormalisasi atau tautan sama), urutkan terbaru dulu.
pub fn merge_news(mut items: Vec<NewsItem>, limit: usize) -> Vec<NewsItem> {
    items.sort_by(|a, b| b.pub_date.cmp(&a.pub_date));
    let mut seen_titles = std::collections::HashSet::new();
    let mut seen_links = std::collections::HashSet::new();
    items.retain(|n| {
        let key = words(&n.title).join(" ");
        seen_titles.insert(key) && seen_links.insert(n.link.clone())
    });
    items.truncate(limit);
    items
}

/// RFC 2822 (`Sun, 22 Nov 2026 16:00:00 +0700`, `GMT`) atau ISO 8601 → detik epoch UTC.
pub fn parse_rss_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.as_bytes().first()?.is_ascii_digit() && s.contains('T') {
        return parse_iso8601(s);
    }
    let body = s.split_once(',').map(|(_, b)| b).unwrap_or(s).trim();
    let mut parts = body.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let mut year: i64 = parts.next()?.parse().ok()?;
    if year < 100 {
        year += 2000;
    }
    let (h, m, sec) = parse_hms(parts.next()?)?;
    let offset = parts.next().map(parse_offset).unwrap_or(Some(0))?;
    Some(days_from_civil(year, month, day) * 86400 + h * 3600 + m * 60 + sec - offset)
}

pub(crate) fn parse_iso8601(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: i64 = d.next()?.parse().ok()?;
    let da: i64 = d.next()?.parse().ok()?;
    let (time, offset) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0)
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        (&rest[..pos], parse_offset(&rest[pos..])?)
    } else {
        (rest, 0)
    };
    let time = time.split('.').next()?;
    let (h, m, sec) = parse_hms(time)?;
    Some(days_from_civil(y, mo, da) * 86400 + h * 3600 + m * 60 + sec - offset)
}

fn parse_hms(t: &str) -> Option<(i64, i64, i64)> {
    let mut it = t.split(':');
    let h = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let s = it.next().map(|x| x.parse().ok()).unwrap_or(Some(0))?;
    Some((h, m, s))
}

fn parse_offset(z: &str) -> Option<i64> {
    let z = z.trim();
    match z.to_ascii_uppercase().as_str() {
        "GMT" | "UT" | "UTC" | "Z" => return Some(0),
        "WIB" => return Some(7 * 3600),
        "WITA" => return Some(8 * 3600),
        "WIT" => return Some(9 * 3600),
        _ => {}
    }
    let sign = match z.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = z[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 4 {
        return None;
    }
    let h: i64 = digits[..2].parse().ok()?;
    let m: i64 = digits[2..].parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

fn month_number(m: &str) -> Option<i64> {
    let m = m.to_ascii_lowercase();
    let names = [
        ["jan", "jan"],
        ["feb", "feb"],
        ["mar", "mar"],
        ["apr", "apr"],
        ["may", "mei"],
        ["jun", "jun"],
        ["jul", "jul"],
        ["aug", "agu"],
        ["sep", "sep"],
        ["oct", "okt"],
        ["nov", "nov"],
        ["dec", "des"],
    ];
    names
        .iter()
        .position(|[en, id]| m.starts_with(en) || m.starts_with(id))
        .map(|i| i as i64 + 1)
}

/// Hari sejak 1970-01-01 (algoritma Howard Hinnant).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn epoch_to_iso(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

const RSS_DISCLAIMER: &str =
    "Indikasi model, bukan peringatan resmi. Ikuti informasi BPBD, BMKG, dan BBWS setempat.";

pub fn build_alerts_rss_xml(
    snapshot: &FloodSnapshot,
    region_filter: &str,
    base_url: &str,
) -> String {
    let rf = region_filter.trim().to_lowercase();
    let matches_region = |prov: &str, island: &str| -> bool {
        rf.is_empty()
            || rf == "indonesia"
            || rf == "nasional"
            || prov.to_lowercase().contains(&rf)
            || island.to_lowercase().contains(&rf)
    };
    let item = |title: &str, link: &str, guid: &str, desc: &str| {
        format!(
            "    <item>\n      <title>{}</title>\n      <link>{}</link>\n      <guid isPermaLink=\"false\">{}</guid>\n      <description>{}</description>\n    </item>\n",
            xml_escape(title),
            xml_escape(link),
            xml_escape(guid),
            xml_escape(desc)
        )
    };

    let mut items_xml = String::new();
    for st in snapshot
        .stations
        .iter()
        .filter(|s| s.level.is_elevated() && matches_region(&s.province, &s.island))
    {
        items_xml.push_str(&item(
            &format!("[{}] {} ({})", st.level.label_id(), st.name, st.province),
            &format!("{base_url}/?basin={}", st.id),
            &format!(
                "{}-{}-{}",
                st.id,
                st.level.slug(),
                snapshot.generated_at_utc
            ),
            &format!(
                "{} — {} {RSS_DISCLAIMER}",
                st.mechanism.label_id(),
                st.level_reason
            ),
        ));
    }
    for rb in snapshot
        .coastal_rob
        .iter()
        .filter(|r| r.level.is_elevated() && matches_region(&r.province, &r.island))
    {
        items_xml.push_str(&item(
            &format!(
                "[Rob · {}] {} ({})",
                rb.level.label_id(),
                rb.name,
                rb.province
            ),
            &format!("{base_url}/?basin={}", rb.id),
            &format!(
                "{}-{}-{}",
                rb.id,
                rb.level.slug(),
                snapshot.generated_at_utc
            ),
            &format!("{} {RSS_DISCLAIMER}", rb.level_reason),
        ));
    }
    for nc in snapshot
        .bmkg_nowcasts
        .iter()
        .filter(|n| {
            rf.is_empty()
                || rf == "indonesia"
                || rf == "nasional"
                || n.province.to_lowercase().contains(&rf)
        })
        .take(8)
    {
        items_xml.push_str(&item(
            &format!("[Nowcast BMKG] {}", nc.title),
            if nc.link.is_empty() {
                base_url
            } else {
                &nc.link
            },
            &format!("bmkg-{}", nc.title),
            &nc.summary,
        ));
    }

    let channel_title = if region_filter.trim().is_empty() {
        "Hidrosentinel Nusantara — indikasi banjir dan rob".to_string()
    } else {
        format!("Hidrosentinel Nusantara — {}", region_filter.trim())
    };

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">\n  <channel>\n    <title>{}</title>\n    <link>{}</link>\n    <description>{}</description>\n    <language>id</language>\n{items_xml}  </channel>\n</rss>\n",
        xml_escape(&channel_title),
        xml_escape(base_url),
        xml_escape(&format!(
            "Stasiun sungai dan titik rob berlevel tinggi ke atas menurut model, serta nowcast BMKG. {RSS_DISCLAIMER}"
        ))
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
    let mut search = item;
    loop {
        let Some(pos) = search.find(&open_prefix) else {
            return String::new();
        };
        let after_open = &search[pos + open_prefix.len()..];
        if !after_open.starts_with('>') && !after_open.starts_with(char::is_whitespace) {
            search = after_open;
            continue;
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
        return decode_entities(raw).trim().to_string();
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
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

pub fn xml_escape(s: &str) -> String {
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
    let low = text.to_lowercase();
    let groups: &[(&str, &[&str])] = &[
        (
            "Jawa",
            &[
                "jakarta",
                "jabodetabek",
                "bogor",
                "depok",
                "tangerang",
                "bekasi",
                "bandung",
                "karawang",
                "cirebon",
                "indramayu",
                "semarang",
                "demak",
                "kudus",
                "pekalongan",
                "solo",
                "surakarta",
                "bojonegoro",
                "surabaya",
                "sidoarjo",
                "malang",
                "yogyakarta",
                "bantul",
                "banten",
                "jawa",
                "ciliwung",
                "citarum",
                "brantas",
                "serayu",
            ],
        ),
        (
            "Sumatera",
            &[
                "aceh",
                "medan",
                "sumut",
                "sumatera",
                "sumbar",
                "padang",
                "riau",
                "pekanbaru",
                "jambi",
                "batanghari",
                "palembang",
                "sumsel",
                "bengkulu",
                "lampung",
                "deli",
                "musi",
                "kampar",
            ],
        ),
        (
            "Kalimantan",
            &[
                "kalimantan",
                "kalbar",
                "kalteng",
                "kalsel",
                "kaltim",
                "kaltara",
                "pontianak",
                "banjarmasin",
                "martapura",
                "samarinda",
                "palangka",
                "kapuas",
                "barito",
                "mahakam",
            ],
        ),
        (
            "Sulawesi",
            &[
                "sulawesi",
                "makassar",
                "sulsel",
                "palu",
                "sulteng",
                "kendari",
                "sultra",
                "manado",
                "sulut",
                "gorontalo",
                "mamuju",
                "sulbar",
            ],
        ),
        (
            "Bali-Nusra",
            &[
                "bali", "lombok", "ntb", "ntt", "kupang", "mataram", "flores",
            ],
        ),
        (
            "Maluku-Papua",
            &[
                "maluku", "ambon", "ternate", "papua", "jayapura", "merauke", "mimika",
            ],
        ),
    ];
    groups
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| low.contains(k)))
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| "Nasional".to_string())
}
