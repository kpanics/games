//! RollyPay: fixed-price orders, authenticated bill verification and atomic fulfillment.
//! Protocol: https://docs.rollypay.io (27 October 2026).
use super::url_form;
use crate::{blocking, db, fail, market::member, now, random_hex, Shared};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use std::{collections::HashMap, env, io::Read, time::Duration};

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS rollypay_orders (
        id TEXT PRIMARY KEY, uid INTEGER NOT NULL, days INTEGER NOT NULL,
        amount INTEGER NOT NULL, bill TEXT UNIQUE, url TEXT, made INTEGER NOT NULL,
        fulfilled INTEGER, state TEXT NOT NULL DEFAULT 'created', last_check INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX IF NOT EXISTS rollypay_orders_uid ON rollypay_orders(uid, made);",
    )
    .expect("rollypay orders");
}
pub(super) fn price(days: i64) -> Option<i64> {
    match days {
        30 => Some(29900),
        90 => Some(49900),
        0 => Some(69900),
        _ => None,
    }
}
fn config() -> Result<(String, String), &'static str> {
    let token = env::var("ROLLYPAY_API_KEY").unwrap_or_default();
    let shop = env::var("ROLLYPAY_TERMINAL_ID").unwrap_or_default();
    if token.is_empty()
        || shop.is_empty()
        || env::var("ROLLYPAY_SIGNING_SECRET")
            .unwrap_or_default()
            .is_empty()
    {
        Err("payment-unavailable")
    } else {
        Ok((token, shop))
    }
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .redirects(0)
        .build()
}
fn read_json(r: ureq::Response) -> Result<Value, &'static str> {
    let mut bytes = Vec::new();
    r.into_reader()
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "payment-provider")?;
    if bytes.len() > 65536 {
        return Err("payment-provider");
    }
    serde_json::from_slice(&bytes).map_err(|_| "payment-provider")
}
fn money(s: &str) -> Option<i64> {
    let (whole, raw_fraction) = s.split_once('.').unwrap_or((s, ""));
    if raw_fraction.len() > 18 || !raw_fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let fraction = raw_fraction.trim_end_matches('0');
    if whole.is_empty()
        || whole.len() > 10
        || fraction.len() > 2
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()? * 10,
        _ => fraction.parse::<i64>().ok()?,
    };
    whole
        .parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(cents)
}
fn money_value(v: &Value) -> Option<i64> {
    money(
        &v.as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string()),
    )
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_url(s: &str) -> bool {
    s.strip_prefix("https://pay.rollypay.io/pay/")
        .is_some_and(valid_id)
}
fn request(method: &str, path: &str, token: &str) -> ureq::Request {
    agent()
        .request(method, &format!("https://rollypay.io/api/v1/{path}"))
        .set("X-API-Key", token)
        .set("X-Nonce", &random_hex(24))
}

#[derive(Deserialize)]
struct Checkout {
    days: i64,
    agree: bool,
    #[serde(default)]
    method: String,
    #[serde(default)]
    promo: String,
    #[serde(default)]
    expected_amount: Option<i64>,
}
async fn checkout(
    State(app): State<Shared>,
    h: HeaderMap,
    Json(input): Json<Checkout>,
) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    if env::var("ROLLYPAY_ENABLED").as_deref() != Ok("1") {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "payment-unavailable");
    }
    let Some(amount) = price(input.days) else {
        return fail(StatusCode::BAD_REQUEST, "term");
    };
    if !input.agree || !["", "sbp", "crypto", "xrocket"].contains(&input.method.as_str()) {
        return fail(StatusCode::BAD_REQUEST, "checkout");
    }
    let (token, shop) = match config() {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    let id = format!("rcd-{}", random_hex(16));
    let (order, uid, days) = (id.clone(), w.uid, input.days);
    let promo = input.promo.clone();
    let expected_amount = input.expected_amount;
    let method = if input.method.is_empty() {
        "sbp".to_owned()
    } else {
        input.method.clone()
    };
    let selected_method = method.clone();
    let reserved = db(&app, move |c| -> Result<i64, &'static str> {
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM rollypay_orders WHERE uid=?1 AND made>?2",
                params![uid, now() - 3600],
                |r| r.get(0),
            )
            .map_err(|_| "busy")?;
        if count >= 10 {
            return Err("slow");
        }
        if crate::recode::of(c, uid).forever {
            return Err("already-forever");
        }
        let tx = c.transaction().map_err(|_| "busy")?;
        let amount = super::income::reserve(&tx, &order, amount, &promo, &selected_method)?;
        if expected_amount.is_some_and(|expected| expected != amount) {
            return Err("price-changed");
        }
        tx.execute(
            "INSERT INTO rollypay_orders(id,uid,days,amount,made) VALUES(?1,?2,?3,?4,?5)",
            params![order, uid, days, amount, now()],
        )
        .map_err(|_| "busy")?;
        tx.commit().map_err(|_| "busy")?;
        Ok(amount)
    })
    .await;
    let amount = match reserved {
        Ok(amount) => amount,
        Err(e) => return fail(StatusCode::CONFLICT, e),
    };
    let order = id.clone();
    let result = blocking(move || {
        let sum = format!("{}.{:02}", amount / 100, amount % 100);
        let description = if days == 0 {
            "Cursed Recode — навсегда".to_owned()
        } else {
            format!("Cursed Recode — {days} дней")
        };
        let mut payload = json!({"amount":sum,"terminal_id":shop,"order_id":order,
            "payment_currency":"RUB","description":description,"customer_id":uid.to_string(),
            "success_redirect_url":format!("https://cursedclient.xyz/api/pay/ok?id={order}"),
            "fail_redirect_url":format!("https://cursedclient.xyz/api/pay/fail?id={order}")});
        payload["payment_method"] = json!(method);
        let r = request("POST", "payments", &token)
            .send_json(payload)
            .map_err(|_| "payment-provider")?;
        let v = read_json(r)?;
        let bill = v["payment_id"].as_str().unwrap_or("");
        let url = v["pay_url"].as_str().unwrap_or("");
        if !valid_id(bill)
            || !valid_url(url)
            || v["order_id"] != order
            || money_value(&v["amount"]) != Some(amount)
            || v["payment_currency"] != "RUB"
            || is_test(&v)
        {
            return Err("payment-provider");
        }
        Ok((bill.to_owned(), url.to_owned()))
    })
    .await;
    let (bill, url) = match result {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, e),
    };
    let link = url.clone();
    let saved = db(&app, move |c| {
        c.execute(
            "UPDATE rollypay_orders SET bill=?2,url=?3 WHERE id=?1",
            params![id, bill, url],
        )
    })
    .await;
    match saved {
        Ok(1) => Json(json!({"url":link})).into_response(),
        _ => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

#[derive(Clone)]
struct Order {
    id: String,
    uid: i64,
    days: i64,
    amount: i64,
    bill: String,
    fulfilled: bool,
    state: String,
}
fn order(c: &Connection, id: &str) -> rusqlite::Result<Option<Order>> {
    c.query_row("SELECT id,uid,days,amount,COALESCE(bill,''),fulfilled IS NOT NULL,state FROM rollypay_orders WHERE id=?1", [id], |r|
        Ok(Order { id:r.get(0)?,uid:r.get(1)?,days:r.get(2)?,amount:r.get(3)?,bill:r.get(4)?,fulfilled:r.get(5)?,state:r.get(6)? })).optional()
}
fn is_test(v: &Value) -> bool {
    v.get("test").is_some_and(|x| x != &Value::Bool(false))
        || v.get("is_test").is_some_and(|x| x != &Value::Bool(false))
        || v.get("environment").is_some_and(|x| x != "prod")
}
fn verify_bill(v: &Value, o: &Order) -> Result<String, &'static str> {
    if v["payment_id"] != o.bill
        || v["order_id"] != o.id
        || v["payment_currency"] != "RUB"
        || money_value(&v["amount"]) != Some(o.amount)
        || is_test(v)
    {
        return Err("bill-mismatch");
    }
    let state = v["status"].as_str().unwrap_or("");
    if ![
        "created",
        "processing",
        "paid",
        "expired",
        "canceled",
        "refunded",
        "chargeback",
    ]
    .contains(&state)
    {
        return Err("payment-status");
    }
    Ok(state.to_owned())
}
fn fulfill(c: &Connection, o: &Order) -> Result<(), &'static str> {
    let tx = c;
    let current = order(tx, &o.id)
        .map_err(|_| "busy")?
        .ok_or("order-missing")?;
    if current.fulfilled {
        return Ok(());
    }
    if current.bill != o.bill
        || current.amount != o.amount
        || current.uid != o.uid
        || current.days != o.days
    {
        return Err("order-mismatch");
    }
    let login: String = tx
        .query_row("SELECT login FROM users WHERE uid=?1", [o.uid], |r| {
            r.get(0)
        })
        .map_err(|_| "account-missing")?;
    let at = now();
    // This non-redeemable ledger entry participates in the existing subscription calculation.
    tx.execute("INSERT INTO recode_keys(code,days,batch,note,made,used_by,used_login,used_at,maker) VALUES(?1,?2,0,?3,?4,?5,?6,?4,-1)",
        params![format!("ROLLYPAY-{}",o.id),o.days,format!("RollyPay {}",o.bill),at,o.uid,login]).map_err(|_| "busy")?;
    tx.execute("INSERT INTO payments(provider,intid,order_id,amount,currency,at) VALUES('rollypay',?1,?2,?3,'RUB',?4)",
        params![o.bill,o.id,format!("{}.{:02}",o.amount/100,o.amount%100),at]).map_err(|_| "busy")?;
    tx.execute(
        "UPDATE rollypay_orders SET fulfilled=?2 WHERE id=?1",
        params![o.id, at],
    )
    .map_err(|_| "busy")?;
    crate::people::audit(
        &tx,
        o.uid,
        "pay.rollypay",
        &o.id,
        json!({"bill":o.bill,"days":o.days,"amount":o.amount}),
    );
    Ok(())
}
fn apply_state(c: &mut Connection, o: &Order, state: &str) -> Result<String, &'static str> {
    apply_state_fee(c, o, state, None)
}
fn apply_state_fee(
    c: &mut Connection,
    o: &Order,
    state: &str,
    fee: Option<(String, i64)>,
) -> Result<String, &'static str> {
    let tx = c.transaction().map_err(|_| "busy")?;
    let current = order(&tx, &o.id)
        .map_err(|_| "busy")?
        .ok_or("order-missing")?;
    if current.bill != o.bill
        || current.uid != o.uid
        || current.amount != o.amount
        || current.days != o.days
    {
        return Err("order-mismatch");
    }
    // A replay must never resurrect refunded access or undo a paid order.
    if matches!(current.state.as_str(), "refunded" | "chargeback") {
        return Ok(current.state);
    }
    if state == "paid" {
        fulfill(&tx, o)?;
        super::income::settle(
            &tx,
            &o.id,
            fee.as_ref().map(|(method, rate)| (method.as_str(), *rate)),
        )?;
    }
    if matches!(state, "refunded" | "chargeback") {
        super::income::reverse(&tx, &o.id)?;
        tx.execute(
            "UPDATE recode_keys SET revoked=?2 WHERE code=?1",
            params![format!("ROLLYPAY-{}", o.id), now()],
        )
        .map_err(|_| "busy")?;
        crate::people::audit(
            &tx,
            o.uid,
            "pay.rollypay.reversed",
            &o.id,
            json!({"status":state,"bill":o.bill}),
        );
    } else if current.fulfilled && state != "paid" {
        return Ok(current.state);
    }
    tx.execute(
        "UPDATE rollypay_orders SET state=?2 WHERE id=?1",
        params![o.id, state],
    )
    .map_err(|_| "busy")?;
    tx.commit().map_err(|_| "busy")?;
    crate::recode::marks(c, o.uid);
    Ok(state.to_owned())
}
async fn reconcile(app: &Shared, o: Order, token: String) -> Result<String, &'static str> {
    if !valid_id(&o.bill) {
        return Err("bill-pending");
    }
    let check = o.clone();
    let (state, fee) = blocking(move || {
        let r = request("GET", &format!("payments/{}", check.bill), &token)
            .call()
            .map_err(|_| "payment-provider")?;
        let v = read_json(r)?;
        let state = verify_bill(&v, &check)?;
        Ok::<_, &'static str>((state, provider_fee(&v)))
    })
    .await?;
    db(app, move |c| apply_state_fee(c, &o, &state, fee)).await
}
// A closed browser or an exhausted webhook retry must not strand a confirmed purchase.
fn due_orders(c: &Connection) -> Vec<Order> {
    let at = now();
    let ids = (|| -> rusqlite::Result<Vec<String>> {
        let mut q=c.prepare("SELECT id FROM rollypay_orders WHERE fulfilled IS NULL AND bill IS NOT NULL AND state IN ('created','processing','expired','paid') AND made>?1 AND last_check<?2 ORDER BY last_check,made LIMIT 10")?;
        let values=q.query_map(params![at-7*86400,at-60],|r|r.get(0))?.collect();values
    })().unwrap_or_default();
    ids.into_iter()
        .filter_map(|id| {
            let o = order(c, &id).ok().flatten()?;
            if !valid_id(&o.bill) {
                return None;
            }
            c.execute(
                "UPDATE rollypay_orders SET last_check=?2 WHERE id=?1",
                params![id, at],
            )
            .ok()?;
            Some(o)
        })
        .collect()
}
pub async fn recover(app: Shared) {
    loop {
        if env::var("ROLLYPAY_ENABLED").as_deref() == Ok("1") {
            if let Ok((token, _)) = config() {
                let pending = db(&app, |c| due_orders(c)).await;
                for o in pending {
                    let id = o.id.clone();
                    match reconcile(&app, o, token.clone()).await {
                        Ok(state) if state == "paid" => {
                            eprintln!("[pay] reconciled confirmed RollyPay order {id}")
                        }
                        Err(error) => eprintln!("[pay] RollyPay reconciliation {id}: {error}"),
                        _ => {}
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
// Prefer the provider's effective fee; otherwise use the agreed tariff for its confirmed method.
fn provider_fee(v: &Value) -> Option<(String, i64)> {
    let method = v["payment_method"].as_str()?;
    let tariff = match method {
        "sbp" => 900,
        "crypto" => 300,
        "cryptobot" => 400,
        "xrocket" => 300,
        _ => return None,
    };
    let rate = if v.get("platform_fee_percent").is_some_and(|x| !x.is_null()) {
        money_value(&v["platform_fee_percent"])?
    } else {
        tariff
    };
    if rate > 10000 {
        return None;
    };
    Some((method.to_owned(), rate))
}
fn signature_ok(body: &str, timestamp: &str, signature: &str, secret: &str) -> bool {
    if secret.is_empty()
        || signature.len() != 64
        || timestamp
            .parse::<i64>()
            .ok()
            .is_none_or(|t| t <= 0 || t > now() + 300)
    {
        return false;
    }
    let mut supplied = [0u8; 32];
    for (i, pair) in signature.as_bytes().chunks_exact(2).enumerate() {
        let Ok(pair) = std::str::from_utf8(pair) else {
            return false;
        };
        let Ok(byte) = u8::from_str_radix(pair, 16) else {
            return false;
        };
        supplied[i] = byte;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    mac.verify_slice(&supplied).is_ok()
}
async fn notified(State(app): State<Shared>, h: HeaderMap, body: String) -> Response {
    let secret = env::var("ROLLYPAY_SIGNING_SECRET").unwrap_or_default();
    let header = |k| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !signature_ok(&body, header("x-timestamp"), header("x-signature"), &secret) {
        return fail(StatusCode::FORBIDDEN, "signature");
    }
    let Ok(mut v) = serde_json::from_str::<Value>(&body) else {
        return fail(StatusCode::BAD_REQUEST, "json");
    };
    if is_test(&v) || header("x-test-mode") == "true" {
        eprintln!("[pay] rollypay sandbox callback verified; no entitlement issued");
        return "OK sandbox ignored".into_response();
    }
    let id = v["order_id"].as_str().unwrap_or("").to_owned();
    let o = match db(&app, move |c| order(c, &id)).await {
        Ok(Some(o)) => o,
        Ok(None) => return "OK unrelated order".into_response(),
        Err(_) => return fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    };
    if o.bill.is_empty() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "bill-pending");
    }
    v["payment_currency"] = v["currency"].clone();
    let state = match verify_bill(&v, &o) {
        Ok(s) => s,
        Err(e) => return fail(StatusCode::BAD_REQUEST, e),
    };
    let event = v["event_type"].as_str().unwrap_or("");
    if event != format!("payment.{state}")
        && !(event == "refund_request.completed" && state == "refunded")
    {
        return fail(StatusCode::BAD_REQUEST, "event-mismatch");
    }
    let tracked_id = o.id.clone();
    let tracked = db(&app, move |c| super::income::tracked(c, &tracked_id)).await;
    if state == "paid" && tracked {
        let (token, _) = match config() {
            Ok(v) => v,
            Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
        };
        return match reconcile(&app, o, token).await {
            Ok(s) if matches!(s.as_str(), "paid" | "refunded" | "chargeback") => {
                "OK".into_response()
            }
            Ok(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "payment-pending"),
            Err(e) => fail(StatusCode::SERVICE_UNAVAILABLE, e),
        };
    }
    match db(&app, move |c| apply_state(c, &o, &state)).await {
        Ok(_) => "OK".into_response(),
        Err(e) => fail(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
async fn ready() -> Response {
    "RollyPay notification endpoint".into_response()
}
#[derive(Deserialize)]
struct StatusQuery {
    id: String,
}
async fn status(State(app): State<Shared>, h: HeaderMap, Query(q): Query<StatusQuery>) -> Response {
    let w = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    let o = match db(&app, move |c| order(c, &q.id)).await {
        Ok(Some(o)) if o.uid == w.uid => o,
        _ => return fail(StatusCode::NOT_FOUND, "order-missing"),
    };

    let id = o.id.clone();
    let due = db(&app, move |c| {
        c.execute(
            "UPDATE rollypay_orders SET last_check=?2 WHERE id=?1 AND last_check<?3",
            params![id, now(), now() - 10],
        )
    })
    .await
    .unwrap_or(0)
        == 1;
    if !due {
        return Json(json!({"paid":o.state=="paid","status":o.state})).into_response();
    }
    let (token, _) = match config() {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    match reconcile(&app, o, token).await {
        Ok(state) => Json(json!({"paid":state=="paid","status":state})).into_response(),
        Err(e) => fail(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
// No claim of successful payment is made based on an untrusted browser redirect.
async fn back_ok(Query(q): Query<HashMap<String, String>>, body: String) -> Response {
    back(q, body, false)
}
async fn back_fail(Query(q): Query<HashMap<String, String>>, body: String) -> Response {
    back(q, body, true)
}
fn back(mut q: HashMap<String, String>, body: String, failed: bool) -> Response {
    q.extend(url_form(&body));
    let id = q
        .get("id")
        .filter(|s| valid_id(s))
        .cloned()
        .unwrap_or_default();
    axum::response::Redirect::to(&format!(
        "/api/pay/result?id={id}&failed={}",
        u8::from(failed)
    ))
    .into_response()
}
async fn result(Query(q): Query<HashMap<String, String>>) -> Response {
    result_page(q.get("failed").is_some_and(|v| v == "1"))
}
async fn result_js() -> Response {
    (
        [("content-type", "application/javascript; charset=utf-8")],
        include_str!("rollypay-result.js"),
    )
        .into_response()
}
fn result_page(failed: bool) -> Response {
    let (title, text) = if failed {
        ("Оплата не завершена", "Если деньги списались, проверь подписку в личном кабинете. Иначе вернись к выбору подписки и попробуй ещё раз.")
    } else {
        ("Возвращение после оплаты", "Подписка появится в личном кабинете после подтверждения платежа. Обычно это занимает несколько секунд. Если доступ пока не появился, обнови кабинет чуть позже.")
    };
    Html(format!("<!doctype html><html lang=ru><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{title} · Cursed</title><style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#09090d;color:#e9e8f5;font:17px/1.6 system-ui}}main{{max-width:580px;padding:32px}}small{{letter-spacing:.2em;color:#aaa6c4}}h1{{line-height:1.2;font-size:32px}}a{{display:inline-block;color:#d7caff;margin:12px 24px 0 0}}p{{color:#bbb9cc}}</style><main><small>CURSED RECODE</small><h1>{title}</h1><p>{text}</p><a href='/#/dashboard'>Личный кабинет</a><a href='/#/buy'>К покупке</a></main><script src=/api/pay/result.js defer></script></html>")).into_response()
}
pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/pay/rollypay/callback", get(ready).post(notified))
        .route("/api/pay/rollypay/create", post(checkout))
        .route("/api/pay/rollypay/status", get(status))
        .route("/api/pay/ok", get(back_ok).post(back_ok))
        .route("/api/pay/fail", get(back_fail).post(back_fail))
        .route("/api/pay/result", get(result))
        .route("/api/pay/result.js", get(result_js))
        .layer(DefaultBodyLimit::max(8192))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Connection, Order) {
        let c = Connection::open_in_memory().unwrap();
        super::super::schema(&c);
        c.execute_batch("CREATE TABLE users(uid INTEGER PRIMARY KEY,login TEXT); INSERT INTO users VALUES(42,'buyer');
            CREATE TABLE recode_keys(code TEXT PRIMARY KEY,days INTEGER,batch INTEGER,note TEXT,made INTEGER,used_by INTEGER,used_login TEXT,used_at INTEGER,maker INTEGER,revoked INTEGER);
            CREATE TABLE audit(at INTEGER,actor INTEGER,action TEXT,target TEXT,details TEXT);
            INSERT INTO rollypay_orders(id,uid,days,amount,bill,made) VALUES('rcd-test',42,30,29900,'bill1',0);").unwrap();
        let o = order(&c, "rcd-test").unwrap().unwrap();
        (c, o)
    }
    #[test]
    fn fulfillment_is_atomic_and_idempotent() {
        let (mut c, o) = fixture();
        apply_state(&mut c, &o, "paid").unwrap();
        apply_state(&mut c, &o, "paid").unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM recode_keys", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(crate::recode::of(&c, 42).active(now()));
        assert!(order(&c, "rcd-test").unwrap().unwrap().fulfilled);
        let (mut c, o) = fixture();
        c.execute_batch("CREATE TRIGGER refuse_payment BEFORE INSERT ON payments BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(apply_state(&mut c, &o, "paid").is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM recode_keys", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(!order(&c, "rcd-test").unwrap().unwrap().fulfilled);
    }
    #[test]
    fn deleted_accounts_and_mismatched_orders_get_nothing() {
        let (mut c, mut o) = fixture();
        o.days = 0;
        assert_eq!(apply_state(&mut c, &o, "paid"), Err("order-mismatch"));
        o.days = 30;
        c.execute("DELETE FROM users", []).unwrap();
        assert_eq!(apply_state(&mut c, &o, "paid"), Err("account-missing"));
        assert!(!order(&c, "rcd-test").unwrap().unwrap().fulfilled);
    }
    #[test]
    fn finance_and_access_commit_together() {
        let (mut c, o) = fixture();
        super::super::income::reserve(&c, &o.id, 29900, "RECODE", "sbp").unwrap();
        c.execute(
            "UPDATE rollypay_orders SET amount=26910 WHERE id=?1",
            [&o.id],
        )
        .unwrap();
        let o = order(&c, &o.id).unwrap().unwrap();
        assert_eq!(apply_state(&mut c, &o, "paid"), Err("payment-fee-pending"));
        assert!(!order(&c, &o.id).unwrap().unwrap().fulfilled);
        c.execute_batch("CREATE TRIGGER refuse_income BEFORE INSERT ON income_entries BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
        assert!(apply_state_fee(&mut c, &o, "paid", Some(("sbp".into(), 900))).is_err());
        assert!(!order(&c, &o.id).unwrap().unwrap().fulfilled);
        c.execute_batch("DROP TRIGGER refuse_income").unwrap();
        apply_state_fee(&mut c, &o, "paid", Some(("sbp".into(), 900))).unwrap();
        apply_state_fee(&mut c, &o, "paid", Some(("sbp".into(), 900))).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM income_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        apply_state(&mut c, &o, "refunded").unwrap();
        assert_eq!(
            c.query_row("SELECT sum(amount) FROM income_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(!crate::recode::of(&c, 42).active(now()));
        assert_eq!(
            provider_fee(&json!({"payment_method":"sbp","platform_fee_percent":"7.50"})),
            Some(("sbp".into(), 750))
        );
        assert_eq!(
            provider_fee(&json!({"payment_method":"crypto"})),
            Some(("crypto".into(), 300))
        );
        assert_eq!(provider_fee(&json!({"payment_method":"unknown"})), None);
    }
    #[test]
    fn recovery_claims_only_due_unfulfilled_orders() {
        let (c, o) = fixture();
        c.execute(
            "UPDATE rollypay_orders SET made=?2 WHERE id=?1",
            params![o.id, now()],
        )
        .unwrap();
        let first = due_orders(&c);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, o.id);
        assert!(due_orders(&c).is_empty());
        c.execute(
            "UPDATE rollypay_orders SET last_check=0,fulfilled=?2 WHERE id=?1",
            params![o.id, now()],
        )
        .unwrap();
        assert!(due_orders(&c).is_empty());
        c.execute(
            "UPDATE rollypay_orders SET fulfilled=NULL,state='refunded' WHERE id=?1",
            [&o.id],
        )
        .unwrap();
        assert!(due_orders(&c).is_empty());
    }
    #[test]
    fn amounts_and_prices() {
        for (s, n) in [
            ("299", 29900),
            ("299.0", 29900),
            ("299.00", 29900),
            ("299.00000000", 29900),
            ("299.01000000", 29901),
            ("299.10000000", 29910),
            ("0.01", 1),
        ] {
            assert_eq!(money(s), Some(n));
        }
        for s in [
            "-299",
            "NaN",
            "1e3",
            "299.001",
            "299.00000001",
            "299.00100000",
            " 299",
            ".01",
            "99999999999999999",
        ] {
            assert_eq!(money(s), None);
        }
        assert_eq!(price(30), Some(29900));
        assert_eq!(price(90), Some(49900));
        assert_eq!(price(0), Some(69900));
        assert_eq!(price(1), None);
    }
    #[test]
    fn refunds_replays_and_late_payments() {
        let (mut c, o) = fixture();
        apply_state(&mut c, &o, "expired").unwrap();
        apply_state(&mut c, &o, "paid").unwrap();
        apply_state(&mut c, &o, "created").unwrap();
        assert_eq!(order(&c, &o.id).unwrap().unwrap().state, "paid");
        apply_state(&mut c, &o, "refunded").unwrap();
        apply_state(&mut c, &o, "paid").unwrap();
        assert_eq!(order(&c, &o.id).unwrap().unwrap().state, "refunded");
        assert!(!crate::recode::of(&c, 42).active(now()));
        let (mut c, o) = fixture();
        apply_state(&mut c, &o, "chargeback").unwrap();
        apply_state(&mut c, &o, "paid").unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM recode_keys", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn signed_raw_body_and_payment_binding() {
        let stamp = now().to_string();
        let body = r#"{"amount":"299.00"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"test-secret").unwrap();
        mac.update(format!("{stamp}.{body}").as_bytes());
        let sig: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(signature_ok(body, &stamp, &sig, "test-secret"));
        assert!(!signature_ok(body, &stamp, &sig, "wrong"));
        assert!(!signature_ok("{}", &stamp, &sig, "test-secret"));
        assert!(!signature_ok(body, "bad", &sig, "test-secret"));
        assert!(!signature_ok(
            body,
            &stamp,
            "短短短短短短短短短短短短短短短短短短短短短",
            "test-secret"
        ));
        let (_, o) = fixture();
        let v = json!({"payment_id":o.bill,"order_id":o.id,"amount":"299.00000000","payment_currency":"RUB","status":"paid","test":false,"environment":"prod"});
        assert_eq!(verify_bill(&v, &o), Ok("paid".into()));
        for (field, value) in [
            ("payment_id", json!("other")),
            ("order_id", json!("other")),
            ("amount", json!("298.99")),
            ("payment_currency", json!("USD")),
            ("test", json!(true)),
            ("environment", json!("sandbox")),
            ("status", json!("unknown")),
        ] {
            let mut bad = v.clone();
            bad[field] = value;
            assert!(verify_bill(&bad, &o).is_err(), "{field}");
        }
        assert!(valid_url("https://pay.rollypay.io/pay/tok_abc-123"));
        assert!(!valid_url("https://pay.rollypay.io.evil/pay/tok_abc"));
        assert!(!valid_url("https://pay.rollypay.io/pay/../../evil"));
    }
}
