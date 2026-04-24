pub mod routes;

use std::borrow::Cow;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct WebAssets;

pub fn asset_for_path(path: &str) -> Option<Cow<'static, [u8]>> {
    asset_for_path_with_mime(path).map(|(data, _)| data)
}

pub fn asset_for_path_with_mime(path: &str) -> Option<(Cow<'static, [u8]>, &'static str)> {
    let key = normalize_asset_path(path);
    if let Some(asset) = WebAssets::get(&key) {
        return Some((asset.data, mime_for_path(&key)));
    }

    WebAssets::get("index.html").map(|asset| (asset.data, mime_for_path("index.html")))
}

pub fn mime_for_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn normalize_asset_path(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        "index.html".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_embeds_index_html() {
        let data = asset_for_path("/index.html").expect("index.html should be embedded");
        assert!(!data.is_empty());
    }

    #[test]
    fn spa_catch_all_returns_index_html() {
        let index = asset_for_path("/index.html").unwrap();
        let route = asset_for_path("/agents/some-agent-id").unwrap();
        assert_eq!(index, route);
    }

    #[test]
    fn mime_types_cover_frontend_assets() {
        assert_eq!(mime_for_path("/"), "application/octet-stream");
        assert_eq!(mime_for_path("/index.html"), "text/html; charset=utf-8");
        assert_eq!(
            mime_for_path("/assets/index.js"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_for_path("/assets/index.css"),
            "text/css; charset=utf-8"
        );
    }
}
