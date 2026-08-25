use std::time::Instant;

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use tracing::Instrument;
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Copy)]
pub struct HttpLogContext {
    component: &'static str,
}

impl HttpLogContext {
    pub const fn new(component: &'static str) -> Self {
        Self { component }
    }
}

pub async fn request_context(
    axum::extract::State(context): axum::extract::State<HttpLogContext>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_request_id(value))
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let request_id_header = HeaderValue::from_str(&request_id)
        .expect("validated or generated request IDs are valid headers");
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_header.clone());

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let span = tracing::info_span!(
        "http_request",
        component = context.component,
        request_id = %request_id,
        method = %method,
        path = %path
    );
    async move {
        let started = Instant::now();
        let mut response = next.run(request).await;
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER, request_id_header);
        tracing::debug!(
            operation = "http_request",
            status_code = response.status().as_u16(),
            duration_ms = duration_millis(started),
            "HTTP request completed"
        );
        response
    }
    .instrument(span)
    .await
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn duration_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::valid_request_id;

    #[test]
    fn request_ids_are_bounded_and_header_safe() {
        assert!(valid_request_id("request-123:child_4"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("request id"));
        assert!(!valid_request_id(&"x".repeat(129)));
    }
}
