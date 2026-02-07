use regex::Regex;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{ScrapedGame, ScrapeProgress};

// ─── WordPress REST API response types ───

#[derive(Debug, Deserialize)]
pub struct WpPost {
    pub id: i64,
    pub date: Option<String>,
    pub link: Option<String>,
    pub title: WpRendered,
    pub content: WpRendered,
    #[serde(rename = "_embedded")]
    pub embedded: Option<WpEmbedded>,
}

#[derive(Debug, Deserialize)]
pub struct WpRendered {
    pub rendered: String,
}

#[derive(Debug, Deserialize)]
pub struct WpEmbedded {
    #[serde(rename = "wp:featuredmedia")]
    pub featured_media: Option<Vec<WpMedia>>,
}

#[derive(Debug, Deserialize)]
pub struct WpMedia {
    pub source_url: Option<String>,
    pub media_details: Option<WpMediaDetails>,
}

#[derive(Debug, Deserialize)]
pub struct WpMediaDetails {
    pub sizes: Option<WpMediaSizes>,
}

#[derive(Debug, Deserialize)]
pub struct WpMediaSizes {
    pub medium: Option<WpMediaSize>,
    pub thumbnail: Option<WpMediaSize>,
    pub medium_large: Option<WpMediaSize>,
}

#[derive(Debug, Deserialize)]
pub struct WpMediaSize {
    pub source_url: Option<String>,
}

// ─── Shared utility functions ───

/// Convert HTML to plain text by stripping tags and decoding entities
pub fn html_to_text(html: &str) -> String {
    let text = html
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#8211;", "\u{2013}")
        .replace("&#8212;", "\u{2014}")
        .replace("&#8217;", "\u{2019}")
        .replace("&#8220;", "\u{201c}")
        .replace("&#8221;", "\u{201d}")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</li>", "\n")
        .replace("</div>", "\n");

    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let stripped = tag_re.replace_all(&text, "");

    stripped.trim().to_string()
}

/// Extract the first meaningful image URL from HTML content.
/// Filters out emojis, gravatars, tracking pixels, badges, icons, etc.
pub fn extract_first_image(html: &str, strict_types: bool) -> Option<String> {
    let re = Regex::new(r#"<img[^>]+src="([^"]+)"[^>]*>"#).ok()?;

    for cap in re.captures_iter(html) {
        if let Some(url) = cap.get(1) {
            let src = url.as_str();

            if src.contains("emoji")
                || src.contains("smilies")
                || src.contains("gravatar")
                || src.contains("wp-includes")
                || src.contains("feeds.feedburner")
                || src.contains("pixel")
                || src.contains("badge")
                || src.contains("button")
                || src.contains("banner")
                || src.contains("icon")
                || src.ends_with(".gif")
                || src.contains("1x1")
                || src.contains("counter")
            {
                continue;
            }

            if src.starts_with("http") {
                if strict_types {
                    if src.contains(".jpg") || src.contains(".jpeg") || src.contains(".png") || src.contains(".webp") || src.contains("wp-content/uploads") {
                        return Some(src.to_string());
                    }
                } else {
                    return Some(src.to_string());
                }
            }
        }
    }
    None
}

/// Extract all meaningful image URLs from HTML content, deduplicated.
pub fn extract_all_images(html: &str, strict_types: bool) -> Vec<String> {
    let re = match Regex::new(r#"<img[^>]+src="([^"]+)"[^>]*>"#) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut images = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for cap in re.captures_iter(html) {
        if let Some(url) = cap.get(1) {
            let src = url.as_str();

            if src.contains("emoji")
                || src.contains("smilies")
                || src.contains("gravatar")
                || src.contains("wp-includes")
                || src.contains("feeds.feedburner")
                || src.contains("pixel")
                || src.contains("badge")
                || src.contains("button")
                || src.contains("banner")
                || src.contains("icon")
                || src.ends_with(".gif")
                || src.contains("1x1")
                || src.contains("counter")
            {
                continue;
            }

            if src.starts_with("http") {
                let include = if strict_types {
                    src.contains(".jpg") || src.contains(".jpeg") || src.contains(".png") || src.contains(".webp") || src.contains("wp-content/uploads")
                } else {
                    true
                };

                if include && seen.insert(src.to_string()) {
                    images.push(src.to_string());
                }
            }
        }
    }
    images
}

/// Extract a field value from text using a regex pattern with a capture group
pub fn extract_field(text: &str, pattern: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Update metadata counters in the shared progress state
pub async fn update_metadata_counts(
    progress: &Arc<RwLock<ScrapeProgress>>,
    games: &[ScrapedGame],
    posts_without_link: i64,
) {
    let with_thumbnail = games.iter().filter(|g| g.thumbnail_url.is_some()).count() as i64;
    let with_genres = games.iter().filter(|g| g.genres.is_some()).count() as i64;
    let with_company = games.iter().filter(|g| g.company.is_some()).count() as i64;
    let with_original_size = games.iter().filter(|g| g.original_size.is_some()).count() as i64;

    let mut p = progress.write().await;
    p.with_thumbnail = with_thumbnail;
    p.with_genres = with_genres;
    p.with_company = with_company;
    p.with_original_size = with_original_size;
    p.magnets_found = games.len() as i64;
    p.posts_without_magnet = posts_without_link;
}
