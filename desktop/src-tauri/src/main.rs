//! Klien tipis desktop Hidrosentinel Nusantara.
//!
//! Jendela memuat halaman peluncur lokal yang memeriksa koneksi lalu membuka situs resmi
//! https://flood.transbulent.id. Data banjir tidak dibundel: aplikasi selalu menampilkan situs
//! terkini, termasuk peringatan "Indikasi model, bukan peringatan resmi" dan banner data lama.
//! Situs jarak jauh tidak mendapat akses IPC Tauri (tanpa capability). Tautan ke domain lain
//! dibuka di peramban bawaan sistem.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

const SITE_HOST: &str = "flood.transbulent.id";

/// URL yang boleh tampil di jendela aplikasi.
fn is_app_url(url: &Url) -> bool {
    let site = url.scheme() == "https" && url.host_str() == Some(SITE_HOST) && url.port().is_none();
    // Halaman peluncur lokal: tauri://localhost (Linux/macOS), http(s)://tauri.localhost (Windows).
    let launcher = url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost");
    site || launcher || url.as_str() == "about:blank"
}

/// Tautan luar dibuka di peramban; skema lain (file:, javascript:, data:) ditolak.
fn open_outside(app: &AppHandle, url: &Url) {
    if matches!(url.scheme(), "https" | "http" | "mailto") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let nav_app = app.handle().clone();
            let popup_app = app.handle().clone();
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Hidrosentinel Nusantara")
                .inner_size(1280.0, 820.0)
                .min_inner_size(380.0, 600.0)
                .center()
                .on_navigation(move |url| {
                    if is_app_url(url) {
                        return true;
                    }
                    open_outside(&nav_app, url);
                    false
                })
                .on_new_window(move |url, _features| {
                    // target=_blank dan window.open: selalu di peramban sistem, tidak di jendela baru.
                    open_outside(&popup_app, &url);
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("gagal menjalankan Hidrosentinel Nusantara");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_app_url(&Url::parse(url).unwrap())
    }

    #[test]
    fn only_the_official_site_and_launcher_stay_in_the_window() {
        assert!(allowed("https://flood.transbulent.id/"));
        assert!(allowed(
            "https://flood.transbulent.id/?s=ciliwung-manggarai&lens=riset"
        ));
        assert!(allowed("tauri://localhost/index.html"));
        assert!(allowed("http://tauri.localhost/index.html"));
        for url in [
            "http://flood.transbulent.id/",
            "https://flood.transbulent.id:8443/",
            "https://flood.transbulent.id.evil.example/",
            "https://evil.example/?u=flood.transbulent.id",
            "https://transbulent.id/",
            "https://ntfy.sh/hidrosentinel",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
        ] {
            assert!(!allowed(url), "{url} must leave the app window");
        }
    }
}
