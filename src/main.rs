#![allow(dead_code)]

use anyhow::{anyhow, bail, Context};
use clap::{Parser, Subcommand};
use error_chain::error_chain;
use log::LevelFilter;
use reqwest::header::{HeaderMap, ACCEPT};
use reqwest::multipart::Part;
use reqwest::StatusCode;
use serde::Deserialize;
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
    /// Show only match scores and errors.
    #[clap(short, long)]
    quiet: bool,
    #[clap(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new().init().unwrap();

    let args = Args::parse();

    log::set_max_level(if args.quiet {
        LevelFilter::Error
    } else {
        LevelFilter::Info
    });

    log::info!("Start application");

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
            Ok(())
        }
        Commands::Query { file } => {
            let results = query_track(file)
                .await
                .context("Failed to query track {file}")?;

            log::info!("{results:?}");

            for result in &results {
                println!(
                    "{:0.3}",
                    result
                        .audio
                        .as_ref()
                        .and_then(|m| m.coverage.query_coverage)
                        .unwrap_or_default()
                )
            }

            if results.is_empty() {
                bail!("No results")
            } else {
                Ok(())
            }
        }
    }
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

    let uuid = Uuid::new_v4();

    let track_id = format!("{uuid} {meta}");

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
        .text("meta", meta) // Ignored by emysound.
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

async fn query_track(path: PathBuf) -> anyhow::Result<Vec<QueryResult>> {
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
            res.json().await.context("Decode response body failed")
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResult {
    /// Unique ID for a query match. You can use this ID to search for query matches in Emy /api/v1/matches endpoint.
    id: String,
    /// Object containing track information.
    track: TrackInfo,
    /// Query match object.
    audio: Option<AudioMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackInfo {
    /// Track unique identifier.
    id: String,
    /// Track title.
    title: Option<String>,
    /// Track artist.
    artist: Option<String>,
    /// Audio track length, measured in seconds.
    #[serde(rename = "audioTrackLength")]
    length: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioMatch {
    /// Query match unique identifier.
    #[serde(rename = "queryMatchId")]
    id: String,
    /// Object containing information about query match coverage.
    coverage: AudioCoverage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioCoverage {
    /// Query match starting position in seconds.
    query_match_starts_at: f32,
    /// Track match starting position in seconds.
    track_match_starts_at: f32,
    /// Gets relative query coverage, calculated by dividing QueryCoverageLength by QueryLength.
    query_coverage: Option<f32>,
    /// Gets relative track coverage, calculated by dividing TrackCoverageLength by TrackLength.
    track_coverage: Option<f32>,
    /// Query coverage length in seconds. Shows how many seconds from the query have been covered in the track.
    query_coverage_length: f32,
    /// Track coverage length in seconds. Shows how many seconds form the track have been covered in the query.
    track_coverage_length: f32,
    /// Discrete query coverage length in seconds. It is calculated by summing QueryCoverageLength with QueryGaps.
    query_discrete_coverage_length: f32,
    /// Discrete track coverage length in seconds. It is calculated by summing TrackCoverageLength with TrackGaps.
    track_discrete_coverage_length: f32,
    /// Query length in seconds.
    query_length: f32,
    /// Track length in seconds.
    track_length: f32,
    /// List of identified gaps in the query.
    query_gaps: Vec<Gap>,
    /// List of identified gaps in the track.
    track_gaps: Vec<Gap>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gap {
    /// Starting position of the gap in seconds.
    start: f32,
    /// Ending position of the gap in seconds.
    end: f32,
    /// Value indicating whether the gap is on the very beginning or very end.
    is_on_edge: bool,
    /// Gets length in seconds calculated by the difference: End - Start.
    length_in_seconds: f32,
}

#[cfg(test)]
mod tests {
    use crate::QueryResult;

    #[test]
    fn test_json_deserialization() {
        let json_input = r#"
[
  {
    "id": "0a1fb0f8-286b-47ed-a19b-457bfbc94995",
    "track": {
      "id": "1ed04cbe-9a68-5fa2-b16d-6b34dd8af37e full",
      "title": "Main theme",
      "artist": "Gravity Falls",
      "metaFields": {},
      "mediaType": "Audio",
      "audioTrackLength": 39.52,
      "videoTrackLength": 0,
      "insertDate": "2022-05-09T15:55:07.2682095Z",
      "lastModifiedTime": "2022-05-09T15:55:07.2684121Z",
      "originalPlaybackUrl": ""
    },
    "matchedAt": "2022-05-09T16:59:21.2074404Z",
    "audio": {
      "queryMatchId": "e60c4030-a866-43bb-9d6a-1a8d1b04b8fe",
      "matchedAt": "2022-05-09T16:59:21.2074404Z",
      "coverage": {
        "queryMatchStartsAt": 0,
        "trackMatchStartsAt": 0,
        "queryCoverage": 0.9179656258704154,
        "trackCoverage": 0.11515599650712359,
        "queryCoverageLength": 4.551524048446744,
        "trackCoverageLength": 4.551524048446744,
        "queryDiscreteCoverageLength": 4.551524048446744,
        "trackDiscreteCoverageLength": 4.551524048446744,
        "queryLength": 4.9582728592162555,
        "trackLength": 39.524854862119014,
        "queryGaps": [],
        "trackGaps": [
          {
            "start": 4.551524048446744,
            "end": 39.524854862119014,
            "isOnEdge": true,
            "lengthInSeconds": 34.97333081367227
          }
        ]
      }
    },
    "streamId": "",
    "reviewStatus": "None",
    "registrationTime": "2022-05-09T16:59:26.1843459Z"
  }
]"#;
        let result: Vec<QueryResult> = serde_json::from_str(json_input).unwrap();
        println!("{result:?}");
        assert_eq!("0a1fb0f8-286b-47ed-a19b-457bfbc94995", result[0].id);
    }
}
