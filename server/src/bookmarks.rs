use axum::{extract::State, routing::post, Json, Router};
use reqwest::{redirect::Policy, Url};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, time::Duration};

use crate::{auth::middleware::AuthUser, auth::routes::AppError, AppState};

const MAX_HTML_BYTES: usize = 512 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route("/bookmarks/metadata", post(fetch_bookmark_metadata))
}

#[derive(Deserialize)]
pub struct BookmarkMetadataRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct BookmarkMetadataResponse {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub site_name: Option<String>,
    pub image_url: Option<String>,
}

async fn fetch_bookmark_metadata(
    State(_state): State<AppState>,
    _auth: AuthUser,
    Json(request): Json<BookmarkMetadataRequest>,
) -> Result<Json<BookmarkMetadataResponse>, AppError> {
    let url = validate_public_url(&request.url).await?;
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(8))
        .user_agent("StarlaneNotesBookmarkPreview/1.0")
        .build()
        .map_err(AppError::internal)?;
    let response = client.get(url.clone()).send().await.map_err(|_| {
        AppError::BadRequest("Could not fetch bookmark metadata".into())
    })?;
    if response.status().is_redirection() {
        return Err(AppError::BadRequest("Bookmark redirects are not followed for safety".into()));
    }
    if !response.status().is_success() {
        return Err(AppError::BadRequest("Bookmark URL did not return a successful response".into()));
    }
    let bytes = response.bytes().await.map_err(AppError::internal)?;
    let html = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_HTML_BYTES)]);
    let title = meta_value(&html, "og:title")
        .or_else(|| title_value(&html))
        .unwrap_or_else(|| url.host_str().unwrap_or("Bookmark").to_string());
    Ok(Json(BookmarkMetadataResponse {
        url: url.to_string(),
        title: truncate(title, 240),
        description: meta_value(&html, "og:description").map(|value| truncate(value, 600)),
        site_name: meta_value(&html, "og:site_name").map(|value| truncate(value, 120)),
        image_url: meta_value(&html, "og:image").filter(|value| Url::parse(value).is_ok()),
    }))
}

async fn validate_public_url(value: &str) -> Result<Url, AppError> {
    let url = Url::parse(value.trim()).map_err(|_| AppError::BadRequest("Bookmark URL must be valid".into()))?;
    if !matches!(url.scheme(), "http" | "https") || url.username() != "" || url.password().is_some() {
        return Err(AppError::BadRequest("Bookmark URL must be a public HTTP(S) URL".into()));
    }
    let host = url.host_str().ok_or_else(|| AppError::BadRequest("Bookmark URL needs a host".into()))?;
    if host.eq_ignore_ascii_case("localhost") { return Err(AppError::BadRequest("Private bookmark URLs are blocked".into())); }
    let addresses = tokio::net::lookup_host((host, url.port_or_known_default().unwrap_or(443)))
        .await
        .map_err(|_| AppError::BadRequest("Bookmark host could not be resolved".into()))?;
    if addresses.into_iter().any(|address| !is_public_ip(address.ip())) {
        return Err(AppError::BadRequest("Private bookmark URLs are blocked".into()));
    }
    Ok(url)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.is_documentation()),
        IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unspecified() || ip.is_unique_local() || ip.is_unicast_link_local()),
    }
}

fn meta_value(html: &str, key: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let marker = format!("{}=\"{}\"", "property", key.to_ascii_lowercase());
    let start = lower.find(&marker).or_else(|| lower.find(&format!("name=\"{}\"", key.to_ascii_lowercase())))?;
    let tag = &html[start..html[start..].find('>')? + start];
    attribute_value(tag, "content")
}

fn title_value(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string()).filter(|value| !value.is_empty())
}

fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let start = lower.find(&format!("{}=\"", name))? + name.len() + 2;
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].trim().to_string()).filter(|value| !value.is_empty())
}

fn truncate(mut value: String, limit: usize) -> String { value.truncate(limit); value }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_private_addresses() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("10.0.0.1".parse().unwrap()));
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
    }
}
