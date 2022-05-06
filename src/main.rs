use error_chain::error_chain;
use reqwest::header::{HeaderMap, ACCEPT};
use reqwest::multipart::Part;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
    }
}

static FILE: &[u8] = include_bytes!("../testdata/reference.mp3");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, "application/json".parse()?);

    let form = reqwest::multipart::Form::new()
        .text("Id", "id")
        .text("Artist", "artist")
        .text("Title", "title")
        .text("MediaType", "Audio")
        .part(
            "file",
            Part::bytes(FILE)
                .file_name("reference.mp3")
                .mime_str("application/octet-stream")?,
        );

    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:3340/api/v1.1/Tracks")
        .basic_auth("ADMIN", Some(""))
        .headers(headers)
        .multipart(form)
        .send()
        .await?;

    println!("{:?}", res.text().await?);

    Ok(())
}
