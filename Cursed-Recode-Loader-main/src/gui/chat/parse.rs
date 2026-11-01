//! How the client's chat reads a server (from Cursed 1.21.11's interface/chat/parse.rs): the lines of what came together, each rule taking as many as it needs — whose line
//! (the channel, the clan, the rank's badge or word, a bought prefix, the name, a title, the words), which section,
//! which event, a request with its server's buttons, a private message, the server clearing the chat; what no rule
//! knows stays as the server sent it. ReallyWorld's and FunTime's formats first (their own plugins: RwChat, Essentials,
//! RwCheck, AnDuels, AnRWEvents, BrightWorld's events; FunTime as recorded 27.09.2026), the rest by their shape.

#![allow(dead_code)]

use self::rich::Span;

/// A server's text as the chat reads it (from Cursed 1.21.11's rich.rs): runs of one look, each with its colour, its
/// flags and the click it carries; `run` is the message's own run it came from (its glyphs' width, its hover, Java's
/// style for a click).
pub mod rich {
    pub const BOLD: u32 = 1;
    pub const OWN_FONT: u32 = 32;

    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Span {
        pub text: String,
        /// 0xRRGGBB, or none given (the text's own colour).
        pub colour: Option<u32>,
        pub flags: u32,
        /// Which of the message's runs can be clicked (its index), or -1.
        pub click: i32,
        pub run: i32,
    }

    impl Span {
        pub fn plain(text: &str, colour: u32) -> Span {
            Span { text: text.into(), colour: Some(colour), flags: 0, click: -1, run: -1 }
        }

        pub fn bold(&self) -> bool {
            self.flags & BOLD != 0
        }
    }

    pub fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// A text split into lines at its line breaks (a server sends several lines in one message), each line its spans.
    pub fn lines(spans: &[Span]) -> Vec<Vec<Span>> {
        let mut out = vec![Vec::new()];
        for s in spans {
            let mut parts = s.text.split('\n');
            if let Some(first) = parts.next() {
                if !first.is_empty() {
                    out.last_mut().expect("one at least").push(Span { text: first.into(), ..s.clone() });
                }
            }
            for part in parts {
                out.push(Vec::new());
                if !part.is_empty() {
                    out.last_mut().expect("just pushed").push(Span { text: part.into(), ..s.clone() });
                }
            }
        }
        out
    }

    /// The spans after the first `n` characters of their plain text.
    pub fn from(spans: &[Span], n: usize) -> Vec<Span> {
        let mut out = Vec::new();
        let mut at = 0;
        for s in spans {
            let len = s.text.chars().count();
            let end = at + len;
            if end > n {
                let skip = n.saturating_sub(at);
                out.push(Span { text: s.text.chars().skip(skip).collect(), ..s.clone() });
            }
            at = end;
        }
        out
    }

    /// The colour of the first run with words in it (none: the text's own).
    pub fn first_colour(spans: &[Span]) -> Option<u32> {
        spans.iter().find(|s| !s.text.trim().is_empty()).and_then(|s| s.colour)
    }

    /// Whether a server's pack draws this character (the ranges packs put their pictures in).
    pub fn packs(ch: char) -> bool {
        let c = ch as u32;
        (0xA000..=0xA63F).contains(&c) || (0xE000..=0xF8FF).contains(&c) || c >= 0xF0000
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Server {
    Rw,
    Ft,
    Other,
}

impl Server {
    pub fn of(address: &str) -> Server {
        let a = address.to_ascii_lowercase();
        if a.contains("reallyworld") {
            Server::Rw
        } else if a.contains("funtime") {
            Server::Ft
        } else {
            Server::Other
        }
    }
}

/// One line of what came: the message it is of (its clicks are that message's), its runs and its words, trimmed.
#[derive(Clone, Debug)]
pub struct Line {
    pub id: i32,
    pub spans: Vec<Span>,
    pub p: String,
}

impl Line {
    pub fn new(id: i32, spans: Vec<Span>) -> Line {
        let p = rich::plain(&spans).trim().to_string();
        Line { id, spans, p }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chan {
    Global,
    Local,
    None,
}

#[derive(Clone, Debug, Default)]
pub struct ChatLine {
    pub player: String,
    pub ch: Option<Chan>,
    pub clan: Option<(String, Option<u32>)>,
    /// The pack's glyph of the rank (ReallyWorld).
    pub rank_glyph: Option<char>,
    /// The rank's word and colour (FunTime), and whether it was dimmed (⦑⦒).
    pub rank_word: Option<(String, Option<u32>, bool)>,
    /// A bought prefix: its word, colour, bold.
    pub prefix: Option<(String, Option<u32>, bool)>,
    pub plus: bool,
    pub title: Option<(String, Option<u32>)>,
    /// A donation's colour of the words.
    pub tint: Option<u32>,
    /// A paid announcement or a sale's ad: its label.
    pub bc: Option<String>,
    pub text: Vec<Span>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Info,
    Ok,
    Bad,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ev {
    Airdrop,
    Talisman,
    Mine,
    Merchant,
    Siege,
    Beacon,
}

impl Ev {
    pub fn name(self) -> &'static str {
        match self {
            Ev::Airdrop => "Аирдроп",
            Ev::Talisman => "Талисман",
            Ev::Mine => "Шахта дворфов",
            Ev::Merchant => "Тайный торговец",
            Ev::Siege => "Защита особняка",
            Ev::Beacon => "Загадочный маяк",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Ev::Airdrop => "chat-drop",
            Ev::Talisman => "chat-gem",
            Ev::Mine => "chat-pick",
            Ev::Merchant => "chat-coin",
            Ev::Siege => "chat-shield",
            Ev::Beacon => "chat-bolt",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvState {
    Appear,
    Summoned,
    Taken,
    Wave,
    Clear,
    Opened,
}

#[derive(Clone, Debug)]
pub enum Prize {
    Rank(String),
    Money(u64),
    Text(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rq {
    Tpa,
    TpaHere,
    Duel,
    Trade,
    Clan,
}

/// A button of a request: the server's own click where it has one (its message, which click), else a command.
#[derive(Clone, Debug)]
pub enum Cmd {
    Click(i32, i32),
    Send(String),
}

#[derive(Clone, Debug)]
pub enum Kind {
    Chat(ChatLine),
    Whisper { from: String, text: Vec<Span>, mine: bool },
    Section { name: String, words: Vec<Span>, event: bool },
    Status { tone: Tone, label: Option<String>, words: Vec<Span> },
    Airdrop { title: String, colour: Option<u32>, loot: Vec<(String, u32)>, more: u32, x: i32, z: i32 },
    Event { ev: Ev, state: EvState, pos: Option<(i32, Option<i32>, i32)>, player: String, clan: String, wave: u32, stats: Vec<(String, u32)>, enemy: String, cmd: String, title: String, text: String },
    Win { player: String, prize: Prize, from: String },
    Boss { name: String, text: String },
    Tp { secs: u32 },
    TpDone,
    Kill { victim: String, text: String },
    Remind { text: String, cmds: Vec<String> },
    Check { via: String, link: Option<String>, ask: Option<String>, secs: u32, rules: Vec<(String, String)>, mute: bool },
    CheckMsg { text: String },
    CheckSent,
    CheckEnd { text: String },
    Request { rq: Rq, from: String, secs: u32, yes: Cmd, no: Option<Cmd>, kit: String, stake: String, clan: String },
    Restart { secs: u32 },
    RestartStop,
    Promo { text: String, link: Option<String> },
    Greeting { title: String, cmds: Vec<String>, links: Vec<String> },
    Cleared { n: usize },
    Skip,
    Raw,
}

pub struct Parsed {
    pub kind: Kind,
    /// Its lines, as they came (each with its message).
    pub lines: Vec<(i32, Vec<Span>)>,
}

// ------------------------------------------------------------------------------------------------ small readers

fn is_name(s: &str) -> bool {
    let n = s.chars().count();
    (2..=16).contains(&n) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The word after a prefix up to a space.
fn word_after<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(prefix)?;
    Some(rest.split_whitespace().next().unwrap_or(""))
}

fn digits_in(s: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<i64>| {
        if cur.chars().any(|c| c.is_ascii_digit()) {
            if let Ok(v) = cur.parse() {
                out.push(v);
            }
        }
        cur.clear();
    };
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_digit() || (c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) && cur.is_empty()) {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// The click of the first run whose words contain one of these (a request's server buttons): its message and click.
fn click_of(lines: &[Line], needles: &[&str]) -> Option<(i32, i32)> {
    lines.iter().flat_map(|l| l.spans.iter().map(move |s| (l.id, s))).find(|(_, s)| s.click >= 0 && needles.iter().any(|n| s.text.contains(n))).map(|(id, s)| (id, s.click))
}

pub fn section_tone(words: &[Span]) -> Tone {
    match rich::first_colour(words) {
        Some(0xFF5555) => Tone::Bad,
        Some(0x55FF55) => Tone::Ok,
        _ => Tone::Info,
    }
}

// ------------------------------------------------------------------------------------------------ the rules

type Rule = fn(&[Line], usize, &str) -> Option<(usize, Kind)>;

fn cleared(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let n = l[i..].iter().take_while(|x| x.p.is_empty()).count();
    (n >= 10).then_some((n, Kind::Cleared { n }))
}

fn skip(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    l[i].p.is_empty().then_some((1, Kind::Skip))
}

/// Essentials: [who -> whom] words; "я" is oneself.
fn private(l: &[Line], i: usize, me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let rest = p.strip_prefix('[')?;
    let (head, text) = rest.split_once("] ")?;
    let (from, to) = head.split_once(" -> ")?;
    if from.contains(' ') || to.contains(' ') {
        return None;
    }
    let is_me = |w: &str| w.eq_ignore_ascii_case("я") || w.eq_ignore_ascii_case("me") || w == me;
    let at = p.chars().count() - text.chars().count();
    let words = rich::from(&l[i].spans, at);
    Some(if is_me(from) {
        (1, Kind::Whisper { from: to.to_string(), text: words, mine: true })
    } else {
        (1, Kind::Whisper { from: from.to_string(), text: words, mine: false })
    })
}

/// The game's own chat (a server without a chat plugin of its own, a world of one's own): <name> words.
fn vanilla_chat(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (name, text) = p.strip_prefix('<')?.split_once("> ")?;
    if !is_name(name) {
        return None;
    }
    let at = p.chars().count() - text.chars().count();
    Some((1, Kind::Chat(ChatLine { player: name.to_string(), text: rich::from(&l[i].spans, at), ..ChatLine::default() })))
}

/// The game's own private messages (/msg), in Russian and in English: "name шепчет вам: …", "Вы шепчете name: …".
fn vanilla_whisper(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (from, text, mine) = if let Some(rest) = p.strip_prefix("Вы шепчете ").or_else(|| p.strip_prefix("You whisper to ")) {
        let (to, text) = rest.split_once(": ")?;
        (to, text, true)
    } else {
        let (from, text) = p.split_once(" шепчет вам: ").or_else(|| p.split_once(" whispers to you: "))?;
        (from, text, false)
    };
    if !is_name(from) {
        return None;
    }
    let at = p.chars().count() - text.chars().count();
    Some((1, Kind::Whisper { from: from.to_string(), text: rich::from(&l[i].spans, at), mine }))
}

/// RwCheck (and its kin): called to a check — all its lines, up to the plugin's next, one card.
fn check(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let via = l[i].p.strip_prefix("ВНИМАНИЕ: Вы были вызваны на проверку через ")?.trim().to_string();
    let mut j = i + 1;
    while j < l.len() && !l[j].p.starts_with("Check »") && !l[j].p.starts_with("ВНИМАНИЕ:") {
        j += 1;
    }
    let all: Vec<&str> = l[i..j].iter().map(|x| x.p.as_str()).collect();
    let joined = all.join("\n");
    let link = joined.split("Инструкция: ").nth(1).and_then(|r| r.split_whitespace().next()).map(str::to_string);
    let ask = joined.split("Пишите свой ").nth(1).and_then(|r| r.split(" в чат").next()).map(str::to_string);
    let mins = joined.find("у вас есть ").and_then(|k| digits_in(&joined[k..]).first().copied()).unwrap_or(5) as u32;
    let rules = all
        .iter()
        .filter_map(|t| {
            let (a, b) = t.split_once(" - ")?;
            match a {
                "Признание в использовании читов" => Some(("Признание".to_string(), b.to_string())),
                "За выход с сервера во время проверки" => Some(("Выход с сервера".to_string(), b.to_string())),
                _ => None,
            }
        })
        .collect();
    Some((j - i, Kind::Check { via, link, ask, secs: mins * 60, rules, mute: joined.contains("/mod") }))
}

fn check_talk(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    if let Some(text) = p.strip_prefix("Check » Проверяющий: ") {
        return Some((1, Kind::CheckMsg { text: text.to_string() }));
    }
    if p.trim_end_matches('.') == "Сообщение отправлено модератору" {
        return Some((1, Kind::CheckSent));
    }
    if p == "Check » Проверка окончена, вы свободны" || p == "Check » Проверяющий вышел с сервера, проверка окончена" {
        return Some((1, Kind::CheckEnd { text: p.trim_start_matches("Check » ").to_string() }));
    }
    None
}

/// Essentials: someone asks to come to you (or you to them); how long it waits; the hints after it.
fn tpa(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (from, rest) = p.split_once(' ')?;
    let here = if rest.starts_with("просит телепортироваться к вам") {
        false
    } else if rest.starts_with("просит вас телепортироваться к нему") {
        true
    } else {
        return None;
    };
    let mut j = i + 1;
    let mut secs = 120;
    while j < l.len() {
        if l[j].p.starts_with("Запрос будет автоматически отменён через") {
            secs = digits_in(&l[j].p).first().copied().unwrap_or(120) as u32;
        } else if !l[j].p.starts_with("Для принятия запроса") && !l[j].p.starts_with("Для отклонения запроса") {
            break;
        }
        j += 1;
    }
    let yes = click_of(&l[i..j], &["✔"]).map_or(Cmd::Send("/tpaccept".into()), |(id, c)| Cmd::Click(id, c));
    let no = click_of(&l[i..j], &["✘"]).map_or(Cmd::Send("/tpdeny".into()), |(id, c)| Cmd::Click(id, c));
    Some((j - i, Kind::Request { rq: if here { Rq::TpaHere } else { Rq::Tpa }, from: from.to_string(), secs, yes, no: Some(no), kit: String::new(), stake: String::new(), clan: String::new() }))
}

fn trade(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Игрок ")?;
    let (from, tail) = rest.split_once(' ')?;
    if !tail.starts_with("хочет провести с Вами сделку") {
        return None;
    }
    let yes = click_of(&l[i..=i], &["Принять"]).map_or(Cmd::Send(format!("/trade accept {from}")), |(id, c)| Cmd::Click(id, c));
    let no = click_of(&l[i..=i], &["Отклонить"]).map_or(Cmd::Send(format!("/trade deny {from}")), |(id, c)| Cmd::Click(id, c));
    Some((1, Kind::Request { rq: Rq::Trade, from: from.to_string(), secs: 0, yes, no: Some(no), kit: String::new(), stake: String::new(), clan: String::new() }))
}

/// RW-Restart: its count, said again and again, and its stop.
fn restart(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Авто Рестарт » ")?;
    if rest.starts_with("Перезагрузка сервера была остановлена") {
        return Some((1, Kind::RestartStop));
    }
    if rest.starts_with("Перезагрузка сервера через") || rest.contains("секунд до перезагрузки") {
        let secs = digits_in(rest).first().copied()? as u32;
        return Some((1, Kind::Restart { secs }));
    }
    None
}

const COMMON: [Rule; 10] = [cleared, skip, private, check, check_talk, tpa, trade, restart, vanilla_chat, vanilla_whisper];

// ------------------------------------------------------------------------------------------------ ReallyWorld

const RW_EVENT_SECTIONS: [&str; 8] = ["АирДроп", "Талисман", "Шахта Дворфов", "Состязание", "Битва с Боссом", "Башни", "Башни Энда", "Кланы"];

fn bar_start(p: &str) -> bool {
    let t = p.trim_start();
    t.starts_with('\u{A401}') || t.starts_with('\u{A400}')
}

/// A case's broadcast: lines that start with the gold bar; one says who won what.
fn rw_win(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    if !bar_start(&l[i].p) {
        return None;
    }
    let j = i + l[i..].iter().take_while(|x| bar_start(&x.p)).count();
    let lines = &l[i..j];
    let won = lines.iter().find(|x| x.p.contains("Игрок ") && x.p.contains(" выиграл"))?;
    let who = won.p.split("Игрок ").nth(1)?.split_whitespace().next()?.to_string();
    let at = won.p.find("выиграл ").map(|b| won.p[..b].chars().count() + 8)?;
    let prize_text = rich::plain(&rich::from(&won.spans, at)).trim().to_string();
    let prize = if prize_text.contains('\u{A402}') {
        Prize::Money(prize_text.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0))
    } else if prize_text.chars().all(|c| c.is_alphabetic() || c == '.' || c == '+') && prize_text.len() <= 14 {
        Prize::Rank(prize_text.clone())
    } else {
        Prize::Text(prize_text.clone())
    };
    let from = lines.iter().find_map(|x| x.p.split("Открыв ").nth(1)).unwrap_or("").trim().to_string();
    Some((j - i, Kind::Win { player: who, prize, from }))
}

/// An airdrop: from "Появился" to the clan's reward (ReallyWorld's own: its loot a line each "Name x3", the place as
/// [x; z]; AnRWEvents' [x: N z: N]).
fn rw_airdrop(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let title = l[i].p.strip_prefix("АирДроп » Появился ")?.trim_end_matches('!').trim().to_string();
    let mut j = i + 1;
    while j < l.len() && j - i < 12 && !l[j - 1].p.trim_end().ends_with("Опыт") && !l[j].p.starts_with("АирДроп » Появился ") && l[j].p.starts_with("АирДроп »") {
        j += 1;
    }
    let lines = &l[i..j];
    let at = l[i].p.find("Появился ").map(|b| l[i].p[..b].chars().count() + 9).unwrap_or(0);
    let colour = rich::first_colour(&rich::from(&l[i].spans, at));
    let from = lines.iter().position(|x| x.p.contains("Награда за открытие"));
    let till = lines.iter().position(|x| x.p.contains("Координаты"));
    let mut loot = Vec::new();
    let mut more = 0;
    if let Some(f) = from {
        for x in &lines[f + 1..till.unwrap_or(lines.len())] {
            let t = x.p.trim_start_matches("АирДроп » ").trim();
            if let Some(rest) = t.strip_prefix("И еще ") {
                more = digits_in(rest).first().copied().unwrap_or(0) as u32;
                continue;
            }
            for part in t.split(',') {
                let part = part.trim();
                // "Мембрана фантомаx5" (no space) or "Тотем бессмертия x2".
                let (name, n) = match part.rfind('x') {
                    Some(k) if part[k + 1..].chars().all(|c| c.is_ascii_digit()) && k + 1 < part.len() => (part[..k].trim(), part[k + 1..].parse().unwrap_or(1)),
                    _ => (part, 1),
                };
                if !name.is_empty() {
                    loot.push((name.to_string(), n));
                }
            }
        }
    }
    let pos = lines.iter().find(|x| x.p.contains("Координаты: ")).map(|x| digits_in(x.p.split("Координаты: ").nth(1).unwrap_or(""))).filter(|d| d.len() >= 2);
    let (x, z) = pos.map_or((0, 0), |d| (d[0] as i32, d[d.len() - 1] as i32));
    Some((j - i, Kind::Airdrop { title, colour, loot, more, x, z }))
}

fn event(ev: Ev, state: EvState) -> Kind {
    Kind::Event { ev, state, pos: None, player: String::new(), clan: String::new(), wave: 0, stats: Vec::new(), enemy: String::new(), cmd: String::new(), title: String::new(), text: String::new() }
}

/// The talisman: found (its place, as recorded 27.09 "Координаты талисмана: x z"), or taken with the clan's numbers.
fn rw_talisman(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    if l[i].p == "Талисман » Появился талисман!" {
        let mut j = i + 1;
        while j < l.len() && l[j].p.starts_with("Талисман »") && !l[j].p.contains("Появился") && !l[j].p.contains("захватил") {
            j += 1;
        }
        let d = l[i..j].iter().find(|x| x.p.contains("Координаты талисмана")).map(|x| digits_in(&x.p)).unwrap_or_default();
        let mut k = event(Ev::Talisman, EvState::Appear);
        if let Kind::Event { pos, text, .. } = &mut k {
            if d.len() >= 2 {
                *pos = Some((d[0] as i32, None, d[d.len() - 1] as i32));
            }
            *text = "Очки за захват: стоять на талисмане, убивать чужие кланы. Топ-3 кланов получат коины и опыт".into();
        }
        return Some((j - i, k));
    }
    let clan = l[i].p.strip_prefix("Талисман » Клан ")?.split(" захватил").next()?.to_string();
    let mut j = i + 1;
    let mut stats = Vec::new();
    while j < l.len() && l[j].p.starts_with("Талисман »") && !l[j].p.contains("Появился") && !l[j].p.contains("захватил") {
        for key in ["Захват территории", "Убийств", "Смертей"] {
            if let Some(v) = l[j].p.split(&format!("{key}: ")).nth(1) {
                stats.push((key.to_string(), digits_in(v).first().copied().unwrap_or(0) as u32));
            }
        }
        j += 1;
    }
    let mut k = event(Ev::Talisman, EvState::Taken);
    if let Kind::Event { clan: c, stats: s, .. } = &mut k {
        *c = clan;
        *s = stats;
    }
    Some((j - i, k))
}

fn rw_mine(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    if !p.starts_with("Шахта Дворфов »") {
        return None;
    }
    if p.contains("» В ") && p.contains("нашли") {
        let mut j = i + 1;
        while j < l.len() && l[j].p.starts_with("Шахта Дворфов »") && !l[j].p.contains("Волна") && !l[j].p.contains("пройдены") {
            j += 1;
        }
        let d = l[i..j].iter().find(|x| x.p.contains("заспавнилась тут")).map(|x| digits_in(&x.p)).unwrap_or_default();
        let mut k = event(Ev::Mine, EvState::Appear);
        if let Kind::Event { pos, text, .. } = &mut k {
            if d.len() >= 2 {
                *pos = Some((d[d.len() - 2] as i32, None, d[d.len() - 1] as i32));
            }
            // What to do there, in the server's own words (the lines from "Одолейте" on, before the rewards' line).
            let words: Vec<&str> = l[i..j]
                .iter()
                .map(|x| x.p.split_once("» ").map_or("", |(_, w)| w))
                .skip_while(|w| !w.starts_with("Одолейте"))
                .take_while(|w| !w.starts_with("Для ") && !w.contains("заспавнилась"))
                .collect();
            *text = words.join(" ");
        }
        return Some((j - i, k));
    }
    if p.contains("Волна") && p.contains("началась") {
        let mut k = event(Ev::Mine, EvState::Wave);
        if let Kind::Event { wave, .. } = &mut k {
            *wave = digits_in(p).first().copied().unwrap_or(0) as u32;
        }
        return Some((1, k));
    }
    if !p.contains("Все волны пройдены") {
        return None;
    }
    // What now, as the server says it ("можно добывать ...").
    let mut k = event(Ev::Mine, EvState::Clear);
    if let Kind::Event { text, .. } = &mut k {
        *text = p.split_once("пройдены").map_or("", |(_, w)| w).trim_start_matches([',', ' ', '—', '-']).trim().to_string();
    }
    Some((1, k))
}

/// The contest's end (BrightEvents, recorded 27.09 19:50): the winner, the prize (money), the congratulations.
fn rw_contest(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Состязание » Игрок ")?;
    let (who, tail) = rest.split_once(' ')?;
    if !tail.starts_with("победил в состязании") {
        return None;
    }
    let mut j = i + 1;
    let mut prize = Prize::Text(String::new());
    while j < l.len() && l[j].p.starts_with("Состязание »") {
        if let Some(v) = l[j].p.split("приз в размере ").nth(1) {
            prize = if v.contains("рубл") { Prize::Text(format!("{} ₽", digits_in(v).first().copied().unwrap_or(0))) } else { Prize::Money(v.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)) };
        }
        j += 1;
    }
    Some((j - i, Kind::Win { player: who.to_string(), prize, from: "Состязание".into() }))
}

fn rw_opened(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let who = word_after(&l[i].p, "АирДроп » Игрок ")?;
    if !l[i].p.ends_with("открыл аирдроп") {
        return None;
    }
    let mut k = event(Ev::Airdrop, EvState::Opened);
    if let Kind::Event { player, .. } = &mut k {
        *player = who.to_string();
    }
    Some((1, k))
}

/// AnDuels: a duel offered, in its frame of dashes (who, the kit, the stake; 30 seconds; the server's buttons).
fn rw_duel(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let dashes = |p: &str| p.len() >= 10 && p.chars().all(|c| c == '-');
    if !dashes(&l[i].p) || !l.get(i + 1).is_some_and(|x| x.p.contains("запрос на дуэль")) {
        return None;
    }
    let mut j = i + 1;
    let (mut from, mut kit, mut stake) = (String::new(), String::new(), String::new());
    while j < l.len() {
        if let Some(rest) = l[j].p.strip_prefix("➝ ") {
            if let Some((k, v)) = rest.split_once(": ") {
                match k {
                    "Ник" => from = v.to_string(),
                    "Набор" => kit = v.to_string(),
                    "Ставка" => stake = v.to_string(),
                    _ => {}
                }
            }
        }
        j += 1;
        if dashes(&l[j - 1].p) && j >= 2 && l[j - 2].p.contains("Нажмите, чтобы") {
            break;
        }
        if j - i > 12 {
            break;
        }
    }
    let yes = click_of(&l[i..j], &["ПРИНЯТЬ", "Принять"]).map_or(Cmd::Send(format!("/duel accept {from}")), |(id, c)| Cmd::Click(id, c));
    let no = click_of(&l[i..j], &["ОТКЛОНИТЬ", "Отклонить"]).map_or(Cmd::Send(format!("/duel deny {from}")), |(id, c)| Cmd::Click(id, c));
    Some((j - i, Kind::Request { rq: Rq::Duel, from, secs: 30, yes, no: Some(no), kit, stake, clan: String::new() }))
}

fn rw_clan_invite(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Кланы » Приглашение клан ")?;
    let (clan, tail) = rest.split_once(" от игрока ")?;
    let from = tail.split(['.', ' ']).next()?.to_string();
    Some((1, Kind::Request { rq: Rq::Clan, from, secs: 0, yes: Cmd::Send("/clan accept".into()), no: None, kit: String::new(), stake: String::new(), clan: clan.to_string() }))
}

fn rw_merchant(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("ТАЙНЫЙ ТОРГОВЕЦ » Появился тайный торговец: ")?;
    let d = digits_in(rest);
    if d.len() < 3 {
        return None;
    }
    let mut k = event(Ev::Merchant, EvState::Appear);
    if let Kind::Event { pos, .. } = &mut k {
        *pos = Some((d[0] as i32, Some(d[1] as i32), d[2] as i32));
    }
    Some((1, k))
}

fn rw_siege(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Защита Особняка » На особняк напала ")?;
    let (enemy, cmd) = rest.split_once("! ")?;
    let mut j = i + 1;
    while j < l.len() && l[j].p.starts_with("Защита Особняка » ") && !l[j].p.contains("напала") {
        j += 1;
    }
    let mut k = event(Ev::Siege, EvState::Appear);
    if let Kind::Event { enemy: e, cmd: c, text, .. } = &mut k {
        *e = enemy.to_string();
        *c = cmd.trim().to_string();
        *text = "Награда каждому — по вкладу в защиту".into();
    }
    Some((j - i, k))
}

/// AutomaticBroadcast: an ad, lines behind the broadcast's bars, a link at the end.
fn rw_promo(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    if !bar_start(&l[i].p) {
        return None;
    }
    let j = i + l[i..].iter().take_while(|x| bar_start(&x.p)).count();
    let all = l[i..j].iter().map(|x| x.p.trim_start_matches(['\u{A400}', '\u{A401}', ' ']).trim().to_string()).collect::<Vec<_>>().join(" ");
    let link = all.split_whitespace().find(|w| w.starts_with("http")).map(str::to_string)?;
    Some((j - i, Kind::Promo { text: all.replace(&link, "").trim().to_string(), link: Some(link) }))
}

fn rw_clan_reward(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("Кланы » Ваш клан получил ")?;
    let d = digits_in(rest);
    if d.len() < 2 {
        return None;
    }
    let why = l.get(i + 1).and_then(|x| x.p.strip_prefix("Кланы » За: ")).map(str::to_string);
    let text = format!("Клану +{} опыта и +{} коинов{}", d[0], d[1], why.as_ref().map_or(String::new(), |w| format!(" · {}", w.to_lowercase())));
    Some((if why.is_some() { 2 } else { 1 }, Kind::Section { name: "Кланы".into(), words: vec![Span::plain(&text, 0x55FF55)], event: true }))
}

/// A boss speaks: its name in dark red, a colon.
fn rw_boss(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    if rich::first_colour(&l[i].spans) != Some(0xAA0000) {
        return None;
    }
    let (name, text) = l[i].p.split_once(": ")?;
    let n = name.chars().count();
    ((3..=32).contains(&n)).then(|| (1, Kind::Boss { name: name.to_string(), text: text.to_string() }))
}

/// The greeting: its links in lines of » (one card).
fn rw_greeting(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let title = l[i].p.strip_prefix("Добро пожаловать на сервер ")?.trim_end_matches('!').to_string();
    let mut j = i + 1;
    let (mut cmds, mut links) = (Vec::new(), Vec::new());
    while j < l.len() && (l[j].p.is_empty() || l[j].p.starts_with("» ")) {
        if let Some(last) = l[j].p.split_whitespace().last() {
            if l[j].p.starts_with("» ") {
                if last.starts_with('/') {
                    cmds.push(last.to_string());
                } else {
                    links.push(last.to_string());
                }
            }
        }
        j += 1;
    }
    Some((j - i, Kind::Greeting { title, cmds, links }))
}

/// A donator's announcement (RwChat /bc).
fn rw_bc(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("[ОБЪЯВЛЕНИЕ] Говорит ")?;
    let (who, text) = rest.split_once(": ")?;
    let at = l[i].p.chars().count() - text.chars().count();
    Some((1, Kind::Chat(ChatLine { player: who.to_string(), ch: Some(Chan::Global), bc: Some("Объявление".into()), text: rich::from(&l[i].spans, at), ..ChatLine::default() })))
}

/// A player's line (RwChat): ▣ (gold: everyone; teal: near), [clan], the rank's glyph or a bought [prefix], the name,
/// + (RwPlus), », the words (a donation may colour them).
fn rw_chat(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let rest = p.strip_prefix("▣ ")?;
    let (head, text) = rest.split_once(" » ")?;
    let mut words: Vec<&str> = head.split_whitespace().collect();
    let mut c = ChatLine { ch: Some(if rich::first_colour(&l[i].spans) == Some(0x00AAAA) { Chan::Local } else { Chan::Global }), ..ChatLine::default() };
    let mut name = words.pop()?.to_string();
    if let Some(n) = name.strip_suffix('+') {
        c.plus = true;
        name = n.to_string();
    }
    if !is_name(&name) {
        return None;
    }
    c.player = name;
    let spans = &l[i].spans;
    let colour_at = |needle: &str| -> Option<u32> {
        let at = p.find(needle).map(|b| p[..b].chars().count())?;
        rich::first_colour(&rich::from(spans, at + 1))
    };
    for w in words {
        let bracketed = w.starts_with('[') && w.ends_with(']') && w.len() > 2;
        if w.chars().count() == 1 && w.chars().all(rich::packs) {
            c.rank_glyph = w.chars().next();
        } else if bracketed && c.clan.is_none() && c.rank_glyph.is_none() && c.prefix.is_none() {
            // The first [..] is the clan when a rank or a prefix follows; a lone [..] is a prefix.
            let inner = &w[1..w.len() - 1];
            c.clan = Some((inner.to_string(), colour_at(w)));
        } else if bracketed {
            let inner = &w[1..w.len() - 1];
            let bold = spans.iter().any(|s| s.text.contains(inner) && s.bold());
            c.prefix = Some((inner.to_string(), colour_at(w), bold));
        }
    }
    // A lone [..] with no rank after it: a bought prefix in the rank's place, not a clan.
    if c.rank_glyph.is_none() && c.prefix.is_none() {
        if let Some((w, col)) = c.clan.take() {
            let bold = spans.iter().any(|s| s.text.contains(&w) && s.bold());
            c.prefix = Some((w, col, bold));
        }
    }
    let at = p.find(" » ").map(|b| p[..b].chars().count() + 3)?;
    c.text = rich::from(spans, at);
    let body = if c.ch == Some(Chan::Local) { 0xAAAAAA } else { 0xFFFFFF };
    c.tint = rich::first_colour(&c.text).filter(|&col| col != body);
    let _ = text;
    Some((1, Kind::Chat(c)))
}

/// A section's line: its name, », the words.
fn rw_section(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (name, text) = p.split_once(" » ")?;
    let n = name.chars().count();
    if !(2..=23).contains(&n) || !name.chars().next()?.is_uppercase() || text.is_empty() {
        return None;
    }
    let first = rich::first_colour(&l[i].spans);
    if matches!(first, Some(0xFFFFFF) | Some(0xAAAAAA) | None) {
        return None;
    }
    let words = rich::from(&l[i].spans, n + 3);
    Some((1, Kind::Section { name: name.to_string(), event: RW_EVENT_SECTIONS.contains(&name), words }))
}

const RW: [Rule; 17] = [rw_win, rw_airdrop, rw_talisman, rw_mine, rw_contest, rw_opened, rw_duel, rw_clan_invite, rw_merchant, rw_siege, rw_promo, rw_clan_reward, rw_boss, rw_greeting, rw_bc, rw_chat, rw_section];

// ------------------------------------------------------------------------------------------------ FunTime

/// A player's line: Ⓖ (everyone) or Ⓛ (near), <clan>, [rank] (or ⦑rank⦒, dimmed), the name, a title, ⇨, the words —
/// grey in italics for everyone, green near; a donation colours them.
fn ft_chat(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (mark, rest) = if let Some(r) = p.strip_prefix("Ⓖ ") {
        (Chan::Global, r)
    } else if let Some(r) = p.strip_prefix("Ⓛ ") {
        (Chan::Local, r)
    } else {
        return None;
    };
    let (head, _text) = rest.split_once(" ⇨")?;
    let spans = &l[i].spans;
    let colour_of = |needle: &str| -> Option<u32> {
        let at = p.find(needle).map(|b| p[..b].chars().count())?;
        rich::first_colour(&rich::from(spans, at))
    };
    let mut c = ChatLine { ch: Some(mark), ..ChatLine::default() };
    let mut h = head.trim();
    if let Some(r) = h.strip_prefix('<') {
        let (clan, tail) = r.split_once("> ")?;
        c.clan = Some((clan.to_string(), colour_of(clan)));
        h = tail;
    }
    let (open, close, dim) = if h.starts_with('[') { ('[', ']', false) } else if h.starts_with('⦑') { ('⦑', '⦒', true) } else { return None };
    let inner = h.strip_prefix(open)?;
    let (rank, tail) = inner.split_once(close)?;
    c.rank_word = Some((rank.to_string(), colour_of(rank), dim));
    let mut parts = tail.trim().splitn(2, ' ');
    let name = parts.next()?;
    if !is_name(name) {
        return None;
    }
    c.player = name.to_string();
    if let Some(title) = parts.next().map(str::trim).filter(|t| !t.is_empty()) {
        c.title = Some((title.to_string(), colour_of(title)));
    }
    let at = p.find(" ⇨").map(|b| p[..b].chars().count() + 2)?;
    let words = rich::from(spans, at);
    let first_words = words.iter().position(|s| !s.text.trim().is_empty()).unwrap_or(0);
    c.text = words[first_words..].to_vec();
    let body = if mark == Chan::Local { 0x55FF55 } else { 0xD8D8D8 };
    c.tint = rich::first_colour(&c.text).filter(|&col| col != body);
    Some((1, Kind::Chat(c)))
}

fn ft_bc(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let rest = p.strip_prefix("⦏Объявление⦐ ")?;
    let (text, who) = rest.rsplit_once(" [Автор: ")?;
    let who = who.trim_end_matches(']');
    let at = p.chars().count() - rest.chars().count();
    let n = text.chars().count();
    let words: Vec<Span> = rich::from(&l[i].spans, at).into_iter().scan(0usize, |seen, s| {
        let len = s.text.chars().count();
        let keep = n.saturating_sub(*seen);
        *seen += len;
        Some(if keep == 0 { None } else { Some(Span { text: s.text.chars().take(keep).collect(), ..s }) })
    }).flatten().collect();
    Some((1, Kind::Chat(ChatLine { player: who.to_string(), ch: Some(Chan::Global), bc: Some("Объявление".into()), text: words, ..ChatLine::default() })))
}

fn ft_sale(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let rest = p.strip_prefix("[$] Продажа ⇨ ")?;
    let (text, who) = rest.rsplit_once(" (")?;
    let who = who.trim_end_matches(')');
    if !is_name(who) {
        return None;
    }
    Some((1, Kind::Chat(ChatLine { player: who.to_string(), ch: Some(Chan::Global), bc: Some("Продажа".into()), text: vec![Span::plain(text.trim(), 0xBFC0E7)], ..ChatLine::default() })))
}

/// A summoned event (the shop sells them): a box «its name», who called it, where.
fn ft_summoned(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    if !p.starts_with('╔') || !p.contains('«') || i + 3 >= l.len() + 1 {
        return None;
    }
    let title = p.split('«').nth(1)?.split('»').next()?.trim().to_string();
    let who = l.get(i + 1)?.p.strip_prefix("║ Призван игроком ")?.trim().to_string();
    let d = digits_in(l.get(i + 2)?.p.strip_prefix("║ на координатах")?);
    if d.len() < 3 || !l.get(i + 3)?.p.starts_with('╚') {
        return None;
    }
    let ev = if title.to_lowercase().contains("аирдроп") { Ev::Airdrop } else { Ev::Beacon };
    let mut k = event(ev, EvState::Summoned);
    if let Kind::Event { pos, player, title: t, .. } = &mut k {
        *pos = Some((d[0] as i32, Some(d[1] as i32), d[2] as i32));
        *player = who;
        *t = title;
    }
    Some((4, k))
}

/// The server's word in a half box: ╔, ╠ the words, ╚.
fn ft_half(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    if l[i].p != "╔" || !l.get(i + 1)?.p.starts_with("╠ ") {
        return None;
    }
    let mut j = i + 1;
    while j < l.len() && l[j].p.starts_with("╠ ") {
        j += 1;
    }
    if l.get(j)?.p != "╚" {
        return None;
    }
    let all = l[i + 1..j].iter().map(|x| x.p.trim_start_matches("╠ ").to_string()).collect::<Vec<_>>().join(" ");
    let link = all.split_whitespace().find(|w| w.starts_with('/') || w.contains(".com/") || w.contains(".ru/") || w.contains(".su/")).map(str::to_string);
    let text = link.as_ref().map_or(all.clone(), |k| all.replace(k, "")).trim().trim_end_matches('-').trim().to_string();
    Some((j - i + 1, Kind::Promo { text, link }))
}

/// A box of ╔ ║ ╚: a case's prize, a key, the greeting.
fn ft_box(l: &[Line], i: usize, me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    if !p.starts_with('╔') && !p.starts_with("★ FunTime") {
        return None;
    }
    let mut j = i + 1;
    while j < l.len() && j - i < 9 && !l[j - 1].p.starts_with('╚') && (l[j].p.starts_with('║') || l[j].p.starts_with('╚') || p.starts_with('★')) {
        j += 1;
    }
    let inner: Vec<String> = l[i..j]
        .iter()
        .map(|x| x.p.trim_matches(|c: char| "║╔╚═╦╩╗╝ ".contains(c)).trim().to_string())
        .filter(|t| !t.is_empty() && !t.chars().all(|c| "═╦╩".contains(c)))
        .collect();
    if inner.iter().any(|t| t.contains("[Кейс]")) {
        let won = inner.iter().find(|t| t.starts_with("Выигрыш: ")).cloned().unwrap_or_default();
        let from = inner.iter().find(|t| !t.contains("[Кейс]") && !t.starts_with("Выигрыш")).cloned().unwrap_or_default();
        return Some((j - i, Kind::Win { player: me.to_string(), prize: Prize::Text(won.trim_start_matches("Выигрыш: ").to_string()), from }));
    }
    if inner.iter().any(|t| t.contains("ключ к кейсу")) {
        return Some((j - i, Kind::Greeting { title: "Ключ к кейсу".into(), cmds: vec!["/warp case".into()], links: Vec::new() }));
    }
    let title = inner.first()?.replace('★', "").trim().to_string();
    let cmds: Vec<String> = inner.iter().flat_map(|t| t.split_whitespace().filter(|w| w.starts_with('/')).map(str::to_string).collect::<Vec<_>>()).collect();
    Some((j - i, Kind::Greeting { title, cmds, links: Vec::new() }))
}

fn ft_tp(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    if let Some(rest) = p.strip_prefix("[⚝] Телепортация! Перемещение через ") {
        let secs = digits_in(rest).first().copied().unwrap_or(5) as u32;
        let mut j = i + 1;
        while j < l.len() && (l[j].p.starts_with("[⚝] Внимание") || l[j].p.contains("[Отменить телепорт]")) {
            j += 1;
        }
        return Some((j - i, Kind::Tp { secs }));
    }
    p.starts_with("⚡ Телепортирование").then_some((1, Kind::TpDone))
}

fn ft_broadcast(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("⚡ ")?;
    let link = rest.split_whitespace().last().filter(|w| w.starts_with('/') || w.contains(".com") || w.contains(".ru") || w.contains(".su") || w.contains(".me")).map(str::to_string);
    let text = link.as_ref().map_or(rest.to_string(), |k| rest.trim_end_matches(k.as_str()).trim().to_string());
    Some((1, Kind::Promo { text, link }))
}

fn ft_afk(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let (tone, text) = match p.as_str() {
        "Вы отошли." => (Tone::Info, "вы отошли"),
        "Вы вернулись." => (Tone::Info, "вы вернулись"),
        _ if p.ends_with("недоступна в режиме AFK") => (Tone::Bad, "команды недоступны, пока вы отошли"),
        _ => return None,
    };
    Some((1, Kind::Status { tone, label: Some("AFK".into()), words: vec![Span::plain(text, 0xBFC0E7)] }))
}

fn ft_status(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let p = &l[i].p;
    let rest = p.strip_prefix('[')?;
    let (mark, _text) = rest.split_once("] ")?;
    let tone = match mark {
        "✔" => Tone::Ok,
        "✘" => Tone::Bad,
        "⚝" | "✾" => Tone::Info,
        _ => return None,
    };
    let words = rich::from(&l[i].spans, mark.chars().count() + 3);
    Some((1, Kind::Status { tone, label: None, words }))
}

fn ft_botfilter(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("BotFilter >> ")?;
    Some((1, Kind::Status { tone: Tone::Info, label: Some("BotFilter".into()), words: vec![Span::plain(rest, 0xBFC0E7)] }))
}

fn ft_kill(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    let rest = l[i].p.strip_prefix("✞ ")?;
    let (victim, text) = rest.split_once(' ')?;
    Some((1, Kind::Kill { victim: victim.to_string(), text: text.to_string() }))
}

fn ft_remind(l: &[Line], i: usize, _me: &str) -> Option<(usize, Kind)> {
    l[i].p.starts_with("** ! Внимание !").then(|| (1, Kind::Remind { text: "Привяжите аккаунт для безопасности".into(), cmds: vec!["/vk".into(), "/tg".into()] }))
}

const FT: [Rule; 13] = [ft_chat, ft_bc, ft_sale, ft_summoned, ft_half, ft_box, ft_tp, ft_broadcast, ft_afk, ft_status, ft_botfilter, ft_kill, ft_remind];

// ------------------------------------------------------------------------------------------------ all of it

/// What came together (a batch of messages, each maybe several lines): read into messages, each with its lines.
pub fn read(lines: &[Line], server: Server, me: &str) -> Vec<Parsed> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut got = None;
        let sets: Vec<&[Rule]> = match server {
            Server::Rw => vec![&COMMON, &RW],
            Server::Ft => vec![&COMMON, &FT],
            Server::Other => vec![&COMMON, &RW, &FT],
        };
        'rules: for set in sets {
            for rule in set.iter() {
                if let Some(g) = rule(lines, i, me) {
                    got = Some(g);
                    break 'rules;
                }
            }
        }
        let (n, kind) = got.unwrap_or((1, Kind::Raw));
        let n = n.max(1).min(lines.len() - i);
        out.push(Parsed { kind, lines: lines[i..i + n].iter().map(|l| (l.id, l.spans.clone())).collect() });
        i += n;
    }
    out
}
