use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ScrapedGame {
    pub title: String,
    pub file_size: String,
    pub magnet_link: String,
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>, // comma-separated URLs
    pub source_url: Option<String>,
    pub post_date: Option<String>,
}

/// Shared progress state
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrapeProgress {
    pub phase: String,
    pub pages_found: i64,
    pub games_scraped: i64,
    pub games_total: i64,
    pub progress: f64,
    pub message: String,
    // Metadata counters
    pub with_thumbnail: i64,
    pub with_genres: i64,
    pub with_company: i64,
    pub with_original_size: i64,
    pub magnets_found: i64,
    pub posts_without_magnet: i64,
}

impl Default for ScrapeProgress {
    fn default() -> Self {
        Self {
            phase: "fetching_pages".to_string(),
            pages_found: 0,
            games_scraped: 0,
            games_total: 0,
            progress: 0.0,
            message: "Starting...".to_string(),
            with_thumbnail: 0,
            with_genres: 0,
            with_company: 0,
            with_original_size: 0,
            magnets_found: 0,
            posts_without_magnet: 0,
        }
    }
}

// ─── WordPress REST API response types ───

#[derive(Debug, Deserialize)]
struct WpPost {
    id: i64,
    date: Option<String>,
    link: Option<String>,
    title: WpRendered,
    content: WpRendered,
    #[serde(rename = "_embedded")]
    embedded: Option<WpEmbedded>,
}

#[derive(Debug, Deserialize)]
struct WpRendered {
    rendered: String,
}

#[derive(Debug, Deserialize)]
struct WpEmbedded {
    #[serde(rename = "wp:featuredmedia")]
    featured_media: Option<Vec<WpMedia>>,
}

#[derive(Debug, Deserialize)]
struct WpMedia {
    source_url: Option<String>,
    media_details: Option<WpMediaDetails>,
}

#[derive(Debug, Deserialize)]
struct WpMediaDetails {
    sizes: Option<WpMediaSizes>,
}

#[derive(Debug, Deserialize)]
struct WpMediaSizes {
    medium: Option<WpMediaSize>,
    thumbnail: Option<WpMediaSize>,
    medium_large: Option<WpMediaSize>,
}

#[derive(Debug, Deserialize)]
struct WpMediaSize {
    source_url: Option<String>,
}

// ─── Public API ───

pub async fn scrape_all_games() -> Result<Vec<ScrapedGame>, Box<dyn std::error::Error>> {
    let progress = Arc::new(RwLock::new(ScrapeProgress::default()));
    scrape_all_games_with_progress(progress).await
}

pub async fn scrape_all_games_with_progress(
    progress: Arc<RwLock<ScrapeProgress>>,
) -> Result<Vec<ScrapedGame>, Box<dyn std::error::Error>> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(60))
        .build()?;

    let base_url = "https://fitgirl-repacks.site/wp-json/wp/v2/posts";
    let per_page = 100; // Max allowed by WP REST API

    // Phase 1: Discover total pages by fetching the first page
    {
        let mut p = progress.write().await;
        p.phase = "fetching_pages".to_string();
        p.message = "Connecting to WordPress API...".to_string();
        p.progress = 0.0;
    }

    let first_url = format!("{}?per_page={}&page=1&_embed=wp:featuredmedia&_fields=id,date,link,title,content,_embedded", base_url, per_page);
    let first_response = client.get(&first_url).send().await?;

    // Get total pages from X-WP-TotalPages header
    let total_pages: i64 = first_response
        .headers()
        .get("X-WP-TotalPages")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    let total_posts: i64 = first_response
        .headers()
        .get("X-WP-Total")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    println!("WP API reports {} total posts across {} pages", total_posts, total_pages);

    let first_posts: Vec<WpPost> = first_response.json().await?;

    {
        let mut p = progress.write().await;
        p.phase = "scraping_games".to_string();
        p.pages_found = total_pages;
        p.games_total = total_posts;
        p.games_scraped = first_posts.len() as i64;
        p.progress = 2.0;
        p.message = format!("Fetching posts (page 1/{})...", total_pages);
    }

    // Parse first page
    let mut all_games: Vec<ScrapedGame> = Vec::new();
    let mut posts_without_magnet: i64 = 0;
    for post in &first_posts {
        if let Some(game) = parse_wp_post(post) {
            all_games.push(game);
        } else {
            posts_without_magnet += 1;
        }
    }
    update_metadata_counts(&progress, &all_games, posts_without_magnet).await;

    // Phase 2: Fetch remaining pages
    // Process in batches to avoid overwhelming the server
    let batch_size = 5;
    let mut current_page: i64 = 2;

    while current_page <= total_pages {
        let end_page = std::cmp::min(current_page + batch_size - 1, total_pages);
        let mut handles = Vec::new();

        for page_num in current_page..=end_page {
            let client = client.clone();
            let url = format!(
                "{}?per_page={}&page={}&_embed=wp:featuredmedia&_fields=id,date,link,title,content,_embedded",
                base_url, per_page, page_num
            );

            handles.push(tokio::spawn(async move {
                match client.get(&url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            match resp.json::<Vec<WpPost>>().await {
                                Ok(posts) => Some((page_num, posts)),
                                Err(e) => {
                                    eprintln!("  Failed to parse page {}: {}", page_num, e);
                                    None
                                }
                            }
                        } else {
                            // 400 status usually means we've gone past the last page
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("  Failed to fetch page {}: {}", page_num, e);
                        None
                    }
                }
            }));
        }

        for handle in handles {
            if let Ok(Some((_page_num, posts))) = handle.await {
                for post in &posts {
                    if let Some(game) = parse_wp_post(post) {
                        all_games.push(game);
                    } else {
                        posts_without_magnet += 1;
                    }
                }
            }
        }

        // Update progress with metadata counts
        update_metadata_counts(&progress, &all_games, posts_without_magnet).await;
        {
            let mut p = progress.write().await;
            let pct = 2.0 + (end_page as f64 / total_pages as f64) * 88.0;
            p.games_scraped = all_games.len() as i64;
            p.progress = pct;
            p.message = format!(
                "Page {}/{} — {} games | 🖼 {} images | 🏷 {} genres | 🏢 {} companies",
                end_page, total_pages, all_games.len(),
                p.with_thumbnail, p.with_genres, p.with_company
            );
        }

        if (end_page % 10) == 0 || end_page == total_pages {
            let p = progress.read().await;
            println!(
                "  Page {}/{} — {} games | {} thumbnails | {} genres | {} companies | {} original sizes | {} skipped (no magnet)",
                end_page, total_pages, all_games.len(),
                p.with_thumbnail, p.with_genres, p.with_company, p.with_original_size,
                posts_without_magnet
            );
        }

        current_page = end_page + 1;

        // Small delay between batches to be nice to the server
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Final validation
    let valid_games: Vec<ScrapedGame> = all_games
        .into_iter()
        .filter(|g| validate_magnet(&g.magnet_link))
        .collect();

    update_metadata_counts(&progress, &valid_games, posts_without_magnet).await;
    {
        let mut p = progress.write().await;
        p.phase = "done".to_string();
        p.games_scraped = valid_games.len() as i64;
        p.progress = 95.0;
        p.message = format!(
            "Saving {} games — 🖼 {} images | 🏷 {} genres | 🏢 {} companies | 📏 {} sizes",
            valid_games.len(), p.with_thumbnail, p.with_genres, p.with_company, p.with_original_size
        );
    }

    println!("Scrape complete: {} valid games with magnets", valid_games.len());
    {
        let p = progress.read().await;
        println!(
            "  Metadata: {} thumbnails, {} genres, {} companies, {} original sizes, {} posts skipped",
            p.with_thumbnail, p.with_genres, p.with_company, p.with_original_size, p.posts_without_magnet
        );
    }
    Ok(valid_games)
}

// ─── Post parsing ───

/// Recompute metadata counters from the current games list
async fn update_metadata_counts(
    progress: &Arc<RwLock<ScrapeProgress>>,
    games: &[ScrapedGame],
    posts_without_magnet: i64,
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
    p.posts_without_magnet = posts_without_magnet;
}

fn parse_wp_post(post: &WpPost) -> Option<ScrapedGame> {
    let title = html_to_text(&post.title.rendered);
    if title.is_empty() {
        return None;
    }

    let content_html = &post.content.rendered;
    let content_text = html_to_text(content_html);

    // Extract magnet link from the HTML content
    let magnet = extract_magnet(content_html)?;

    // Extract metadata from the text content
    let file_size = extract_field(&content_text, r"(?i)(?:repack\s+size)\s*[:\s]\s*(.+?)(?:\n|$)")
        .unwrap_or_else(|| "N/A".to_string());

    let original_size = extract_field(&content_text, r"(?i)(?:original\s+size)\s*[:\s]\s*(.+?)(?:\n|$)");

    let genres = extract_field(&content_text, r"(?i)(?:genres?\s*/?\s*tags?)\s*[:\s]\s*(.+?)(?:\n|$)")
        .map(|g| g.trim_end_matches(|c: char| c == '.' || c == ',').to_string());

    let company = extract_field(&content_text, r"(?i)(?:compan(?:y|ies))\s*[:\s]\s*(.+?)(?:\n|$)")
        .map(|c| c.trim_end_matches(|c: char| c == '.' || c == ',').to_string());

    // Get thumbnail URL — try multiple sources:
    // 1. First <img> in the post content (most reliable — FitGirl always embeds screenshots)
    // 2. Embedded featured media from WP API
    let content_img = extract_first_image(content_html);

    let featured_img = post.embedded.as_ref()
        .and_then(|e| e.featured_media.as_ref())
        .and_then(|media| media.first())
        .and_then(|m| {
            m.media_details.as_ref()
                .and_then(|d| d.sizes.as_ref())
                .and_then(|s| {
                    s.medium.as_ref().and_then(|ms| ms.source_url.clone())
                        .or_else(|| s.medium_large.as_ref().and_then(|ms| ms.source_url.clone()))
                        .or_else(|| s.thumbnail.as_ref().and_then(|ms| ms.source_url.clone()))
                })
                .or_else(|| m.source_url.clone())
        });

    let thumbnail_url = content_img.or(featured_img);

    // Extract all screenshot URLs from content
    let screenshots = extract_all_images(content_html);
    let screenshots = if screenshots.is_empty() {
        None
    } else {
        Some(screenshots.join("|||"))
    };

    let source_url = post.link.clone();
    let post_date = post.date.clone();

    Some(ScrapedGame {
        title,
        file_size,
        magnet_link: magnet,
        genres,
        company,
        original_size,
        thumbnail_url,
        screenshots,
        source_url,
        post_date,
    })
}

/// Extract a magnet link from HTML content
fn extract_magnet(html: &str) -> Option<String> {
    let re = Regex::new(r#"href="(magnet:\?xt=urn:btih:[^"]+)""#).ok()?;
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract the first meaningful image URL from HTML content.
/// Skips tiny images (icons/badges), tracking pixels, and emoji.
fn extract_first_image(html: &str) -> Option<String> {
    let re = Regex::new(r#"<img[^>]+src="([^"]+)"[^>]*>"#).ok()?;

    for cap in re.captures_iter(html) {
        if let Some(url) = cap.get(1) {
            let src = url.as_str();

            // Skip common non-screenshot images
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

            // Must be an actual image URL
            if src.starts_with("http") && (src.contains(".jpg") || src.contains(".jpeg") || src.contains(".png") || src.contains(".webp") || src.contains("wp-content/uploads")) {
                return Some(src.to_string());
            }
        }
    }
    None
}

/// Extract ALL meaningful image URLs from HTML content (for screenshot gallery).
fn extract_all_images(html: &str) -> Vec<String> {
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

            if src.starts_with("http")
                && (src.contains(".jpg") || src.contains(".jpeg") || src.contains(".png") || src.contains(".webp") || src.contains("wp-content/uploads"))
            {
                if seen.insert(src.to_string()) {
                    images.push(src.to_string());
                }
            }
        }
    }
    images
}

/// Extract a field value using a regex pattern against plain text
fn extract_field(text: &str, pattern: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Simple HTML to text conversion - strips tags
fn html_to_text(html: &str) -> String {
    // Decode common HTML entities first
    let text = html
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#8211;", "–")
        .replace("&#8212;", "—")
        .replace("&#8217;", "'")
        .replace("&#8220;", "\u{201c}")
        .replace("&#8221;", "\u{201d}")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</li>", "\n")
        .replace("</div>", "\n");

    // Strip remaining HTML tags
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let stripped = tag_re.replace_all(&text, "");

    stripped.trim().to_string()
}

fn validate_magnet(link: &str) -> bool {
    let magnet_regex = Regex::new(r"^magnet:\?xt=urn:btih:[a-fA-F0-9]{40}").unwrap();
    magnet_regex.is_match(link)
}
