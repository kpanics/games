//! Payments through FreeKassa (her shop 76760, 10.10.2026): the notification a payment sends here, and where a payer
//! comes back to. Nothing is sold yet: a payment is checked (from FreeKassa's own addresses, signed with the second
//! secret word, for her shop) and kept, and FreeKassa is told YES. What a payment gives is for when there is something
//! to sell.
//!
//! The shop's number and the secret words come from /etc/cursed/pay.env (root:cursed 640, read by the service's
//! EnvironmentFile): FK_MERCHANT, FK_SECRET1 (the payment form's), FK_SECRET2 (the notifications').

mod rollypay;
mod income;
pub use rollypay::recover;

use std::collections::HashMap;
use std::env;

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rusqlite::{params, Connection};

use crate::{db, now, Shared};

/// Where FreeKassa's notifications come from (its documentation, 10.10.2026).
const FROM: [&str; 4] = ["168.119.157.136", "168.119.60.227", "178.154.197.79", "51.250.54.238"];

pub fn schema(c: &Connection) {
    rollypay::schema(c);
    income::schema(c);
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS payments (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             provider TEXT NOT NULL,                     -- 'freekassa'
             intid TEXT NOT NULL,                        -- the provider's number of the operation
             order_id TEXT NOT NULL,                     -- ours, as the payment form carried it
             amount TEXT NOT NULL,                       -- as the provider wrote it
             currency TEXT NOT NULL DEFAULT '',          -- the provider's currency id
             email TEXT NOT NULL DEFAULT '',
             extra TEXT NOT NULL DEFAULT '{}',           -- the us_ fields, the commission (JSON)
             at INTEGER NOT NULL,
             UNIQUE (provider, intid));",
    )
    .expect("make the payments' table");
}

/// md5 of a string, lower-case hex: what FreeKassa signs with.
fn md5_hex(s: &str) -> String {
    use md5::{Digest, Md5};
    let d = Md5::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The notification's signature as FreeKassa makes it: md5(MERCHANT_ID:AMOUNT:secret2:MERCHANT_ORDER_ID).
pub fn signature(merchant: &str, amount: &str, secret2: &str, order: &str) -> String {
    md5_hex(&format!("{merchant}:{amount}:{secret2}:{order}"))
}

/// The payment form's signature: md5(m:oa:secret1:currency:o) (for the links to pay, when there is something to sell).
#[allow(dead_code)]
pub fn form_signature(merchant: &str, amount: &str, secret1: &str, currency: &str, order: &str) -> String {
    md5_hex(&format!("{merchant}:{amount}:{secret1}:{currency}:{order}"))
}

/// What the notification says is checked: its sender, the shop, the signature. Err with why not.
pub fn check(fields: &HashMap<String, String>, ip: Option<&str>, merchant: &str, secret2: &str) -> Result<(), &'static str> {
    if !ip.is_some_and(|ip| FROM.contains(&ip)) {
        return Err("not from FreeKassa");
    }
    let f = |k: &str| fields.get(k).map(String::as_str).unwrap_or("");
    if merchant.is_empty() || secret2.is_empty() {
        return Err("not set up");
    }
    if f("MERCHANT_ID") != merchant {
        return Err("another shop");
    }
    let want = signature(f("MERCHANT_ID"), f("AMOUNT"), secret2, f("MERCHANT_ORDER_ID"));
    if !f("SIGN").eq_ignore_ascii_case(&want) {
        return Err("bad signature");
    }
    if f("intid").is_empty() {
        return Err("no operation");
    }
    Ok(())
}

/// GET or POST (form) from FreeKassa: a payment made. A call with nothing in it (an address checked) is answered as
/// ready; a payment checked and kept is answered YES (FreeKassa's word that it was taken).
async fn notified(State(app): State<Shared>, h: HeaderMap, Query(query): Query<HashMap<String, String>>, body: String) -> Response {
    let mut fields = query;
    for (k, v) in url_form(&body) {
        fields.insert(k, v);
    }
    if !fields.contains_key("SIGN") {
        return (StatusCode::OK, "ready").into_response();
    }
    let ip = h.get("x-real-ip").and_then(|v| v.to_str().ok()).map(str::trim).map(str::to_string);
    let merchant = env::var("FK_MERCHANT").unwrap_or_default();
    let secret2 = env::var("FK_SECRET2").unwrap_or_default();
    if let Err(why) = check(&fields, ip.as_deref(), &merchant, &secret2) {
        eprintln!("[pay] freekassa notification refused ({why}) from {}", ip.as_deref().unwrap_or("?"));
        return (StatusCode::BAD_REQUEST, why).into_response();
    }
    let f = |k: &str| fields.get(k).cloned().unwrap_or_default();
    let extra: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .filter(|(k, _)| k.starts_with("us_") || *k == "commission" || *k == "payer_account")
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let (intid, order, amount, cur, email) = (f("intid"), f("MERCHANT_ORDER_ID"), f("AMOUNT"), f("CUR_ID"), f("P_EMAIL"));
    let kept = db(&app, move |c| {
        c.execute(
            "INSERT OR IGNORE INTO payments (provider, intid, order_id, amount, currency, email, extra, at) VALUES ('freekassa', ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![intid, order, amount, cur, email, serde_json::Value::Object(extra).to_string(), now()],
        )
    })
    .await;
    match kept {
        Ok(_) => (StatusCode::OK, "YES").into_response(),
        Err(e) => {
            eprintln!("[pay] a payment could not be kept: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "not kept").into_response()
        }
    }
}

/// x-www-form-urlencoded pairs.
fn url_form(body: &str) -> Vec<(String, String)> {
    body.split('&').filter(|p| !p.is_empty()).map(|p| {
        let (k, v) = p.split_once('=').unwrap_or((p, ""));
        (decode(k), decode(v))
    }).collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/pay/freekassa", get(notified).post(notified).layer(DefaultBodyLimit::max(8192)))
        .merge(rollypay::routes())
        .merge(income::routes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(sign: &str) -> HashMap<String, String> {
        [("MERCHANT_ID", "76760"), ("AMOUNT", "100"), ("MERCHANT_ORDER_ID", "o1"), ("intid", "555"), ("SIGN", sign)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn md5_as_known() {
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn a_notification_checked() {
        let good = signature("76760", "100", "s2", "o1");
        assert_eq!(check(&fields(&good), Some("168.119.157.136"), "76760", "s2"), Ok(()));
        assert_eq!(check(&fields(&good.to_uppercase()), Some("51.250.54.238"), "76760", "s2"), Ok(()));
        assert_eq!(check(&fields(&good), Some("1.2.3.4"), "76760", "s2"), Err("not from FreeKassa"));
        assert_eq!(check(&fields(&good), None, "76760", "s2"), Err("not from FreeKassa"));
        assert_eq!(check(&fields("00"), Some("168.119.60.227"), "76760", "s2"), Err("bad signature"));
        assert_eq!(check(&fields(&good), Some("168.119.60.227"), "76761", "s2"), Err("another shop"));
        assert_eq!(check(&fields(&good), Some("168.119.60.227"), "76760", ""), Err("not set up"));
    }

    #[test]
    fn forms_read() {
        let f = url_form("MERCHANT_ID=76760&AMOUNT=100.50&P_EMAIL=a%40b.c&us_x=y+z&SIGN=ab");
        assert_eq!(f[2], ("P_EMAIL".to_string(), "a@b.c".to_string()));
        assert_eq!(f[3], ("us_x".to_string(), "y z".to_string()));
        assert_eq!(decode("100%"), "100%");
    }
}
