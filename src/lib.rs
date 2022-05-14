#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, ACCEPT};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const EMYSOUND_API: &str = "http://localhost:3340/api/v1.1/";

pub async fn insert(path: &Path, artist: String, title: String) -> Result<Uuid> {
    const TARGET: &str = "emysound::insert";

    log::debug!(
        target: TARGET,
        "path={path:?}, artist={artist}, title={title}"
    );

    let file_name = path
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
        .ok_or_else(|| {
            log::error!(
                target: TARGET,
                "Can't extract the filename from path={:?}",
                path
            );
            anyhow!("Track path is invalid, can't extract the filename")
        })?;

    log::debug!(target: TARGET, "Track filename: {}", file_name);

    log::debug!(target: TARGET, "Reading track file...");

    let content = tokio::fs::read(&path).await.context("Reading track file")?;
    let content_length = content.len();

    let track_id = Uuid::new_v4();

    log::debug!(target: TARGET, "Track id: {track_id}");

    let headers = {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, "application/json".parse()?);
        h
    };

    let form = Form::new()
        .text("Id", track_id.to_string())
        .text("Artist", artist)
        .text("Title", title)
        .text("MediaType", "Audio")
        .part(
            "file",
            Part::stream_with_length(content, content_length as u64)
                .file_name(file_name)
                .mime_str("application/octet-stream")
                .context("Preparing form content")?,
        );

    log::debug!("Sending request to EmySound");
    let url: Url = Url::parse(EMYSOUND_API)?;
    let url = url.join("Tracks")?;
    let res = Client::new()
        .post(url)
        .basic_auth("ADMIN", Some(""))
        .headers(headers)
        .multipart(form)
        .send()
        .await?;

    let status = res.status();

    match status {
        StatusCode::OK => Ok(track_id),
        _ => {
            let text = res.text().await?;
            log::error!(target: TARGET, "Failed to insert track {status} {text}");
            Err(anyhow!("Failed to insert track {status} {text}"))
        }
    }
}

pub async fn query(path: PathBuf) -> Result<Vec<QueryResult>> {
    const TARGET: &str = "emysound::query";
    log::debug!(target: TARGET, "path={path:?}");

    let file_name = path
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
        .ok_or_else(|| {
            log::error!(
                target: TARGET,
                "Can't extract the filename from path={:?}",
                path
            );
            anyhow!("Track path is invalid, can't extract the filename")
        })?;

    log::debug!(target: TARGET, "Track filename: {}", file_name);

    log::debug!(target: TARGET, "Reading track file...");

    let content = tokio::fs::read(&path).await.context("Reading track file")?;
    let content_length = content.len();

    let headers = {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, "application/json".parse()?);
        h
    };

    let form = Form::new().part(
        "file",
        Part::stream_with_length(content, content_length as u64)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .context("Preparing form content")?,
    );

    let client = reqwest::Client::new();

    log::debug!(target: TARGET, "Sending request to EmySound");

    let url: Url = Url::parse(EMYSOUND_API)?;
    let url = url.join("Query")?;

    let res = client
        .post(url)
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

    let status = res.status();

    match status {
        StatusCode::OK => res.json().await.context("Decode response body failed"),
        _ => {
            let text = res.text().await?;
            log::error!(target: TARGET, "Failed to query track {status} {text}");
            Err(anyhow!("Failed to query track {status} {text}"))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// Unique ID for a query match. You can use this ID to search for query matches in Emy /api/v1/matches endpoint.
    pub id: String,
    /// Object containing track information.
    pub track: TrackInfo,
    /// Query match object.
    pub audio: Option<AudioMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    /// Track unique identifier.
    pub id: String,
    /// Track title.
    pub title: Option<String>,
    /// Track artist.
    pub artist: Option<String>,
    /// Audio track length, measured in seconds.
    #[serde(rename = "audioTrackLength")]
    pub length: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMatch {
    /// Query match unique identifier.
    #[serde(rename = "queryMatchId")]
    pub id: String,
    /// Object containing information about query match coverage.
    pub coverage: AudioCoverage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioCoverage {
    /// Query match starting position in seconds.
    pub query_match_starts_at: f32,
    /// Track match starting position in seconds.
    pub track_match_starts_at: f32,
    /// Gets relative query coverage, calculated by dividing QueryCoverageLength by QueryLength.
    pub query_coverage: Option<f32>,
    /// Gets relative track coverage, calculated by dividing TrackCoverageLength by TrackLength.
    pub track_coverage: Option<f32>,
    /// Query coverage length in seconds. Shows how many seconds from the query have been covered in the track.
    pub query_coverage_length: f32,
    /// Track coverage length in seconds. Shows how many seconds form the track have been covered in the query.
    pub track_coverage_length: f32,
    /// Discrete query coverage length in seconds. It is calculated by summing QueryCoverageLength with QueryGaps.
    pub query_discrete_coverage_length: f32,
    /// Discrete track coverage length in seconds. It is calculated by summing TrackCoverageLength with TrackGaps.
    pub track_discrete_coverage_length: f32,
    /// Query length in seconds.
    pub query_length: f32,
    /// Track length in seconds.
    pub track_length: f32,
    /// List of identified gaps in the query.
    pub query_gaps: Vec<Gap>,
    /// List of identified gaps in the track.
    pub track_gaps: Vec<Gap>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    /// Starting position of the gap in seconds.
    pub start: f32,
    /// Ending position of the gap in seconds.
    pub end: f32,
    /// Value indicating whether the gap is on the very beginning or very end.
    pub is_on_edge: bool,
    /// Gets length in seconds calculated by the difference: End - Start.
    pub length_in_seconds: f32,
}
