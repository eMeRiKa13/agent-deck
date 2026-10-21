//! Read the signed-in Claude Desktop session without exposing credentials over IPC.

use std::{path::Path, time::Duration};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    domain::ProviderQuota,
    utils::{home_dir, now_ms},
};

use super::{format_plan, parse_quota};

const COOKIE_HOST: &str = ".claude.ai";
const CHROMIUM_EPOCH_MICROS: i64 = 11_644_473_600_000_000;

struct StoredCookie {
    host: String,
    name: String,
    value: String,
    encrypted_value: Vec<u8>,
}

struct CookieStore {
    version: u32,
    cookies: Vec<StoredCookie>,
}

// A session belongs exclusively to Claude.app; no browser cookie stores are inspected.
struct DesktopSession {
    session_key: String,
    organization: Option<String>,
}

pub(super) async fn load_quota() -> Result<ProviderQuota, String> {
    let store = tokio::task::spawn_blocking(read_cookie_store)
        .await
        .map_err(|_| "Lecture de la session Claude Desktop interrompue".to_string())??;
    let needs_key = store.cookies.iter().any(|cookie| cookie.value.is_empty());
    let mut key = [0_u8; 16];
    if needs_key {
        let password = safe_storage_password().await?;
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&password, b"saltysalt", 1003, &mut key);
    }
    let session = decode_session(&store, &key)?;
    fetch_quota(session).await
}

fn read_cookie_store() -> Result<CookieStore, String> {
    let root = home_dir()?.join("Library/Application Support/Claude");
    let path = [root.join("Network/Cookies"), root.join("Cookies")]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("Session Claude Desktop absente : ouvrir Claude et se connecter")?;
    read_cookie_store_at(&path)
}

fn read_cookie_store_at(path: &Path) -> Result<CookieStore, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "Cookies Claude Desktop illisibles")?;
    connection.busy_timeout(Duration::from_millis(500)).ok();
    let version = connection
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| "Version des cookies Claude Desktop illisible")?
        .parse()
        .map_err(|_| "Version des cookies Claude Desktop invalide")?;
    let mut statement = connection
        .prepare(
            "SELECT host_key, name, value, encrypted_value FROM cookies
         WHERE host_key IN ('.claude.ai', 'claude.ai')
           AND name IN ('sessionKey', 'lastActiveOrg') AND path = '/' AND is_secure = 1
           AND (expires_utc = 0 OR expires_utc > ?1)
         ORDER BY last_access_utc DESC",
        )
        .map_err(|_| "Format des cookies Claude Desktop incompatible")?;
    let rows = statement
        .query_map([now_ms() * 1000 + CHROMIUM_EPOCH_MICROS], |row| {
            Ok(StoredCookie {
                host: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
                encrypted_value: row.get(3)?,
            })
        })
        .map_err(|_| "Lecture des cookies Claude Desktop impossible")?;
    let cookies = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Cookie Claude Desktop invalide")?;
    if !cookies.iter().any(|cookie| cookie.name == "sessionKey") {
        return Err(
            "Session Claude Desktop absente ou expirée : se reconnecter dans Claude".to_string(),
        );
    }
    Ok(CookieStore { version, cookies })
}

// macOS may ask for access to Claude's Safe Storage item on the first read.
async fn safe_storage_password() -> Result<Vec<u8>, String> {
    let mut command = tokio::process::Command::new("/usr/bin/security");
    command
        .args([
            "find-generic-password",
            "-s",
            "Claude Safe Storage",
            "-a",
            "Claude Key",
            "-w",
        ])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| "Délai dépassé pour le Trousseau Claude Desktop")?
        .map_err(|_| "Trousseau Claude Desktop inaccessible")?;
    if !output.status.success() {
        return Err("Accès au Trousseau Claude Desktop refusé ou indisponible".to_string());
    }
    let mut password = output.stdout;
    if password.last() == Some(&b'\n') {
        password.pop();
    }
    if password.last() == Some(&b'\r') {
        password.pop();
    }
    if password.is_empty() {
        return Err("Clé Claude Desktop vide".to_string());
    }
    Ok(password)
}

fn decode_cookie(cookie: &StoredCookie, version: u32, key: &[u8; 16]) -> Result<String, String> {
    if cookie.host != COOKIE_HOST && cookie.host != "claude.ai" {
        return Err("Hôte du cookie Claude Desktop invalide".to_string());
    }
    if !cookie.value.is_empty() {
        return Ok(cookie.value.clone());
    }
    let encrypted = cookie
        .encrypted_value
        .strip_prefix(b"v10")
        .ok_or("Chiffrement des cookies Claude Desktop non pris en charge")?;
    let mut buffer = encrypted.to_vec();
    let plaintext = cbc::Decryptor::<aes::Aes128>::new(key.into(), (&[b' '; 16]).into())
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| "Déchiffrement de la session Claude Desktop impossible")?;
    // Chromium 24+ binds every encrypted cookie to its host with a SHA-256 prefix.
    let plaintext = if version >= 24 {
        let hash = Sha256::digest(cookie.host.as_bytes());
        plaintext
            .strip_prefix(&hash[..])
            .ok_or("Hôte du cookie Claude Desktop invalide")?
    } else {
        plaintext
    };
    String::from_utf8(plaintext.to_vec())
        .map_err(|_| "Encodage du cookie Claude Desktop invalide".to_string())
}

fn decode_session(store: &CookieStore, key: &[u8; 16]) -> Result<DesktopSession, String> {
    let session = store
        .cookies
        .iter()
        .find(|cookie| cookie.name == "sessionKey")
        .ok_or("Session Claude Desktop introuvable")?;
    let session_key = decode_cookie(session, store.version, key)?;
    if session_key.is_empty()
        || !session_key
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b';')
    {
        return Err("Session Claude Desktop invalide".to_string());
    }
    let organization = store
        .cookies
        .iter()
        .find(|cookie| cookie.name == "lastActiveOrg")
        .map(|cookie| decode_cookie(cookie, store.version, key))
        .transpose()?
        .filter(|value| valid_organization_id(value));
    Ok(DesktopSession {
        session_key,
        organization,
    })
}

fn valid_organization_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, ch)| {
            if [8, 13, 18, 23].contains(&index) {
                ch == '-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
}

fn select_organization<'a>(
    organizations: &'a Value,
    active: Option<&str>,
) -> Result<&'a Value, String> {
    let entries = organizations
        .as_array()
        .ok_or("Liste des organisations Claude invalide")?;
    if let Some(active) = active {
        return entries
            .iter()
            .find(|entry| entry["uuid"].as_str() == Some(active))
            .ok_or_else(|| "Organisation active Claude Desktop inaccessible".to_string());
    }
    if let [only] = entries.as_slice() {
        if only["uuid"].as_str().is_some_and(valid_organization_id) {
            return Ok(only);
        }
    }
    Err(
        "Organisation Claude Desktop ambiguë : sélectionner une organisation dans Claude"
            .to_string(),
    )
}

async fn fetch_quota(session: DesktopSession) -> Result<ProviderQuota, String> {
    let mut cookie = HeaderValue::from_str(&format!("sessionKey={}", session.session_key))
        .map_err(|_| "Session Claude Desktop invalide")?;
    cookie.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, cookie);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(headers)
        .user_agent("Agent Deck/0.1.0")
        .build()
        .map_err(|_| "Initialisation de la connexion Claude impossible")?;
    let organizations = get_json(&client, "https://claude.ai/api/organizations").await?;
    let organization = select_organization(&organizations, session.organization.as_deref())?;
    let id = organization["uuid"]
        .as_str()
        .filter(|id| valid_organization_id(id))
        .ok_or("Identifiant d’organisation Claude invalide")?;
    let plan = organization["rate_limit_tier"]
        .as_str()
        .map(format_plan)
        .unwrap_or_else(|| "Claude App".to_string());
    let value = get_json(
        &client,
        &format!("https://claude.ai/api/organizations/{id}/usage?cedar_ember=1"),
    )
    .await?;
    parse_quota(&value, plan)
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    let response = client
        .get(url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|_| "Quotas Claude Desktop injoignables".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Quotas Claude Desktop : HTTP {}",
            response.status()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "Réponse de quota Claude Desktop invalide".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncryptMut;
    use serde_json::json;

    fn cookie_fixture(host: &str, version: u32) -> StoredCookie {
        let mut plaintext = if version >= 24 {
            Sha256::digest(host.as_bytes()).to_vec()
        } else {
            Vec::new()
        };
        plaintext.extend_from_slice(b"test-session");
        let length = plaintext.len();
        plaintext.resize(length + 16, 0);
        let ciphertext =
            cbc::Encryptor::<aes::Aes128>::new((&[7; 16]).into(), (&[b' '; 16]).into())
                .encrypt_padded_mut::<Pkcs7>(&mut plaintext, length)
                .unwrap();
        let mut encrypted_value = b"v10".to_vec();
        encrypted_value.extend_from_slice(ciphertext);
        StoredCookie {
            host: host.into(),
            name: "sessionKey".into(),
            value: String::new(),
            encrypted_value,
        }
    }

    #[test]
    fn decrypts_legacy_and_host_bound_desktop_cookies() {
        for version in [23, 24] {
            assert_eq!(
                decode_cookie(&cookie_fixture(COOKIE_HOST, version), version, &[7; 16]).unwrap(),
                "test-session"
            );
        }
        let mut wrong_host = cookie_fixture(COOKIE_HOST, 24);
        wrong_host.host = "unrelated.example".into();
        assert!(decode_cookie(&wrong_host, 24, &[7; 16]).is_err());
        assert!(decode_cookie(&cookie_fixture(COOKIE_HOST, 24), 24, &[8; 16]).is_err());
    }

    #[test]
    fn follows_the_desktop_organization_without_guessing() {
        let a = "11111111-1111-1111-1111-111111111111";
        let b = "22222222-2222-2222-2222-222222222222";
        let organizations = json!([{"uuid":a},{"uuid":b}]);
        assert_eq!(
            select_organization(&organizations, Some(b)).unwrap()["uuid"],
            b
        );
        assert!(select_organization(&organizations, None).is_err());
        assert!(select_organization(&organizations, Some("missing")).is_err());
        assert!(valid_organization_id(a));
        assert!(!valid_organization_id("../../other"));
    }

    #[test]
    fn cookie_reader_rejects_expired_sessions_and_ignores_other_hosts() {
        let path = std::env::temp_dir().join(format!(
            "agent-deck-claude-cookies-{}-{}.sqlite",
            std::process::id(),
            now_ms()
        ));
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(
            "CREATE TABLE meta (key TEXT, value TEXT);
             INSERT INTO meta VALUES ('version', '24');
             CREATE TABLE cookies (host_key TEXT, name TEXT, value TEXT, encrypted_value BLOB,
                 path TEXT, expires_utc INTEGER, is_secure INTEGER, last_access_utc INTEGER);
             INSERT INTO cookies VALUES ('.claude.ai','sessionKey','expired',X'','/',1,1,0);
             INSERT INTO cookies VALUES ('.unrelated.example','sessionKey','unrelated',X'','/',0,1,0);"
        ).unwrap();
        assert!(read_cookie_store_at(&path).is_err());
        connection.execute_batch(
            "INSERT INTO cookies VALUES ('.claude.ai','sessionKey','test-session',X'','/',0,1,1);"
        ).unwrap();
        let store = read_cookie_store_at(&path).unwrap();
        assert_eq!(store.cookies.len(), 1);
        assert_eq!(
            decode_session(&store, &[0; 16]).unwrap().session_key,
            "test-session"
        );
        drop(connection);
        std::fs::remove_file(path).unwrap();
    }
}
