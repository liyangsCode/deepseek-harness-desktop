//! 自签会话 cookie：复刻 dsh-client-connection 的浏览器会话认证格式，
//! 用于接入一个已在运行的 dsh web 实例（复用分支）。
//! 格式出处：dsh-client-connection/lib/index.js 的 cookieName / encodeCookie / isAuthenticated。

use anyhow::{bail, Context};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// cookie 名前缀（dsh 源码 COOKIE_PREFIX）。
const COOKIE_PREFIX: &str = "dsh-auth-";
/// 会话 cookie 有效期，与 dsh 默认的 cookieMaxAgeDays=30 一致。
const COOKIE_MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// 凭证文件里浏览器会话记录的键。
const RECORD_KEY: &str = "client-connection/browser-session";

#[derive(Deserialize)]
struct CredentialsDoc {
    records: HashMap<String, CredentialRecord>,
}

#[derive(Deserialize)]
struct CredentialRecord {
    kind: String,
    payload: SecretPayload,
}

#[derive(Deserialize)]
struct SecretPayload {
    version: u32,
    secret: String,
}

/// 从 ~/.dsh/.credentials.yaml 读浏览器会话签名密钥（base64url 编码的 32 字节）。
pub fn read_browser_session_secret() -> anyhow::Result<[u8; 32]> {
    let home = std::env::var_os("HOME").context("HOME 环境变量缺失")?;
    let path = Path::new(&home).join(".dsh").join(".credentials.yaml");
    let text = std::fs::read_to_string(&path).with_context(|| format!("读取凭证文件失败：{}", path.display()))?;
    let doc: CredentialsDoc = serde_yaml_ng::from_str(&text).context("凭证文件不是合法的 YAML")?;

    // 记录必须存在，且格式与 dsh 当前版本约定的一致
    let record = doc.records.get(RECORD_KEY).context("凭证文件里没有浏览器会话记录")?;
    if record.kind != "grant" || record.payload.version != 1 {
        bail!("浏览器会话记录格式不受支持（dsh 可能升级改了格式）");
    }
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&record.payload.secret)?;
    let secret: [u8; 32] = secret.try_into().map_err(|_| anyhow::anyhow!("签名密钥解码后不是 32 字节"))?;

    Ok(secret)
}

/// 按 dsh 的格式算出自签 cookie，返回（cookie 名，cookie 值，过期毫秒时间戳）。
pub fn build_session_cookie(secret: &[u8; 32], authority: &str) -> (String, String, u64) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|span| span.as_millis() as u64).unwrap_or(0);
    let expires_at = now + COOKIE_MAX_AGE_MS;

    // cookie 名 = dsh-auth- + base64url(SHA256(authority))
    let name = COOKIE_PREFIX.to_string() + &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(authority.as_bytes()));

    // cookie 值 = v1.<base64url(JSON payload)>.<base64url(HMAC-SHA256(secret, 那段 base64url 字符串本身))>
    let payload = serde_json::json!({
        "version": 1,
        "authority": authority,
        "issuedAt": now,
        "expiresAt": expires_at,
    });
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC 接受任意长度密钥");
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    let value = format!("v1.{body}.{signature}");

    (name, value, expires_at)
}

/// 用最小 HTTP 请求验证自签 cookie 是否被目标实例接受（200 接受，401 拒绝）。
pub fn verify_cookie(authority: &str, name: &str, value: &str, port: u16) -> bool {
    verify(authority, name, value, port).unwrap_or(false)
}

/// 向 127.0.0.1:port 发一个带 Cookie 头的 GET /，只看响应状态码。
fn verify(authority: &str, name: &str, value: &str, port: u16) -> anyhow::Result<bool> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Host 必须与签名里的 authority 完全一致，服务端按 Host 头取 cookie 名
    let request = format!("GET / HTTP/1.1\r\nHost: {authority}\r\nCookie: {name}={value}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes())?;

    // 只读状态行：HTTP/1.1 <状态码> ...
    let mut status_line = String::new();
    BufReader::new(stream).read_line(&mut status_line)?;
    let status = status_line.split_whitespace().nth(1).unwrap_or_default();

    Ok(status == "200")
}
