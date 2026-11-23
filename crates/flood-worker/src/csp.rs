//! Content-Security-Policy halaman statis. Skrip inline (pemuat WASM dari Trunk) diizinkan lewat
//! hash SHA-256 isi skripnya, bukan `'unsafe-inline'`.

use sha2::{Digest, Sha256};

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// `sha256-…` untuk setiap `<script>` inline yang dieksekusi: tanpa `src` dan bukan blok data
/// (`application/ld+json`). Hash dihitung atas teks persis di antara tag pembuka dan penutup.
pub fn inline_script_hashes(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase(); // panjang byte sama, indeks berlaku untuk `html`
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find("<script") {
        let after = pos + rel + "<script".len();
        let Some(tag_end_rel) = lower[after..].find('>') else {
            break;
        };
        let tag_end = after + tag_end_rel;
        let attrs = &lower[after..tag_end];
        let body_start = tag_end + 1;
        let Some(close_rel) = lower[body_start..].find("</script") else {
            break;
        };
        let body_end = body_start + close_rel;
        pos = body_end + "</script".len();
        let executable = !attrs.contains("src=") && !attrs.contains("application/ld+json");
        let body = &html[body_start..body_end];
        if executable && !body.trim().is_empty() {
            out.push(format!(
                "sha256-{}",
                base64(&Sha256::digest(body.as_bytes()))
            ));
        }
    }
    out
}

/// CSP halaman statis. `script_hashes` berisi `sha256-…` dari `inline_script_hashes`.
pub fn content_security_policy(script_hashes: &[String]) -> String {
    let mut script_src =
        String::from("script-src 'self' 'wasm-unsafe-eval' https://static.cloudflareinsights.com");
    for h in script_hashes {
        script_src.push_str(" '");
        script_src.push_str(h);
        script_src.push('\'');
    }
    // Ubin DEM Terrarium: hanya bucket elevation-tiles-prod (bentuk path dan subdomain).
    const DEM: &str = "https://s3.amazonaws.com/elevation-tiles-prod/ https://elevation-tiles-prod.s3.amazonaws.com";
    format!(
        "default-src 'self'; {script_src}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob: https://tiles.openfreemap.org {DEM}; \
         font-src 'self' data:; \
         connect-src 'self' https://tiles.openfreemap.org {DEM} https://static.cloudflareinsights.com https://cloudflareinsights.com; \
         worker-src 'self' blob:; child-src 'self' blob:; object-src 'none'; \
         base-uri 'self'; frame-ancestors 'self'; form-action 'self'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_sha256_digest() {
        // SHA-256("abc") dalam base64 baku, seperti yang dipakai sumber CSP.
        assert_eq!(
            format!("sha256-{}", base64(&Sha256::digest(b"abc"))),
            "sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
    }

    #[test]
    fn hashes_only_executable_inline_scripts() {
        let html = r#"<head>
<script type="module">abc</script>
<script src="/x.js"></script>
<script type="application/ld+json">{"a":1}</script>
<script>  </script>
</head>"#;
        assert_eq!(
            inline_script_hashes(html),
            vec!["sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=".to_string()]
        );
    }

    #[test]
    fn policy_has_no_unsafe_inline_for_scripts() {
        let csp = content_security_policy(&["sha256-AAAA".to_string()]);
        let script = csp
            .split("; ")
            .find(|d| d.starts_with("script-src"))
            .unwrap();
        assert!(script.contains("'sha256-AAAA'"));
        assert!(!script.contains("unsafe-inline"));
    }
}
