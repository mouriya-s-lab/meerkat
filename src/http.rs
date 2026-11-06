//! JSON over NSH's `fetch`, with errors as the person reading the
//! dashboard needs them: the URL, the status and the server's own message.

use nsh_core::prelude::{HttpRequest, fetch};
use serde::{Deserialize, de::DeserializeOwned};

/// Sends `request` and parses a 2xx body as `T`.
pub(crate) async fn json<T: DeserializeOwned>(request: HttpRequest) -> Result<T, String> {
    let url = request.url.clone();
    let response = fetch(request).await.map_err(|error| format!("{url}: {error}"))?;
    if !response.ok() {
        return Err(format!(
            "{url}: HTTP {}{}",
            response.status(),
            error_message(&response.body)
                .map(|message| format!(" {message}"))
                .unwrap_or_default()
        ));
    }
    serde_json::from_slice(&response.body)
        .map_err(|error| format!("{url}: 响应无法解析：{error}"))
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
