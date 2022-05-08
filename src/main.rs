use anyhow::{anyhow, bail, Context};
use clap::{Parser, Subcommand};
use error_chain::error_chain;
use log::LevelFilter;
use reqwest::header::{HeaderMap, ACCEPT};
use reqwest::multipart::Part;
use reqwest::StatusCode;
use simple_logger::SimpleLogger;
use std::path::{Path, PathBuf};
use uuid::Uuid;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Add tracks to the database.
    Insert {
        /// Track filename.
        #[clap(short, long, parse(from_os_str))]
        file: PathBuf,
        /// Track artist.
        #[clap(short, long)]
        artist: String,
        /// Track title.
        #[clap(short, long)]
        title: String,
        /// Track meta info.
        #[clap(short, long)]
        meta: String,
    },
    /// Query database for similar tracks.
    Query {
        /// Track filename.
        #[clap(short, long, parse(from_os_str))]
        file: PathBuf,
    },
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    log::info!("Start application");

    let args = Args::parse();

    match args.command {
        Commands::Insert {
            file,
            artist,
            title,
            meta,
        } => {
            insert_track(file.as_path(), artist, title, meta)
                .await
                .context("Failed to insert track {file}")?;
            println!("Track inserted successfully.");
        }
        Commands::Query { file } => {
            let results = query_track(file)
                .await
                .context("Failed to query track {file}")?;
            println!("Results: {:?}", results);
        }
    }

    Ok(())
}

async fn insert_track(
    path: &Path,
    artist: String,
    title: String,
    meta: String,
) -> anyhow::Result<uuid::Uuid> {
    log::info!("Inserting track {path:?}");

    let file_name = path
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("Track path is invalid, can't extract the filename"))?;

    log::info!("Track filename: {}", file_name);

    log::info!("Reading track file...");

    let content = tokio::fs::read(&path).await.context("Reading track file")?;
    let content_length = content.len();

    let uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("{artist}:{title}:{meta}").as_bytes(),
    );

    let track_id = format!("{artist} {title} {meta} {uuid}");

    log::info!("Track id: {track_id}");

    let headers = {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, "application/json".parse()?);
        h
    };

    let form = reqwest::multipart::Form::new()
        .text("Id", track_id)
        .text("Artist", artist)
        .text("Title", title)
        .text("MediaType", "Audio")
        .text("meta", meta)
        .part(
            "file",
            Part::stream_with_length(content, content_length as u64)
                .file_name(file_name)
                .mime_str("application/octet-stream")?,
        );

    let client = reqwest::Client::new();

    log::info!("Sending request to EmySound");

    let res = client
        .post("http://localhost:3340/api/v1.1/Tracks")
        .basic_auth("ADMIN", Some(""))
        .headers(headers)
        .multipart(form)
        .send()
        .await?;

    log::info!("Response {}", res.status());

    match res.status() {
        StatusCode::OK => {
            log::info!("Track inserted!");
            Ok(uuid)
        }
        _ => {
            log::info!("Failed to insert track.");
            bail!(
                "Failed to insert track {} {}",
                res.status(),
                res.text().await?
            );
        }
    }
}

async fn query_track(path: PathBuf) -> anyhow::Result<Vec<String>> {
    log::info!("Querying track {path:?}");

    let file_name = path
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("Track path is invalid, can't extract the filename"))?;

    log::info!("Track filename: {}", file_name);

    log::info!("Reading track file...");

    let content = tokio::fs::read(&path).await.context("Reading track file")?;
    let content_length = content.len();

    let headers = {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, "application/json".parse()?);
        h
    };

    let form = reqwest::multipart::Form::new().part(
        "file",
        Part::stream_with_length(content, content_length as u64)
            .file_name(file_name)
            .mime_str("application/octet-stream")?,
    );

    let client = reqwest::Client::new();

    log::info!("Sending request to EmySound");

    let res = client
        .post("http://localhost:3340/api/v1.1/Query")
        .basic_auth("ADMIN", Some(""))
        .headers(headers)
        .query(&[
            ("mediaType", "Audio"),
            ("minConfidence", "0.2"),
            ("minCoverage", "0"),
            ("registerMatches", "true"),
        ])
        .multipart(form)
        .send()
        .await?;

    log::info!("Response {}", res.status());

    match res.status() {
        StatusCode::OK => {
            log::info!("Query succeeded!");
            let results = res.text().await.context("Decode response body failed")?;
            println!("{results}");
            Ok(vec![])
        }
        _ => {
            log::info!("Query failed.");
            bail!(
                "Failed to query track {} {}",
                res.status(),
                res.text().await?
            );
        }
    }
}
