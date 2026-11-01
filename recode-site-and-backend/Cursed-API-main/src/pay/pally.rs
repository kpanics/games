//! Pally: fixed-price orders, authenticated bill verification and atomic fulfillment.
//! Protocol: https://pally.info/reference/api (26 October 2026).
use super::{md5_hex, url_form};
use crate::{blocking, db, fail, market::member, now, random_hex, Shared};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, env, io::Read, time::Duration};

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS pally_orders (
        id TEXT PRIMARY KEY, uid INTEGER NOT NULL, days INTEGER NOT NULL,
        amount INTEGER NOT NULL, bill TEXT UNIQUE, url TEXT, made INTEGER NOT NULL,
        fulfilled INTEGER, last_check INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX IF NOT EXISTS pally_orders_uid ON pally_orders(uid, made);",
    )
    .expect("pally orders");
}
fn price(days: i64) -> Option<i64> {
    match days {
        30 => Some(29900),
        90 => Some(49900),
        0 => Some(69900),
        _ => None,
    }
}
fn config() -> Result<(String, String), &'static str> {
    let token = env::var("PALLY_API_TOKEN").unwrap_or_default();
    let shop = env::var("PALLY_SHOP_ID").unwrap_or_default();
    if token.is_empty() || shop.is_empty() {
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
fn successful(v: &Value) -> bool {
    v["success"] == true || v["success"] == "true"
}
fn money(s: &str) -> Option<i64> {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
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
    !s.is_empty() && s.len() <= 80 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn valid_url(s: &str) -> bool {
    [
        "https://pally.info/transfer/",
        "https://pal24.pro/transfer/",
    ]
    .iter()
    .any(|p| s.strip_prefix(p).is_some_and(valid_id))
}

#[derive(Deserialize)]
struct Checkout {
    days: i64,
    agree: bool,
    #[serde(default)]
    method: String,
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
    let Some(amount) = price(input.days) else {
        return fail(StatusCode::BAD_REQUEST, "term");
    };
    if !input.agree || !["", "sbp", "card"].contains(&input.method.as_str()) {
        return fail(StatusCode::BAD_REQUEST, "checkout");
    }
    let (token, shop) = match config() {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    let id = format!("rcd-{}", random_hex(16));
    let (order, uid, days) = (id.clone(), w.uid, input.days);
    let reserved = db(&app, move |c| -> Result<(), &'static str> {
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM pally_orders WHERE uid=?1 AND made>?2",
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
        c.execute(
            "INSERT INTO pally_orders(id,uid,days,amount,made) VALUES(?1,?2,?3,?4,?5)",
            params![order, uid, days, amount, now()],
        )
        .map_err(|_| "busy")?;
        Ok(())
    })
    .await;
    if let Err(e) = reserved {
        return fail(StatusCode::CONFLICT, e);
    }
    let order = id.clone();
    let result = blocking(move || {
        let sum = format!("{}.{:02}", amount / 100, amount % 100);
        let description = if days == 0 {
            "Cursed Recode — навсегда".to_owned()
        } else {
            format!("Cursed Recode — {days} дней")
        };
        let method = match input.method.as_str() {
            "sbp" => "SBP",
            "card" => "BANK_CARD",
            _ => "",
        };
        let r = agent()
            .post("https://pal24.pro/api/v1/bill/create")
            .set("Authorization", &format!("Bearer {token}"))
            .send_form(&[
                ("amount", &sum),
                ("shop_id", &shop),
                ("order_id", &order),
                ("description", &description),
                ("name", &description),
                ("type", "normal"),
                ("currency_in", "RUB"),
                ("locale", "ru"),
                ("payer_pays_commission", "0"),
                ("ttl", "3600"),
                ("payment_method", method),
                ("success_url", "https://cursedclient.xyz/api/pay/ok"),
                ("fail_url", "https://cursedclient.xyz/api/pay/fail"),
                ("return_url", "https://cursedclient.xyz/#/buy"),
            ])
            .map_err(|_| "payment-provider")?;
        let v = read_json(r)?;
        let bill = v["bill_id"].as_str().unwrap_or("");
        let url = v["link_page_url"].as_str().unwrap_or("");
        if !successful(&v) || !valid_id(bill) || !valid_url(url) {
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
            "UPDATE pally_orders SET bill=?2,url=?3 WHERE id=?1",
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
}
fn order(c: &Connection, id: &str) -> rusqlite::Result<Option<Order>> {
    c.query_row("SELECT id,uid,days,amount,COALESCE(bill,''),fulfilled IS NOT NULL FROM pally_orders WHERE id=?1", [id], |r|
        Ok(Order { id:r.get(0)?,uid:r.get(1)?,days:r.get(2)?,amount:r.get(3)?,bill:r.get(4)?,fulfilled:r.get(5)? })).optional()
}
fn verify_bill(v: &Value, o: &Order) -> Result<bool, &'static str> {
    if !successful(v)
        || v["id"] != o.bill
        || v["order_id"] != o.id
        || v["currency_in"] != "RUB"
        || v["type"] != "NORMAL"
        || money_value(&v["amount"]) != Some(o.amount)
    {
        return Err("bill-mismatch");
    }
    Ok(matches!(v["status"].as_str(), Some("SUCCESS" | "OVERPAID")))
}
fn fulfill(c: &mut Connection, o: &Order) -> Result<(), &'static str> {
    let tx = c.transaction().map_err(|_| "busy")?;
    let current = order(&tx, &o.id)
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
        params![format!("PALLY-{}",o.id),o.days,format!("Pally {}",o.bill),at,o.uid,login]).map_err(|_| "busy")?;
    tx.execute("INSERT INTO payments(provider,intid,order_id,amount,currency,at) VALUES('pally',?1,?2,?3,'RUB',?4)",
        params![o.bill,o.id,format!("{}.{:02}",o.amount/100,o.amount%100),at]).map_err(|_| "busy")?;
    tx.execute(
        "UPDATE pally_orders SET fulfilled=?2 WHERE id=?1",
        params![o.id, at],
    )
    .map_err(|_| "busy")?;
    crate::people::audit(
        &tx,
        o.uid,
        "pay.pally",
        &o.id,
        json!({"bill":o.bill,"days":o.days,"amount":o.amount}),
    );
    tx.commit().map_err(|_| "busy")
}
async fn reconcile(app: &Shared, o: Order, token: String) -> Result<bool, &'static str> {
    if o.fulfilled {
        return Ok(true);
    }
    if !valid_id(&o.bill) {
        return Err("bill-pending");
    }
    let check = o.clone();
    let paid = blocking(move || {
        let r = agent()
            .get("https://pal24.pro/api/v1/bill/status")
            .query("id", &check.bill)
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|_| "payment-provider")?;
        verify_bill(&read_json(r)?, &check)
    })
    .await?;
    if paid {
        db(app, move |c| fulfill(c, &o)).await?;
    }
    Ok(paid)
}
fn signature_ok(fields: &HashMap<String, String>, token: &str) -> bool {
    let f = |k: &str| fields.get(k).map(String::as_str).unwrap_or("");
    let want = md5_hex(&format!("{}:{}:{token}", f("OutSum"), f("InvId"))).to_uppercase();
    let supplied = f("SignatureValue").to_uppercase();
    !token.is_empty()
        && supplied.len() == want.len()
        && supplied
            .bytes()
            .zip(want.bytes())
            .fold(0u8, |d, (a, b)| d | (a ^ b))
            == 0
}
async fn notified(State(app): State<Shared>, body: String) -> Response {
    let fields: HashMap<_, _> = url_form(&body).into_iter().collect();
    let (token, _) = match config() {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    if !signature_ok(&fields, &token) {
        return fail(StatusCode::FORBIDDEN, "signature");
    }
    let id = fields.get("InvId").cloned().unwrap_or_default();
    let o = match db(&app, move |c| order(c, &id)).await {
        Ok(Some(o)) => o,
        _ => return fail(StatusCode::BAD_REQUEST, "order-missing"),
    };
    if fields.get("TrsId") != Some(&o.bill)
        || fields.get("CurrencyIn").map(String::as_str) != Some("RUB")
        || fields
            .get("OutSum")
            .and_then(|v| money(v))
            .is_none_or(|v| v < o.amount)
    {
        return fail(StatusCode::BAD_REQUEST, "payment-mismatch");
    }
    // Status/currency/bill are not covered by Pally's MD5: confirm via authenticated API.
    match reconcile(&app, o, token).await {
        Ok(_) => "OK".into_response(),
        Err(e) => fail(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
async fn ready() -> Response {
    "Pally notification endpoint".into_response()
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
    if o.fulfilled {
        return Json(json!({"paid":true})).into_response();
    }
    let id = o.id.clone();
    let due = db(&app, move |c| {
        c.execute(
            "UPDATE pally_orders SET last_check=?2 WHERE id=?1 AND last_check<?3",
            params![id, now(), now() - 10],
        )
    })
    .await
    .unwrap_or(0)
        == 1;
    if !due {
        return Json(json!({"paid":false})).into_response();
    }
    let (token, _) = match config() {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    match reconcile(&app, o, token).await {
        Ok(paid) => Json(json!({"paid":paid})).into_response(),
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
        .get("InvId")
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
        include_str!("pally-result.js"),
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
        .route("/api/pay/pally", get(ready).post(notified))
        .route("/api/pay/pally/create", post(checkout))
        .route("/api/pay/pally/status", get(status))
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
            INSERT INTO pally_orders(id,uid,days,amount,bill,made) VALUES('rcd-test',42,30,29900,'bill1',0);").unwrap();
        let o = order(&c, "rcd-test").unwrap().unwrap();
        (c, o)
    }
    #[test]
    fn fulfillment_is_atomic_and_idempotent() {
        let (mut c, o) = fixture();
        fulfill(&mut c, &o).unwrap();
        fulfill(&mut c, &o).unwrap();
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
        assert!(fulfill(&mut c, &o).is_err());
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
        assert_eq!(fulfill(&mut c, &o), Err("order-mismatch"));
        o.days = 30;
        c.execute("DELETE FROM users", []).unwrap();
        assert_eq!(fulfill(&mut c, &o), Err("account-missing"));
        assert!(!order(&c, "rcd-test").unwrap().unwrap().fulfilled);
    }
    #[test]
    fn amounts_and_prices() {
        for (s, n) in [
            ("299", 29900),
            ("299.0", 29900),
            ("299.00", 29900),
            ("0.01", 1),
        ] {
            assert_eq!(money(s), Some(n));
        }
        for s in [
            "-299",
            "NaN",
            "1e3",
            "299.001",
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
    fn signature_and_bill_binding() {
        let mut f: HashMap<String, String> = [("InvId", "rcd-1"), ("OutSum", "299.00")]
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect();
        f.insert("SignatureValue".into(), md5_hex("299.00:rcd-1:test"));
        assert!(signature_ok(&f, "test"));
        assert!(!signature_ok(&f, ""));
        assert!(!signature_ok(&f, "wrong"));
        f.insert("OutSum".into(), "699".into());
        assert!(!signature_ok(&f, "test"));
        let o = Order {
            id: "rcd-1".into(),
            bill: "bill1".into(),
            uid: 1,
            days: 30,
            amount: 29900,
            fulfilled: false,
        };
        let v = json!({"success":true,"id":"bill1","order_id":"rcd-1","currency_in":"RUB","type":"NORMAL","amount":"299.00","status":"SUCCESS"});
        assert_eq!(verify_bill(&v, &o), Ok(true));
        for (field, value) in [
            ("id", "bill2"),
            ("order_id", "rcd-2"),
            ("currency_in", "USD"),
            ("type", "MULTI"),
            ("amount", "298"),
        ] {
            let mut bad = v.clone();
            bad[field] = json!(value);
            assert!(verify_bill(&bad, &o).is_err());
        }
        for state in ["NEW", "PROCESS", "UNDERPAID", "FAIL"] {
            let mut pending = v.clone();
            pending["status"] = json!(state);
            assert_eq!(verify_bill(&pending, &o), Ok(false));
        }
        assert!(valid_url("https://pally.info/transfer/Abc1"));
        assert!(!valid_url("https://pally.info.evil/transfer/Abc1"));
    }
}
