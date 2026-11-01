//! Binds: any number on a module or on any setting (a switch flips, a choice goes to its next word, a button is
//! pressed — or, when the bind has one, the setting is set to its value: "Quality: Medium" on a key), each a
//! combination of keys (held ones first, the last the one pressed: "Left Ctrl + G") and either
//! pressed (each press switches) or held (on while the keys are down, back when one is let go). Keys and mouse
//! buttons are named as SDL and the menu name them ("G", "Left Ctrl", "Mouse 4").

/// One bind: its keys (the modifiers held, then the key pressed), whether it is held, and the value it sets its
/// setting to (none: it goes on as a bind does; a switch "1" or "0", a choice its option, a slider its number).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Bind {
    pub keys: Vec<String>,
    pub hold: bool,
    pub value: Option<String>,
}

impl Bind {
    pub fn one(key: &str) -> Bind {
        Bind { keys: vec![key.to_string()], hold: false, value: None }
    }

    /// As the menu shows it: short names joined ("Ctrl + G").
    pub fn label(&self) -> String {
        self.keys.iter().map(|k| short(k)).collect::<Vec<_>>().join(" + ")
    }

    /// The key that sets it off (the last); the others are held.
    pub fn trigger(&self) -> &str {
        self.keys.last().map_or("", String::as_str)
    }

    /// Whether `pressed`, with `held` down (it among them), sets it off: its trigger pressed with all its other keys
    /// held (a bind of modifiers only goes off on any of them, once all are down).
    pub fn fires(&self, pressed: &str, held: &[String]) -> bool {
        if self.keys.is_empty() || !self.keys.iter().all(|k| held.iter().any(|h| h == k)) {
            return false;
        }
        if self.keys.iter().all(|k| modifier(k)) {
            self.keys.iter().any(|k| k == pressed)
        } else {
            self.trigger() == pressed
        }
    }
}

/// A key's name as short as it is still clear ("Left Ctrl" "Ctrl", "Right Shift" "R Shift").
pub fn short(k: &str) -> String {
    match k {
        "Left Ctrl" => "Ctrl".into(),
        "Left Shift" => "Shift".into(),
        "Left Alt" => "Alt".into(),
        "Left GUI" => "Win".into(),
        "Right Ctrl" => "R Ctrl".into(),
        "Right Shift" => "R Shift".into(),
        "Right Alt" => "R Alt".into(),
        "Right GUI" => "R Win".into(),
        "Middle" => "MMB".into(),
        _ => k.to_string(),
    }
}

/// A key held to make a combination (it is not a bind's trigger on its own unless it is all the bind is).
pub fn modifier(k: &str) -> bool {
    matches!(k, "Left Ctrl" | "Right Ctrl" | "Left Shift" | "Right Shift" | "Left Alt" | "Right Alt" | "Left GUI" | "Right GUI")
}

/// Modifiers first in one order, then the trigger: two ways of pressing the same combination are one bind.
pub fn canonical(mut keys: Vec<String>) -> Vec<String> {
    let order = |k: &String| match k.as_str() {
        "Left Ctrl" | "Right Ctrl" => 0,
        "Left Shift" | "Right Shift" => 1,
        "Left Alt" | "Right Alt" => 2,
        "Left GUI" | "Right GUI" => 3,
        _ => 9,
    };
    let trig = keys.iter().rposition(|k| !modifier(k)).map(|i| keys.remove(i));
    keys.sort_by_key(order);
    keys.dedup();
    if let Some(t) = trig {
        keys.push(t);
    }
    keys
}

fn esc(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '%' | '+' | ',' | '^' | '=' | '\n' | '\r' => o.push_str(&format!("%{:02X}", ch as u32)),
            _ => o.push(ch),
        }
    }
    o
}

fn unesc(s: &str) -> String {
    let b = s.as_bytes();
    let mut o = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                o.push(v);
                i += 3;
                continue;
            }
        }
        o.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&o).into_owned()
}

/// Binds as one line of the settings' file: keys joined by '+', binds by ',', a held one marked '^', its value after
/// '='.
pub fn save(list: &[Bind]) -> String {
    list.iter()
        .filter(|b| !b.keys.is_empty())
        .map(|b| {
            format!(
                "{}{}{}",
                b.keys.iter().map(|k| esc(k)).collect::<Vec<_>>().join("+"),
                if b.hold { "^" } else { "" },
                b.value.as_ref().map_or(String::new(), |v| format!("={}", esc(v)))
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn load(s: &str) -> Vec<Bind> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (p, value) = match p.split_once('=') {
                Some((k, v)) => (k, Some(unesc(v))),
                None => (p, None),
            };
            let (body, hold) = match p.strip_suffix('^') {
                Some(b) => (b, true),
                None => (p, false),
            };
            Bind { keys: body.split('+').filter(|k| !k.is_empty()).map(unesc).collect(), hold, value }
        })
        .filter(|b| !b.keys.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let l = vec![
            Bind { keys: vec!["Left Ctrl".into(), "G".into()], hold: false, value: None },
            Bind { keys: vec!["Keypad +".into()], hold: true, value: Some("Medium".into()) },
            Bind { keys: vec!["R".into()], hold: false, value: Some("2.5".into()) },
            Bind::one("Mouse 4"),
        ];
        let s = save(&l);
        assert_eq!(load(&s), l);
    }
    #[test]
    fn canonical_order() {
        assert_eq!(canonical(vec!["G".into(), "Left Shift".into(), "Left Ctrl".into()]), vec!["Left Ctrl".to_string(), "Left Shift".into(), "G".into()]);
        assert_eq!(canonical(vec!["Left Alt".into()]), vec!["Left Alt".to_string()]);
    }
}
