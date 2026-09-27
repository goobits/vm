use std::fmt::Write;

use axum::{
    http::{header::CONTENT_SECURITY_POLICY, HeaderValue},
    response::{Html, IntoResponse, Response},
};

pub(super) fn response(html: String) -> Response {
    let mut response = Html(html).into_response();
    // Remote indexes remain markup, but must never gain executable authority on
    // the registry origin. Package clients consume links without running scripts.
    response.headers_mut().insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; form-action 'none'; sandbox",
        ),
    );
    response
}

pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

pub(super) fn artifact_link(filename: &str, hash: &str) -> String {
    let href = escape(&format!(
        "../../packages/{}#sha256={}",
        path_segment(filename),
        path_segment(hash)
    ));
    let filename = escape(filename);
    format!(r#"    <a href="{href}">{filename}</a><br/>"#)
}
