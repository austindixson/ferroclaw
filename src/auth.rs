use crate::config;
use crate::error::{FerroError, Result};
use base64::Engine;
use rand::Rng;
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const OPENAI_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
const OPENAI_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CALLBACK_PORT: u16 = 1455;
const OPENAI_CALLBACK_PATH: &str = "/auth/callback";
const OPENAI_REDIRECT_HOST: &str = "localhost";
const OPENAI_SCOPE: &str = "openid profile email offline_access";

pub fn login_openai_oauth() -> Result<()> {
    println!("OpenAI OAuth login (browser authorize flow)");

    let (access_token, refresh_token, expires_in) = login_openai_oauth_pkce()?;

    upsert_env_var("OPENAI_OAUTH_TOKEN", &access_token)?;

    if let Some(refresh) = refresh_token.as_deref() {
        let _ = upsert_env_var("OPENAI_OAUTH_REFRESH_TOKEN", refresh);
    }
    if let Some(expires_in) = expires_in {
        let _ = upsert_env_var("OPENAI_OAUTH_EXPIRES_IN", &expires_in.to_string());
    }

    println!("Saved OPENAI_OAUTH_TOKEN to {}", env_path().display());
    println!("Ferroclaw will use OPENAI_OAUTH_TOKEN when auth_mode = \"oauth\".");
    Ok(())
}

pub fn logout_openai_oauth() -> Result<()> {
    remove_env_var("OPENAI_OAUTH_TOKEN")?;
    let _ = remove_env_var("OPENAI_OAUTH_REFRESH_TOKEN");
    let _ = remove_env_var("OPENAI_OAUTH_EXPIRES_IN");
    println!("Removed OpenAI OAuth values from {}", env_path().display());
    Ok(())
}

fn login_openai_oauth_pkce() -> Result<(String, Option<String>, Option<u64>)> {
    let listeners = bind_localhost_oauth(OPENAI_CALLBACK_PORT)?;

    let verifier = pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let state = random_hex_32();
    let redirect_uri =
        format!("http://{OPENAI_REDIRECT_HOST}:{OPENAI_CALLBACK_PORT}{OPENAI_CALLBACK_PATH}");

    let mut auth = Url::parse(OPENAI_AUTH_URL)
        .map_err(|e| FerroError::Config(format!("Bad OpenAI auth URL: {e}")))?;
    auth.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CODEX_CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", OPENAI_SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "ferroclaw");

    open_browser(auth.as_str())?;
    println!("Opened browser to authorize OpenAI OAuth.");

    let params = wait_oauth_on_listener(listeners, OPENAI_CALLBACK_PATH, Duration::from_secs(600))?;

    if params.get("state").map(|s| s.as_str()) != Some(state.as_str()) {
        return Err(FerroError::Config("OAuth state mismatch.".into()));
    }

    let code = params
        .get("code")
        .ok_or_else(|| FerroError::Config("Missing authorization code from callback.".into()))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| FerroError::Config(format!("Failed to build HTTP client: {e}")))?;

    let form = [
        ("grant_type", "authorization_code"),
        ("client_id", OPENAI_CODEX_CLIENT_ID),
        ("code", code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    let resp = client
        .post(OPENAI_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .map_err(|e| FerroError::Config(format!("Token exchange failed: {e}")))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| FerroError::Config(format!("Failed reading token response: {e}")))?;
    if !status.is_success() {
        return Err(FerroError::Config(format!(
            "OpenAI token error HTTP {}: {}",
            status.as_u16(),
            text
        )));
    }

    let v: Value = serde_json::from_str(&text)
        .map_err(|e| FerroError::Config(format!("Invalid token JSON: {e}")))?;

    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| FerroError::Config("Token response missing access_token".into()))?
        .to_string();

    let refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let expires_in = v.get("expires_in").and_then(|x| x.as_u64());

    Ok((access, refresh, expires_in))
}

fn env_path() -> PathBuf {
    config::config_dir().join(".env")
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_verifier() -> String {
    let mut rng = rand::thread_rng();
    let mut b = [0u8; 32];
    rng.fill(&mut b);
    b64url(&b)
}

fn pkce_challenge_s256(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    b64url(h.finalize().as_slice())
}

fn random_hex_32() -> String {
    let mut b = [0u8; 16];
    rand::thread_rng().fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn oauth_success_html(title: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"font-family:system-ui;padding:2rem\"><p>{title}</p><p>You can close this tab.</p></body></html>"#
    )
}

fn oauth_error_html(msg: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body style=\"font-family:system-ui;padding:2rem\"><p>OAuth error</p><p>{msg}</p></body></html>"#
    )
}

fn parse_http_first_request(
    stream: &mut TcpStream,
) -> io::Result<(String, HashMap<String, String>)> {
    let mut buf = [0u8; 16384];
    let n = stream.read(&mut buf)?;
    let req = std::str::from_utf8(&buf[..n])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-utf8 request"))?;

    let line = req
        .lines()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty request"))?;

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bad request line",
        ));
    }

    let path_query = parts[1];
    let path_only = path_query
        .split('?')
        .next()
        .unwrap_or(path_query)
        .to_string();
    let query = path_query.split_once('?').map(|(_, q)| q).unwrap_or("");

    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| k.to_string());
        let val = urlencoding::decode(v)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| v.to_string());
        params.insert(key, val);
    }

    Ok((path_only, params))
}

fn bind_localhost_oauth(port: u16) -> Result<Vec<TcpListener>> {
    let addrs = [format!("127.0.0.1:{port}"), format!("[::1]:{port}")];
    let mut listeners = Vec::new();
    let mut errors = Vec::new();

    for addr in addrs {
        match TcpListener::bind(&addr) {
            Ok(listener) => listeners.push(listener),
            Err(e) => errors.push(format!("{addr} ({e})")),
        }
    }

    if listeners.is_empty() {
        return Err(FerroError::Config(format!(
            "Could not bind OAuth callback on port {port}. Tried: {}",
            errors.join(", ")
        )));
    }

    Ok(listeners)
}

fn wait_oauth_on_listener(
    listeners: Vec<TcpListener>,
    expected_path: &str,
    deadline: Duration,
) -> Result<HashMap<String, String>> {
    for listener in &listeners {
        let _ = listener.set_nonblocking(true);
    }

    let start = Instant::now();
    loop {
        if start.elapsed() > deadline {
            return Err(FerroError::Config(
                "OAuth timed out waiting for browser redirect.".into(),
            ));
        }

        for listener in &listeners {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));

                    let (path, params) = parse_http_first_request(&mut stream).map_err(|e| {
                        FerroError::Config(format!("Bad OAuth callback request: {e}"))
                    })?;

                    if path != expected_path {
                        let body = oauth_error_html("wrong callback path");
                        let resp = format!(
                            "HTTP/1.1 404 Not Found\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        return Err(FerroError::Config(format!(
                            "Unexpected callback path: {path}"
                        )));
                    }

                    if let Some(err) = params.get("error") {
                        let body = oauth_error_html(err);
                        let resp = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        return Err(FerroError::Config(format!("OAuth provider error: {err}")));
                    }

                    let body = oauth_success_html("Signed in successfully.");
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());

                    return Ok(params);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    return Err(FerroError::Config(format!(
                        "OAuth callback accept failed: {e}"
                    )));
                }
            }
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| FerroError::Config(format!("Could not open browser: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| FerroError::Config(format!("Could not open browser: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| FerroError::Config(format!("Could not open browser: {e}")))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(FerroError::Config(
        "Unsupported OS for automatic browser launch".into(),
    ))
}

fn upsert_env_var(key: &str, value: &str) -> Result<()> {
    let path = env_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            FerroError::Config(format!("Failed creating {}: {e}", parent.display()))
        })?;
    }

    let mut lines: Vec<String> = if path.exists() {
        fs::read_to_string(&path)
            .map_err(|e| FerroError::Config(format!("Failed reading {}: {e}", path.display())))?
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![
            "# Ferroclaw secrets".to_string(),
            "# Do not commit this file".to_string(),
            "".to_string(),
        ]
    };

    let prefix = format!("{key}=");
    let mut replaced = false;
    for line in &mut lines {
        if line.trim_start().starts_with(&prefix) {
            *line = format!("{key}={value}");
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.push(format!("{key}={value}"));
    }

    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }

    fs::write(&path, content)
        .map_err(|e| FerroError::Config(format!("Failed writing {}: {e}", path.display())))?;
    Ok(())
}

fn remove_env_var(key: &str) -> Result<()> {
    let path = env_path();
    if !path.exists() {
        return Ok(());
    }

    let prefix = format!("{key}=");
    let lines: Vec<String> = fs::read_to_string(&path)
        .map_err(|e| FerroError::Config(format!("Failed reading {}: {e}", path.display())))?
        .lines()
        .filter(|line| !line.trim_start().starts_with(&prefix))
        .map(|s| s.to_string())
        .collect();

    let mut content = lines.join("\n");
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(&path, content)
        .map_err(|e| FerroError::Config(format!("Failed writing {}: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_non_empty() {
        let verifier = pkce_verifier();
        let challenge = pkce_challenge_s256(&verifier);
        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
    }
}
