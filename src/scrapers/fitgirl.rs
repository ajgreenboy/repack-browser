use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::{GameScraper, LinkType, ScrapedGame, ScrapeProgress};
use super::utils::{self, WpPost};

pub struct FitGirlScraper {
    client: Client,
}

impl FitGirlScraper {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

#[async_trait]
impl GameScraper for FitGirlScraper {
    async fn scrape_all_games(
        &self,
        progress: Arc<RwLock<ScrapeProgress>>
    ) -> Result<Vec<ScrapedGame>, Box<dyn std::error::Error>> {
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
        let first_response = self.client.get(&first_url).send().await?;

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
        utils::update_metadata_counts(&progress, &all_games, posts_without_magnet).await;

        // Phase 2: Fetch remaining pages
        let batch_size = 5;
        let mut current_page: i64 = 2;

        while current_page <= total_pages {
            let end_page = std::cmp::min(current_page + batch_size - 1, total_pages);
            let mut handles = Vec::new();

            for page_num in current_page..=end_page {
                let client = self.client.clone();
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
            utils::update_metadata_counts(&progress, &all_games, posts_without_magnet).await;
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
            .filter(|g| validate_magnet(&g.download_link))
            .collect();

        utils::update_metadata_counts(&progress, &valid_games, posts_without_magnet).await;
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

    fn source_name(&self) -> &'static str {
        "fitgirl"
    }

    fn source_label(&self) -> &'static str {
        "FitGirl Repacks"
    }
}

// ─── Post parsing ───

fn parse_wp_post(post: &WpPost) -> Option<ScrapedGame> {
    let title = utils::html_to_text(&post.title.rendered);
    if title.is_empty() {
        return None;
    }

    let content_html = &post.content.rendered;
    let content_text = utils::html_to_text(content_html);

    // Extract magnet link from the HTML content
    let magnet = extract_magnet(content_html)?;

    // Extract metadata from the text content
    let file_size = utils::extract_field(&content_text, r"(?i)(?:repack\s+size)\s*[:\s]\s*(.+?)(?:\n|$)")
        .unwrap_or_else(|| "N/A".to_string());

    let original_size = utils::extract_field(&content_text, r"(?i)(?:original\s+size)\s*[:\s]\s*(.+?)(?:\n|$)");

    let genres = utils::extract_field(&content_text, r"(?i)(?:genres?\s*/?\s*tags?)\s*[:\s]\s*(.+?)(?:\n|$)")
        .map(|g| g.trim_end_matches(|c: char| c == '.' || c == ',').to_string());

    let company = utils::extract_field(&content_text, r"(?i)(?:compan(?:y|ies))\s*[:\s]\s*(.+?)(?:\n|$)")
        .map(|c| c.trim_end_matches(|c: char| c == '.' || c == ',').to_string());

    // Get thumbnail URL (strict_types=true for FitGirl to avoid junk images)
    let content_img = utils::extract_first_image(content_html, true);

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
    let screenshots = utils::extract_all_images(content_html, true);
    let screenshots = if screenshots.is_empty() {
        None
    } else {
        Some(screenshots.join("|||"))
    };

    let source_url = post.link.clone();
    let post_date = post.date.clone();

    Some(ScrapedGame {
        title,
        source: "fitgirl".to_string(),
        file_size,
        download_link: magnet,
        link_type: LinkType::Magnet,
        genres,
        company,
        original_size,
        thumbnail_url,
        screenshots,
        source_url,
        post_date,
    })
}

fn extract_magnet(html: &str) -> Option<String> {
    let re = Regex::new(r#"href="(magnet:\?xt=urn:btih:[^"]+)""#).ok()?;
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn validate_magnet(link: &str) -> bool {
    let magnet_regex = Regex::new(r"^magnet:\?xt=urn:btih:[a-fA-F0-9]{40}").unwrap();
    magnet_regex.is_match(link)
}
