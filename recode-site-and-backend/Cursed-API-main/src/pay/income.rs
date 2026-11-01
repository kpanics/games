//! Promo snapshots and ruble income ledger. No bank transfers are initiated here.
use crate::{db, fail, market::member, now, Shared, OWNER};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn schema(c: &Connection) {
    c.execute_batch("CREATE TABLE IF NOT EXISTS income_config(id INTEGER PRIMARY KEY CHECK(id=1),admin_rate INTEGER NOT NULL,admin2_rate INTEGER NOT NULL,media_rate INTEGER NOT NULL); INSERT OR IGNORE INTO income_config VALUES(1,2000,500,1500);
CREATE TABLE IF NOT EXISTS promo_codes(id INTEGER PRIMARY KEY AUTOINCREMENT,code TEXT NOT NULL UNIQUE COLLATE NOCASE,owner INTEGER UNIQUE,discount INTEGER NOT NULL,share INTEGER NOT NULL,active INTEGER NOT NULL DEFAULT 1,made INTEGER NOT NULL);
INSERT OR IGNORE INTO promo_codes(code,owner,discount,share,made) VALUES('RECODE',NULL,1000,0,strftime('%s','now'));
CREATE TABLE IF NOT EXISTS promo_names(name TEXT PRIMARY KEY COLLATE NOCASE,promo INTEGER NOT NULL);
INSERT OR IGNORE INTO promo_names SELECT code,id FROM promo_codes;
CREATE TABLE IF NOT EXISTS income_orders(id TEXT PRIMARY KEY,list_price INTEGER NOT NULL,amount INTEGER NOT NULL,promo_id INTEGER,promo TEXT NOT NULL,media_uid INTEGER,discount INTEGER NOT NULL,admin_rate INTEGER NOT NULL,admin2_rate INTEGER NOT NULL,media_rate INTEGER NOT NULL,method TEXT NOT NULL,made INTEGER NOT NULL,fee INTEGER,fee_rate INTEGER,net INTEGER,settled INTEGER,reversed INTEGER);
CREATE TABLE IF NOT EXISTS income_entries(order_id TEXT NOT NULL,uid INTEGER NOT NULL,kind TEXT NOT NULL,amount INTEGER NOT NULL,at INTEGER NOT NULL,PRIMARY KEY(order_id,uid,kind));
CREATE INDEX IF NOT EXISTS income_entries_uid ON income_entries(uid,at);
CREATE TABLE IF NOT EXISTS income_payouts(id TEXT PRIMARY KEY,uid INTEGER NOT NULL,amount INTEGER NOT NULL CHECK(amount>0),note TEXT NOT NULL,at INTEGER NOT NULL,actor INTEGER NOT NULL);").expect("income schema");
    // The first checkout version could leave a paid purchase unfulfilled. Preserve
    // fulfilled history, but attach the agreed shares to unfinished full-price orders
    // before the recovery worker verifies their real provider status.
    c.execute("INSERT OR IGNORE INTO income_orders(id,list_price,amount,promo,discount,admin_rate,admin2_rate,media_rate,method,made)
        SELECT o.id,o.amount,o.amount,'',0,t.admin_rate,t.admin2_rate,0,'',o.made
        FROM rollypay_orders o CROSS JOIN income_config t
        WHERE t.id=1 AND o.fulfilled IS NULL AND o.bill IS NOT NULL
        AND o.state IN ('created','processing','expired','paid') AND o.made>?1
        AND ((o.days=30 AND o.amount=29900) OR (o.days=90 AND o.amount=49900) OR (o.days=0 AND o.amount=69900))",
        [now()-7*86400]).expect("unfinished full-price order snapshots");
}
pub fn part(amount: i64, rate: i64) -> i64 {
    (amount * rate + 5000) / 10000
}
fn normal(code: &str) -> Result<String, &'static str> {
    let c = code.trim().to_ascii_uppercase();
    if c.len() < 3
        || c.len() > 24
        || !c
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        Err("promo-format")
    } else {
        Ok(c)
    }
}
fn media(uid: i64) -> bool {
    crate::people::roles_of(uid).iter().any(|r| r == "media")
}
fn allowed(c: &Connection, uid: i64) -> bool {
    crate::is_owner(uid)
        || (uid == 622 && config(c).is_ok_and(|(rate, _, _)| rate > 0))
        || uid == 19
        || media(uid)
        || c.query_row(
            "SELECT EXISTS(SELECT 1 FROM promo_codes WHERE owner=?1)",
            [uid],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false)
}
fn config(c: &Connection) -> Result<(i64, i64, i64), &'static str> {
    c.query_row(
        "SELECT admin_rate,admin2_rate,media_rate FROM income_config WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .map_err(|_| "busy")
}
fn coupon(
    c: &Connection,
    code: &str,
) -> Result<(Option<i64>, String, Option<i64>, i64, i64), &'static str> {
    if code.trim().is_empty() {
        return Ok((None, String::new(), None, 0, 0));
    }
    let code = normal(code)?;
    c.query_row(
        "SELECT id,code,owner,discount,share FROM promo_codes WHERE code=?1 AND active=1",
        [code],
        |r| Ok((Some(r.get(0)?), r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()
    .map_err(|_| "busy")?
    .ok_or("promo-invalid")
}
pub fn reserve(
    c: &Connection,
    id: &str,
    list: i64,
    code: &str,
    method: &str,
) -> Result<i64, &'static str> {
    let (pid, name, uid, discount, share) = coupon(c, code)?;
    let (a, b, _) = config(c)?;
    if a + b + share > 10000 {
        return Err("income-rates");
    };
    let amount = list - part(list, discount);
    c.execute("INSERT INTO income_orders(id,list_price,amount,promo_id,promo,media_uid,discount,admin_rate,admin2_rate,media_rate,method,made) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,list,amount,pid,name,uid,discount,a,b,share,method,now()]).map_err(|_|"busy")?;
    Ok(amount)
}
pub fn tracked(c: &Connection, id: &str) -> bool {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM income_orders WHERE id=?1)",
        [id],
        |r| r.get(0),
    )
    .unwrap_or(false)
}
/// Called inside the same transaction that grants access. Exact method comes from authenticated provider GET.
pub fn settle(c: &Connection, id: &str, fee: Option<(&str, i64)>) -> Result<(), &'static str> {
    let row=c.query_row("SELECT amount,admin_rate,admin2_rate,media_rate,media_uid,settled,reversed FROM income_orders WHERE id=?1",[id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<i64>>(4)?,r.get::<_,Option<i64>>(5)?,r.get::<_,Option<i64>>(6)?))).optional().map_err(|_|"busy")?;
    let Some((amount, a, b, m, uid, settled, reversed)) = row else {
        return Ok(());
    };
    if settled.is_some() || reversed.is_some() {
        return Ok(());
    };
    let (method, rate) = fee.ok_or("payment-fee-pending")?;
    if !(0..=10000).contains(&rate) {
        return Err("payment-fee");
    };
    let f = part(amount, rate);
    let net = amount - f;
    let av = part(net, a);
    let bv = part(net, b).min(net - av);
    let mv = part(net, m).min(net - av - bv);
    let owner = net - av - bv - mv;
    if owner < 0 {
        return Err("income-rates");
    };
    for (who, value) in [(OWNER, owner), (622, av), (19, bv)] {
        c.execute(
            "INSERT INTO income_entries VALUES(?1,?2,'earned',?3,?4)",
            params![id, who, value, now()],
        )
        .map_err(|_| "busy")?;
    }
    if let Some(who) = uid {
        if mv > 0 {
            c.execute("INSERT INTO income_entries(order_id,uid,kind,amount,at) VALUES(?1,?2,'earned',?3,?4) ON CONFLICT(order_id,uid,kind) DO UPDATE SET amount=amount+excluded.amount",params![id,who,mv,now()]).map_err(|_|"busy")?;
        }
    }
    c.execute(
        "UPDATE income_orders SET method=?2,fee=?3,fee_rate=?4,net=?5,settled=?6 WHERE id=?1",
        params![id, method, f, rate, net, now()],
    )
    .map_err(|_| "busy")?;
    Ok(())
}
pub fn reverse(c: &Connection, id: &str) -> Result<(), &'static str> {
    c.execute("INSERT OR IGNORE INTO income_entries(order_id,uid,kind,amount,at) SELECT order_id,uid,'reversed',-amount,?2 FROM income_entries WHERE order_id=?1 AND kind='earned'",params![id,now()]).map_err(|_|"busy")?;
    c.execute(
        "UPDATE income_orders SET reversed=COALESCE(reversed,?2) WHERE id=?1",
        params![id, now()],
    )
    .map_err(|_| "busy")?;
    Ok(())
}
fn balance(c: &Connection, uid: i64) -> rusqlite::Result<i64> {
    c.query_row("SELECT COALESCE((SELECT sum(amount) FROM income_entries WHERE uid=?1),0)-COALESCE((SELECT sum(amount) FROM income_payouts WHERE uid=?1),0)",[uid],|r|r.get(0))
}
fn response(v: Result<Value, &'static str>) -> Response {
    match v {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(
            if e == "forbidden" {
                StatusCode::FORBIDDEN
            } else if e == "busy" {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::BAD_REQUEST
            },
            e,
        ),
    }
}
#[derive(Deserialize)]
struct Quote {
    days: i64,
    #[serde(default)]
    promo: String,
}
async fn quote(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Quote>) -> Response {
    if let Err(r) = member(&app, &h, false).await {
        return r;
    }
    let Some(list) = super::rollypay::price(q.days) else {
        return fail(StatusCode::BAD_REQUEST, "term");
    };
    response(db(&app,move|c|{let(_,name,_,discount,_)=coupon(c,&q.promo)?;Ok(json!({"promo":name,"discount":discount,"list_price":list,"amount":list-part(list,discount)}))}).await)
}
#[derive(Deserialize)]
struct Period {
    #[serde(default = "month")]
    days: i64,
}
fn month() -> i64 {
    30
}
fn data(c: &Connection, uid: i64, days: i64) -> Result<Value, &'static str> {
    if !allowed(c, uid) {
        return Err("forbidden");
    };
    let owner = crate::is_owner(uid);
    let from = now() - days.clamp(1, 365) * 86400;
    let (a, b, m) = config(c)?;
    let mut st=c.prepare("SELECT i.id,o.days,o.state,i.amount,i.list_price,i.promo,i.discount,i.method,i.fee,i.net,i.settled,i.reversed,i.made,i.admin_rate,i.admin2_rate,i.media_rate,i.media_uid,COALESCE((SELECT sum(amount) FROM income_entries WHERE order_id=i.id AND uid=?1),0),COALESCE((SELECT amount FROM income_entries WHERE order_id=i.id AND uid=?1 AND kind='earned'),0) FROM income_orders i JOIN rollypay_orders o ON o.id=i.id WHERE i.made>=?2 AND (?3 OR EXISTS(SELECT 1 FROM income_entries WHERE order_id=i.id AND uid=?1)) ORDER BY i.made DESC LIMIT 100").map_err(|_|"busy")?;
    let orders=st.query_map(params![uid,from,owner],|r|{
 let mut v=json!({"id":r.get::<_,String>(0)?,"days":r.get::<_,i64>(1)?,"state":r.get::<_,String>(2)?,"amount":r.get::<_,i64>(3)?,"list_price":r.get::<_,i64>(4)?,"promo":r.get::<_,String>(5)?,"discount":r.get::<_,i64>(6)?,"method":r.get::<_,String>(7)?,"settled":r.get::<_,Option<i64>>(10)?,"reversed":r.get::<_,Option<i64>>(11)?,"made":r.get::<_,i64>(12)?,"mine":r.get::<_,i64>(17)?,"earned":r.get::<_,i64>(18)?});
 if owner{v["fee"]=json!(r.get::<_,Option<i64>>(8)?);v["net"]=json!(r.get::<_,Option<i64>>(9)?);v["admin_rate"]=json!(r.get::<_,i64>(13)?);v["admin2_rate"]=json!(r.get::<_,i64>(14)?);v["media_rate"]=json!(r.get::<_,i64>(15)?);v["media_uid"]=json!(r.get::<_,Option<i64>>(16)?);}Ok(v)
 }).map_err(|_|"busy")?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"busy")?;
    let mut st=c.prepare("SELECT p.id,p.code,p.owner,COALESCE(u.login,''),p.discount,p.share,p.active,COALESCE((SELECT count(*) FROM income_orders i WHERE i.promo_id=p.id AND settled IS NOT NULL AND reversed IS NULL),0),COALESCE((SELECT sum(e.amount) FROM income_entries e JOIN income_orders i ON i.id=e.order_id WHERE i.promo_id=p.id AND e.uid=p.owner),0) FROM promo_codes p LEFT JOIN users u ON u.uid=p.owner WHERE ?1 OR p.owner=?2 ORDER BY p.id").map_err(|_|"busy")?;
    let codes=st.query_map(params![owner,uid],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"code":r.get::<_,String>(1)?,"uid":r.get::<_,Option<i64>>(2)?,"login":r.get::<_,String>(3)?,"discount":r.get::<_,i64>(4)?,"share":r.get::<_,i64>(5)?,"active":r.get::<_,i64>(6)?==1,"purchases":r.get::<_,i64>(7)?,"income":r.get::<_,i64>(8)?}))).map_err(|_|"busy")?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"busy")?;
    let mut st=c.prepare("SELECT p.id,p.uid,COALESCE(u.login,''),p.amount,p.note,p.at FROM income_payouts p LEFT JOIN users u ON u.uid=p.uid WHERE ?1 OR p.uid=?2 ORDER BY p.at DESC LIMIT 100").map_err(|_|"busy")?;
    let payouts=st.query_map(params![owner,uid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"uid":r.get::<_,i64>(1)?,"login":r.get::<_,String>(2)?,"amount":r.get::<_,i64>(3)?,"note":r.get::<_,String>(4)?,"at":r.get::<_,i64>(5)?}))).map_err(|_|"busy")?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"busy")?;
    let paid: i64 = c
        .query_row(
            "SELECT COALESCE(sum(amount),0) FROM income_payouts WHERE uid=?1",
            [uid],
            |r| r.get(0),
        )
        .map_err(|_| "busy")?;
    let mut recipients = vec![];
    if owner {
        let mut s=c.prepare("SELECT u.uid,u.login FROM users u WHERE u.uid IN (622,19) OR EXISTS(SELECT 1 FROM promo_codes p WHERE p.owner=u.uid) ORDER BY u.uid").map_err(|_|"busy")?;
        for r in s
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|_| "busy")?
        {
            let (id, login) = r.map_err(|_| "busy")?;
            recipients.push(
                json!({"uid":id,"login":login,"available":balance(c,id).map_err(|_|"busy")?}),
            );
        }
    }
    let mut summary = json!({});
    for (key,sql) in [
 ("mine","SELECT COALESCE(sum(e.amount),0) FROM income_entries e JOIN income_orders i ON i.id=e.order_id WHERE e.uid=?1 AND i.made>=?2"),
 ("count","SELECT count(*) FROM income_orders i WHERE i.made>=?2 AND i.settled IS NOT NULL AND i.reversed IS NULL AND (?3 OR EXISTS(SELECT 1 FROM income_entries e WHERE e.order_id=i.id AND e.uid=?1))"),
 ("amount","SELECT COALESCE(sum(i.amount),0) FROM income_orders i WHERE ?3 AND i.made>=?2 AND i.settled IS NOT NULL AND i.reversed IS NULL AND ?1>=0"),
 ("fee","SELECT COALESCE(sum(i.fee),0) FROM income_orders i WHERE ?3 AND i.made>=?2 AND i.settled IS NOT NULL AND i.reversed IS NULL AND ?1>=0"),
 ("admins","SELECT COALESCE(sum((i.net*i.admin_rate+5000)/10000+min((i.net*i.admin2_rate+5000)/10000,i.net-((i.net*i.admin_rate+5000)/10000))),0) FROM income_orders i WHERE ?3 AND i.made>=?2 AND i.settled IS NOT NULL AND i.reversed IS NULL AND ?1>=0"),
 ("media","SELECT COALESCE(sum(CASE WHEN i.reversed IS NULL THEN min((i.net*i.media_rate+5000)/10000,i.net-((i.net*i.admin_rate+5000)/10000)-min((i.net*i.admin2_rate+5000)/10000,i.net-((i.net*i.admin_rate+5000)/10000))) ELSE 0 END),0) FROM income_orders i WHERE ?3 AND i.made>=?2 AND i.settled IS NOT NULL AND ?1>=0")
 ] { // Named bindings keep the statements' parameter counts independent.
 let mut st=c.prepare(sql).map_err(|_|"busy")?;
 if st.parameter_count()>=1 {st.raw_bind_parameter(1,uid).map_err(|_|"busy")?;}
 if st.parameter_count()>=2 {st.raw_bind_parameter(2,from).map_err(|_|"busy")?;}
 if st.parameter_count()>=3 {st.raw_bind_parameter(3,owner).map_err(|_|"busy")?;}
 summary[key]=json!(st.raw_query().next().map_err(|_|"busy")?.ok_or("busy")?.get::<_,i64>(0).map_err(|_|"busy")?);
 }
    let mut chart_stmt=c.prepare("SELECT strftime('%Y-%m-%d',i.made,'unixepoch'),sum(e.amount) FROM income_entries e JOIN income_orders i ON i.id=e.order_id WHERE e.uid=?1 AND i.made>=?2 GROUP BY 1 ORDER BY 1").map_err(|_|"busy")?;
    let chart = chart_stmt
        .query_map(params![uid, from], |r| {
            Ok(json!({"date":r.get::<_,String>(0)?,"amount":r.get::<_,i64>(1)?}))
        })
        .map_err(|_| "busy")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "busy")?;
    Ok(
        json!({"summary":summary,"chart":chart,"owner":owner,"media":media(uid),"uid":uid,"rate":if uid==622{a}else if uid==19{b}else{codes.first().and_then(|v|v["share"].as_i64()).unwrap_or(if media(uid){m}else{0})},"available":balance(c,uid).map_err(|_|"busy")?,"paid":paid,"orders":orders,"codes":codes,"payouts":payouts,"recipients":recipients,"rates":if owner{json!({"admin":a,"admin2":b,"media":m})}else{Value::Null}}),
    )
}
async fn dashboard(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Period>) -> Response {
    let w = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    response(db(&app, move |c| data(c, w.uid, q.days)).await)
}
#[derive(Deserialize)]
struct CodeInput {
    #[serde(default)]
    id: Option<i64>,
    code: String,
    #[serde(default)]
    uid: Option<i64>,
    #[serde(default)]
    discount: Option<i64>,
    #[serde(default)]
    share: Option<i64>,
    #[serde(default)]
    active: Option<bool>,
}
fn save_code(c: &mut Connection, actor: i64, i: CodeInput) -> Result<Value, &'static str> {
    let name = normal(&i.code)?;
    let owner = crate::is_owner(actor);
    let tx = c.transaction().map_err(|_| "busy")?;
    let (a, b, default) = config(&tx)?;
    let result = if let Some(id) = i.id {
        let old = tx
            .query_row(
                "SELECT owner,discount,share,code,active FROM promo_codes WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| "busy")?
            .ok_or("promo-invalid")?;
        if !owner && old.0 != Some(actor) {
            return Err("forbidden");
        };
        if !owner
            && (i.uid.is_some() || i.discount.is_some() || i.share.is_some() || i.active.is_some())
        {
            return Err("forbidden");
        };
        let discount = if owner {
            i.discount.unwrap_or(old.1)
        } else {
            old.1
        };
        let share = if owner {
            i.share.unwrap_or(old.2)
        } else {
            old.2
        };
        if !(0..=9000).contains(&discount)
            || share < 0
            || a + b + share > 10000
            || (old.0.is_none() && share != 0)
        {
            return Err("income-rates");
        };
        if old.3.eq_ignore_ascii_case("RECODE")
            && (name != "RECODE" || discount != 1000 || share != 0 || i.active == Some(false))
        {
            return Err("promo-reserved");
        };
        let previous = tx
            .query_row(
                "SELECT promo FROM promo_names WHERE name=?1",
                [&name],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| "busy")?;
        if previous.is_some_and(|p| p != id) {
            return Err("promo-taken");
        };
        tx.execute(
            "INSERT OR IGNORE INTO promo_names VALUES(?1,?2)",
            params![name, id],
        )
        .map_err(|_| "promo-taken")?;
        tx.execute(
            "UPDATE promo_codes SET code=?2,discount=?3,share=?4,active=?5 WHERE id=?1",
            params![
                id,
                name,
                discount,
                share,
                i.active.map(i64::from).unwrap_or(old.4)
            ],
        )
        .map_err(|_| "promo-taken")?;
        id
    } else {
        if !owner && !media(actor) {
            return Err("forbidden");
        };
        let who = if owner { i.uid } else { Some(actor) };
        if let Some(uid) = who {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1)",
                    [uid],
                    |r| r.get(0),
                )
                .map_err(|_| "busy")?;
            if !exists {
                return Err("account-missing");
            }
        }
        let discount = if owner {
            i.discount.unwrap_or(1500)
        } else {
            1500
        };
        let share = if owner {
            i.share.unwrap_or(if who.is_some() { default } else { 0 })
        } else {
            default
        };
        if !(0..=9000).contains(&discount)
            || share < 0
            || a + b + share > 10000
            || (who.is_none() && share != 0)
        {
            return Err("income-rates");
        };
        if tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM promo_names WHERE name=?1)",
                [&name],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| "busy")?
        {
            return Err("promo-taken");
        };
        tx.execute(
            "INSERT INTO promo_codes(code,owner,discount,share,made) VALUES(?1,?2,?3,?4,?5)",
            params![name, who, discount, share, now()],
        )
        .map_err(|_| "promo-taken")?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO promo_names VALUES(?1,?2)", params![name, id])
            .map_err(|_| "busy")?;
        id
    };
    crate::people::audit(
        &tx,
        actor,
        "income.promo",
        &result.to_string(),
        json!({"code":name}),
    );
    tx.commit().map_err(|_| "busy")?;
    Ok(json!({"ok":true,"id":result}))
}
async fn code_save(State(app): State<Shared>, h: HeaderMap, Json(i): Json<CodeInput>) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    response(db(&app, move |c| save_code(c, w.uid, i)).await)
}
#[derive(Deserialize)]
struct Rates {
    admin: i64,
    admin2: i64,
    media: i64,
}
async fn rates_save(State(app): State<Shared>, h: HeaderMap, Json(i): Json<Rates>) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    if !crate::is_owner(w.uid) {
        return fail(StatusCode::FORBIDDEN, "forbidden");
    };
    response(
        db(&app, move |c| {
            if [i.admin, i.admin2, i.media]
                .iter()
                .any(|x| *x < 0 || *x > 10000)
                || i.admin + i.admin2 + i.media > 10000
            {
                return Err("income-rates");
            };
            let max: i64 = c
                .query_row("SELECT COALESCE(max(share),0) FROM promo_codes", [], |r| {
                    r.get(0)
                })
                .map_err(|_| "busy")?;
            if i.admin + i.admin2 + max > 10000 {
                return Err("income-rates");
            };
            let tx = c.transaction().map_err(|_| "busy")?;
            tx.execute(
                "UPDATE income_config SET admin_rate=?1,admin2_rate=?2,media_rate=?3 WHERE id=1",
                params![i.admin, i.admin2, i.media],
            )
            .map_err(|_| "busy")?;
            crate::people::audit(
                &tx,
                w.uid,
                "income.rates",
                "",
                json!({"admin":i.admin,"admin2":i.admin2,"media":i.media}),
            );
            tx.commit().map_err(|_| "busy")?;
            Ok(json!({"ok":true}))
        })
        .await,
    )
}
#[derive(Deserialize)]
struct Payout {
    id: String,
    uid: i64,
    amount: i64,
    #[serde(default)]
    note: String,
}
fn payout(c: &mut Connection, actor: i64, i: Payout) -> Result<Value, &'static str> {
    if !crate::is_owner(actor) {
        return Err("forbidden");
    };
    if i.id.len() != 32
        || !i.id.bytes().all(|b| b.is_ascii_hexdigit())
        || i.amount <= 0
        || i.amount > 100_000_000
        || i.note.chars().count() > 250
    {
        return Err("payout-invalid");
    };
    let tx = c.transaction().map_err(|_| "busy")?;
    if let Some((uid, amount, note)) = tx
        .query_row(
            "SELECT uid,amount,note FROM income_payouts WHERE id=?1",
            [&i.id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| "busy")?
    {
        return if uid == i.uid && amount == i.amount && note == i.note {
            Ok(json!({"ok":true}))
        } else {
            Err("payout-conflict")
        };
    }
    if i.uid == OWNER || !allowed(&tx, i.uid) {
        return Err("forbidden");
    };
    if balance(&tx, i.uid).map_err(|_| "busy")? < i.amount {
        return Err("payout-balance");
    };
    tx.execute(
        "INSERT INTO income_payouts VALUES(?1,?2,?3,?4,?5,?6)",
        params![i.id, i.uid, i.amount, i.note, now(), actor],
    )
    .map_err(|_| "busy")?;
    crate::people::audit(
        &tx,
        actor,
        "income.payout",
        &i.id,
        json!({"uid":i.uid,"amount":i.amount,"note":i.note}),
    );
    tx.commit().map_err(|_| "busy")?;
    Ok(json!({"ok":true}))
}
async fn payout_save(State(app): State<Shared>, h: HeaderMap, Json(i): Json<Payout>) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    response(db(&app, move |c| payout(c, w.uid, i)).await)
}
async fn access(State(app): State<Shared>, h: HeaderMap) -> Response {
    let w = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    Json(db(&app, move |c| json!({"allowed":allowed(c,w.uid)})).await).into_response()
}
pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/income/access", get(access))
        .route("/api/pay/quote", get(quote))
        .route("/api/income", get(dashboard))
        .route("/api/income/code", post(code_save))
        .route("/api/income/rates", post(rates_save))
        .route("/api/income/payout", post(payout_save))
        .layer(DefaultBodyLimit::max(4096))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        super::super::schema(&c);
        c.execute_batch("CREATE TABLE users(uid INTEGER PRIMARY KEY,login TEXT);INSERT INTO users VALUES(0,'Lunuca'),(19,'Andrey_AKM'),(622,'Janade'),(42,'media'),(77,'buyer');CREATE TABLE audit(at INTEGER,actor INTEGER,action TEXT,target TEXT,details TEXT);").unwrap();
        c
    }
    fn code(c: &mut Connection, name: &str, who: Option<i64>) -> i64 {
        save_code(
            c,
            0,
            CodeInput {
                id: None,
                code: name.into(),
                uid: who,
                discount: Some(1500),
                share: Some(if who.is_some() { 1500 } else { 0 }),
                active: None,
            },
        )
        .unwrap()["id"]
            .as_i64()
            .unwrap()
    }
    fn reserve_order(c: &Connection, id: &str, name: &str) -> i64 {
        let amount = reserve(c, id, 29900, name, "sbp").unwrap();
        c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,made,state) VALUES(?1,77,30,?2,?3,'paid')",params![id,amount,now()]).unwrap();
        amount
    }
    #[test]
    fn disabling_share_removes_access_preserves_earned_income() {
        let c = fixture();
        assert!(allowed(&c, 622));
        reserve_order(&c, "earned", "");
        settle(&c, "earned", Some(("sbp", 900))).unwrap();
        let earned = balance(&c, 622).unwrap();
        assert!(earned > 0);
        c.execute("UPDATE income_config SET admin_rate=0 WHERE id=1", []).unwrap();
        assert!(!allowed(&c, 622));
        assert!(allowed(&c, 0));
        assert!(allowed(&c, 19));
        reserve_order(&c, "after", "");
        settle(&c, "after", Some(("sbp", 900))).unwrap();
        assert_eq!(balance(&c, 622).unwrap(), earned);
    }
    #[test]
    fn exact_discounts_and_conservation() {
        let mut c = fixture();
        code(&mut c, "FRAME", Some(42));
        assert_eq!(reserve_order(&c, "m", "frame"), 25415);
        settle(&c, "m", Some(("sbp", 900))).unwrap();
        assert_eq!(balance(&c, 622).unwrap(), 4626);
        assert_eq!(balance(&c, 19).unwrap(), 1156);
        assert_eq!(balance(&c, 42).unwrap(), 3469);
        assert_eq!(balance(&c, 0).unwrap(), 13877);
        assert_eq!(
            c.query_row("SELECT sum(amount) FROM income_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            23128
        );
        assert_eq!(reserve_order(&c, "r", "recode"), 26910);
        assert_eq!(reserve_order(&c, "plain", ""), 29900);
        assert_eq!(
            reserve(&c, "bad", 29900, "unknown", "sbp"),
            Err("promo-invalid")
        );
    }
    #[test]
    fn no_duplicate_and_reverse_after_paid_out() {
        let mut c = fixture();
        code(&mut c, "FRAME", Some(42));
        reserve_order(&c, "m", "FRAME");
        settle(&c, "m", Some(("crypto", 300))).unwrap();
        let earned = balance(&c, 42).unwrap();
        payout(
            &mut c,
            0,
            Payout {
                id: "a".repeat(32),
                uid: 42,
                amount: earned,
                note: "transfer".into(),
            },
        )
        .unwrap();
        payout(
            &mut c,
            0,
            Payout {
                id: "a".repeat(32),
                uid: 42,
                amount: earned,
                note: "transfer".into(),
            },
        )
        .unwrap();
        settle(&c, "m", Some(("crypto", 300))).unwrap();
        reverse(&c, "m").unwrap();
        reverse(&c, "m").unwrap();
        settle(&c, "m", Some(("crypto", 300))).unwrap();
        assert_eq!(balance(&c, 42).unwrap(), -earned);
        assert_eq!(balance(&c, 622).unwrap(), 0);
        assert!(payout(
            &mut c,
            0,
            Payout {
                id: "b".repeat(32),
                uid: 42,
                amount: 1,
                note: String::new()
            }
        )
        .is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM income_payouts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn snapshots_rename_and_private_views() {
        let mut c = fixture();
        let id = code(&mut c, "FRAME", Some(42));
        reserve_order(&c, "old", "FRAME");
        save_code(
            &mut c,
            42,
            CodeInput {
                id: Some(id),
                code: "NEWCODE".into(),
                uid: None,
                discount: None,
                share: None,
                active: None,
            },
        )
        .unwrap();
        assert!(reserve(&c, "no", 29900, "FRAME", "sbp").is_err());
        assert!(save_code(
            &mut c,
            0,
            CodeInput {
                id: None,
                code: "FRAME".into(),
                uid: Some(77),
                discount: None,
                share: None,
                active: None
            }
        )
        .is_err());
        c.execute("UPDATE income_config SET admin_rate=1000", [])
            .unwrap();
        c.execute(
            "UPDATE promo_codes SET share=2000,discount=2000 WHERE id=?1",
            [id],
        )
        .unwrap();
        settle(&c, "old", Some(("sbp", 900))).unwrap();
        assert_eq!(balance(&c, 622).unwrap(), 4626);
        let d = data(&c, 42, 30).unwrap();
        assert_eq!(d["orders"][0]["promo"], "FRAME");
        assert!(d["orders"][0].get("fee").is_none());
        assert!(d["rates"].is_null());
        assert_eq!(d["codes"].as_array().unwrap().len(), 1);
        assert_eq!(data(&c, 77, 30), Err("forbidden"));
        assert!(
            data(&c, 0, 30).unwrap()["summary"]["amount"]
                .as_i64()
                .unwrap()
                > 0
        );
    }
    #[test]
    fn permissions_and_payout_limits() {
        let mut c = fixture();
        let id = code(&mut c, "FRAME", Some(42));
        assert!(save_code(
            &mut c,
            77,
            CodeInput {
                id: Some(id),
                code: "STOLEN".into(),
                uid: None,
                discount: None,
                share: None,
                active: None
            }
        )
        .is_err());
        assert!(save_code(
            &mut c,
            42,
            CodeInput {
                id: Some(id),
                code: "FRAME".into(),
                uid: None,
                discount: Some(0),
                share: None,
                active: None
            }
        )
        .is_err());
        assert!(payout(
            &mut c,
            19,
            Payout {
                id: "a".repeat(32),
                uid: 19,
                amount: 1,
                note: String::new()
            }
        )
        .is_err());
        assert!(payout(
            &mut c,
            0,
            Payout {
                id: "a".repeat(32),
                uid: 19,
                amount: 1,
                note: String::new()
            }
        )
        .is_err());
    }
    #[test]
    fn upgrade_snapshots_unfinished_full_price_orders_only() {
        let c = fixture();
        c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,bill,made) VALUES('legacy',77,30,29900,'bill-legacy',?1)",[now()]).unwrap();
        c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,bill,made,fulfilled) VALUES('finished',77,30,29900,'bill-finished',?1,?1)",[now()]).unwrap();
        schema(&c);
        schema(&c);
        assert!(tracked(&c, "legacy"));
        assert!(!tracked(&c, "finished"));
        assert_eq!(balance(&c, 622).unwrap(), 0); // A snapshot alone never books income.
        settle(&c, "legacy", Some(("sbp", 900))).unwrap();
        assert_eq!(balance(&c, 622).unwrap(), 5442);
    }
    #[test]
    fn missing_fee_and_legacy_orders() {
        let c = fixture();
        reserve_order(&c, "m", "");
        assert_eq!(settle(&c, "m", None), Err("payment-fee-pending"));
        assert_eq!(balance(&c, 0).unwrap(), 0);
        assert!(settle(&c, "legacy", None).is_ok());
        reverse(&c, "m").unwrap();
        settle(&c, "m", Some(("sbp", 900))).unwrap();
        assert_eq!(balance(&c, 0).unwrap(), 0);
    }
}
