#![allow(dead_code)]

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use log::LevelFilter;
use std::path::PathBuf;

use emycloud_client_rs::{insert, query, MediaSource};

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
    let args = Args::parse();

    simplelog::WriteLogger::init(
        LevelFilter::Info,
        simplelog::Config::default(),
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("emysound-client.log")?,
    )?;

    match args.command {
        Commands::Insert {
            file,
            artist,
            title,
        } => {
            match insert(MediaSource::File(file.as_path()), artist, title)
                .await
                .context("Failed to insert track {file}")
            {
                Ok(id) => {
                    println!("{id}");
                    Ok(())
                }
                Err(e) => {
                    log::error!("Failed to insert track {e}");
                    Err(e)
                }
            }
        }

        Commands::Query { file } => {
            match query(MediaSource::File(file.as_path()))
                .await
                .context(format!("Failed to query track {:?}", file))
            {
                Ok(results) => {
                    log::debug!("{results:?}");

                    // results.iter().sort
                    for result in &results {
                        println!(
                            "{:0.3} {}",
                            result
                                .audio
                                .as_ref()
                                .and_then(|m| m.coverage.query_coverage)
                                .unwrap_or_default(),
                            result.track.id
                        )
                    }
                    if results.is_empty() {
                        log::info!("No results.");
                        Err(anyhow!("No results"))
                    } else {
                        Ok(())
                    }
                }
                Err(e) => {
                    log::error!("{e}");
                    Err(e)
                }
            }
        }
    }
}
