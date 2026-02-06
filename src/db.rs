use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, sqlite::SqlitePoolOptions};

// ─── Download-related row types ───

#[derive(Debug, Clone, FromRow)]
pub struct DownloadRow {
    pub id: i64,
    pub game_id: i64,
    pub status: String,
    pub progress: f64,
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub file_path: Option<String>,
    pub installer_path: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub game_title: String,
    pub game_size: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct DownloadFileRow {
    pub id: i64,
    pub filename: String,
    pub file_size: Option<i64>,
    pub file_path: Option<String>,
    pub is_extracted: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Game {
    pub id: i64,
    pub title: String,
    pub file_size: String,
    pub magnet_link: String,
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>,
    pub source_url: Option<String>,
    pub post_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub search: Option<String>,
    pub sort: Option<String>,
    pub genre: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Initialize the database connection pool and run migrations.
pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Create tables if they don't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS games (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            file_size TEXT NOT NULL DEFAULT '',
            magnet_link TEXT NOT NULL,
            genres TEXT,
            company TEXT,
            original_size TEXT,
            thumbnail_url TEXT,
            screenshots TEXT,
            source_url TEXT,
            post_date TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migrations for existing DBs - add new columns if they don't exist
    for col in &["genres", "company", "original_size", "thumbnail_url", "source_url", "post_date", "screenshots"] {
        let _ = sqlx::query(&format!("ALTER TABLE games ADD COLUMN {} TEXT", col))
            .execute(&pool)
            .await;
    }

    // Add index for search performance
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_games_title ON games(title COLLATE NOCASE)"
    )
    .execute(&pool)
    .await?;

    // Download management tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            game_id INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'queued',
            progress REAL DEFAULT 0.0,
            download_speed TEXT,
            eta TEXT,
            file_path TEXT,
            installer_path TEXT,
            error_message TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            FOREIGN KEY (game_id) REFERENCES games(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migration: add installer_path column if it doesn't exist (for existing DBs)
    let _ = sqlx::query("ALTER TABLE downloads ADD COLUMN installer_path TEXT")
        .execute(&pool)
        .await;

    // Settings key-value table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS download_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            download_id INTEGER NOT NULL,
            filename TEXT NOT NULL,
            file_size INTEGER,
            file_path TEXT,
            is_extracted BOOLEAN DEFAULT 0,
            FOREIGN KEY (download_id) REFERENCES downloads(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// Query games with search, sort, and pagination.
pub async fn query_games(
    pool: &SqlitePool,
    query: GameQuery,
) -> Result<(Vec<Game>, i64), sqlx::Error> {
    let per_page = query.per_page.unwrap_or(50);
    let page = query.page.unwrap_or(1);
    let offset = (page - 1) * per_page;

    let search_pattern = query
        .search
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s));

    let genre_pattern = query
        .genre
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s));

    let order_clause = match query.sort.as_deref() {
        Some("title_asc") => "title COLLATE NOCASE ASC",
        Some("title_desc") => "title COLLATE NOCASE DESC",
        Some("size_asc") => "file_size ASC",
        Some("size_desc") => "file_size DESC",
        Some("date_asc") => "COALESCE(post_date, '') ASC, id ASC",
        Some("date_desc") => "COALESCE(post_date, '') DESC, id DESC",
        _ => "id DESC",
    };

    // Build WHERE clauses dynamically
    let mut conditions: Vec<String> = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref pattern) = search_pattern {
        conditions.push("title LIKE ?".to_string());
        bind_values.push(pattern.clone());
    }

    if let Some(ref pattern) = genre_pattern {
        conditions.push("genres LIKE ?".to_string());
        bind_values.push(pattern.clone());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total matching rows
    let count_sql = format!("SELECT COUNT(*) FROM games {}", where_clause);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for val in &bind_values {
        count_query = count_query.bind(val);
    }
    let total: i64 = count_query.fetch_one(pool).await?;

    // Fetch page of results
    let select_sql = format!(
        "SELECT id, title, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date FROM games {} ORDER BY {} LIMIT ? OFFSET ?",
        where_clause, order_clause
    );
    let mut select_query = sqlx::query_as::<_, Game>(&select_sql);
    for val in &bind_values {
        select_query = select_query.bind(val);
    }
    let games = select_query
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok((games, total))
}

/// Get all unique genres from the database, split by comma.
pub async fn get_all_genres(pool: &SqlitePool) -> Result<Vec<(String, i64)>, sqlx::Error> {
    // Get all genre strings
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT genres FROM games WHERE genres IS NOT NULL AND genres != ''"
    )
    .fetch_all(pool)
    .await?;

    // Split by comma, count occurrences
    let mut genre_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (genres_str,) in rows {
        for genre in genres_str.split(',') {
            let trimmed = genre.trim().to_string();
            if !trimmed.is_empty() {
                *genre_counts.entry(trimmed).or_insert(0) += 1;
            }
        }
    }

    // Sort by count descending
    let mut genres: Vec<(String, i64)> = genre_counts.into_iter().collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(genres)
}

/// Get a random game
pub async fn get_random_game(pool: &SqlitePool) -> Result<Game, sqlx::Error> {
    sqlx::query_as::<_, Game>(
        "SELECT id, title, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date FROM games ORDER BY RANDOM() LIMIT 1"
    )
    .fetch_one(pool)
    .await
}

/// Get a single game by ID.
pub async fn get_game_by_id(pool: &SqlitePool, id: i64) -> Result<Game, sqlx::Error> {
    sqlx::query_as::<_, Game>(
        "SELECT id, title, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date FROM games WHERE id = ?"
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

/// A game record ready for insertion
pub struct GameInsert {
    pub title: String,
    pub file_size: String,
    pub magnet_link: String,
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>,
    pub source_url: Option<String>,
    pub post_date: Option<String>,
}

/// Atomically replace all games in a single transaction.
/// Deletes existing games and inserts new ones; rolls back on failure.
pub async fn replace_all_games(
    pool: &SqlitePool,
    games: Vec<GameInsert>,
) -> Result<usize, sqlx::Error> {
    let count = games.len();
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM games")
        .execute(&mut *tx)
        .await?;

    for g in &games {
        sqlx::query(
            "INSERT INTO games (title, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&g.title)
            .bind(&g.file_size)
            .bind(&g.magnet_link)
            .bind(&g.genres)
            .bind(&g.company)
            .bind(&g.original_size)
            .bind(&g.thumbnail_url)
            .bind(&g.screenshots)
            .bind(&g.source_url)
            .bind(&g.post_date)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(count)
}

/// Clear all games from the database.
#[allow(dead_code)]
pub async fn clear_games(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM games").execute(pool).await?;
    Ok(())
}

/// Insert games without clearing first. Returns count inserted.
#[allow(dead_code)]
pub async fn insert_games(
    pool: &SqlitePool,
    games: Vec<GameInsert>,
) -> Result<usize, sqlx::Error> {
    let count = games.len();

    for g in &games {
        sqlx::query(
            "INSERT INTO games (title, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&g.title)
            .bind(&g.file_size)
            .bind(&g.magnet_link)
            .bind(&g.genres)
            .bind(&g.company)
            .bind(&g.original_size)
            .bind(&g.thumbnail_url)
            .bind(&g.screenshots)
            .bind(&g.source_url)
            .bind(&g.post_date)
            .execute(pool)
            .await?;
    }

    Ok(count)
}

// ─── Settings ───

/// Get a setting value by key. Returns None if not found.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM settings WHERE key = ?"
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(v,)| v))
}

/// Set a setting value (upsert).
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value"
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a setting by key.
pub async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get all settings as key-value pairs.
pub async fn get_all_settings(pool: &SqlitePool) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM settings ORDER BY key"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
