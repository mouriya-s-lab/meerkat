//! JSON over NSH's `fetch`, with errors as the person reading the
//! Quorum needs them: the URL, the status and the server's own message.

use nsh_core::prelude::{HttpRequest, HttpResponse, fetch};
use serde::{Deserialize, de::DeserializeOwned};

/// Sends `request` and parses a 2xx body as `T`.
pub(crate) async fn json<T: DeserializeOwned>(request: HttpRequest) -> Result<T, String> {
    let url = request.url.clone();
    let body = bytes(request).await?;
    serde_json::from_slice(&body).map_err(|error| format!("{url}: 响应无法解析：{error}"))
}

/// Sends `request` and gives a 2xx body as it is.
pub(crate) async fn bytes(request: HttpRequest) -> Result<Vec<u8>, String> {
    let url = request.url.clone();
    let response = send(request).await?;
    if !response.ok() {
        return Err(failure(&url, &response));
    }
    Ok(response.body)
}

/// Sends `request`; any status is a response, only a request that got no
/// answer is an error.
pub(crate) async fn send(request: HttpRequest) -> Result<HttpResponse, String> {
    let url = request.url.clone();
    fetch(request).await.map_err(|error| format!("{url}: {error}"))
}

/// What a response that is not the expected one says: the URL, the status
/// and the server's message.
pub(crate) fn failure(url: &str, response: &HttpResponse) -> String {
    format!(
        "{url}: HTTP {}{}",
        response.status(),
        error_message(&response.body)
            .map(|message| format!(" {message}"))
            .unwrap_or_default()
    )
}

/// The message of an error body: the programs' `{"error","message"}`,
/// GitHub's `{"message"}`, else the first line of the text.
fn error_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct ErrorBody {
        error: Option<String>,
        message: Option<String>,
    }
    if let Ok(ErrorBody { error, message }) = serde_json::from_slice::<ErrorBody>(body) {
        let joined = [error, message]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(": ");
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    let text = String::from_utf8_lossy(body);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(line.chars().take(200).collect())
}
