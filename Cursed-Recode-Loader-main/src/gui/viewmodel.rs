//! Hand placement and swings. View Model controls each physical hand's offset, rotation and size.
//! Recode/Custom use editable keyframes; Nurik, Catlavan and Newcode use the ordered source
//! transforms in swingport.rs. Java sends hand, item, progress and render facts, then applies the
//! returned pose/operations through cursed.recode.fx.ViewModel.

use super::{theme, Gui};

/// A moment of a swing: when (0..1), where (x, y, z), turned (about x, y, z, degrees), how big; how it eases to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Key {
    pub t: f32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub size: f32,
    /// 0 straight, 1 smooth, 2 a spring, 3 sharp (fast out)
    pub ease: u8,
}

impl Key {
    const fn new(t: f32, pos: [f32; 3], rot: [f32; 3], size: f32, ease: u8) -> Key {
        Key { t, pos, rot, size, ease }
    }
}

const REST: [f32; 3] = [0.0; 3];

/// The styles, by their names in the menu, as keyframes (Vanilla: the game's own swing, none here).
pub(super) const STYLES: [&str; 8] = ["Vanilla", "Recode", "1.7", "Smooth", "Spin", "Slide", "Stab", "Custom"];

fn preset(style: &str) -> Vec<Key> {
    match style {
        // ours: a short arc, the blade led by the wrist, settling with a breath
        "Recode" => vec![
            Key::new(0.0, REST, REST, 1.0, 1),
            Key::new(0.28, [-0.18, 0.06, -0.12], [-38.0, 18.0, -22.0], 1.0, 3),
            Key::new(0.62, [-0.06, -0.08, 0.02], [-62.0, -8.0, 6.0], 1.0, 1),
            Key::new(1.0, REST, REST, 1.0, 2),
        ],
        // the old game's: the item tipped forward and down through the swing
        "1.7" => vec![
            Key::new(0.0, REST, [0.0, 0.0, 0.0], 1.0, 1),
            Key::new(0.5, [-0.2, 0.1, -0.1], [-80.0, -20.0, -20.0], 1.0, 1),
            Key::new(1.0, REST, REST, 1.0, 1),
        ],
        "Smooth" => vec![
            Key::new(0.0, REST, REST, 1.0, 1),
            Key::new(0.5, [-0.22, 0.08, -0.1], [-55.0, 10.0, -14.0], 1.0, 1),
            Key::new(1.0, REST, REST, 1.0, 1),
        ],
        // a whole turn of the item about its length
        "Spin" => vec![
            Key::new(0.0, REST, REST, 1.0, 1),
            Key::new(0.5, [-0.1, 0.06, -0.05], [-180.0, 0.0, 0.0], 1.0, 0),
            Key::new(1.0, REST, [-360.0, 0.0, 0.0], 1.0, 1),
        ],
        // down and out to the side, as Exhibition's
        "Slide" => vec![
            Key::new(0.0, REST, REST, 1.0, 1),
            Key::new(0.4, [0.12, -0.2, 0.0], [-20.0, 30.0, -40.0], 1.0, 3),
            Key::new(1.0, REST, REST, 1.0, 1),
        ],
        // a thrust forward
        "Stab" => vec![
            Key::new(0.0, REST, REST, 1.0, 1),
            Key::new(0.3, [0.0, 0.02, -0.35], [-10.0, 0.0, 0.0], 1.0, 3),
            Key::new(1.0, REST, REST, 1.0, 1),
        ],
        _ => vec![Key::new(0.0, REST, REST, 1.0, 1), Key::new(1.0, REST, REST, 1.0, 1)],
    }
}

/// Keyframes as a setting keeps them: "t px py pz rx ry rz size ease" by ";".
pub(super) fn keys_parse(s: &str) -> Vec<Key> {
    let mut v: Vec<Key> = s
        .split(';')
        .filter_map(|k| {
            let f: Vec<f32> = k.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (f.len() >= 9).then(|| Key::new(f[0].clamp(0.0, 1.0), [f[1], f[2], f[3]], [f[4], f[5], f[6]], f[7].clamp(0.2, 3.0), f[8] as u8 % 4))
        })
        .collect();
    v.sort_by(|a, b| a.t.total_cmp(&b.t));
    v
}

pub(super) fn keys_dump(v: &[Key]) -> String {
    v.iter()
        .map(|k| format!("{:.3} {:.3} {:.3} {:.3} {:.1} {:.1} {:.1} {:.3} {}", k.t, k.pos[0], k.pos[1], k.pos[2], k.rot[0], k.rot[1], k.rot[2], k.size, k.ease))
        .collect::<Vec<_>>()
        .join(";")
}

fn ease(e: u8, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    match e {
        0 => x,
        1 => x * x * (3.0 - 2.0 * x),
        // a spring: past and back
        2 => 1.0 - (-6.0 * x).exp() * (9.0 * x).cos(),
        // sharp: fast out, slow in
        _ => 1.0 - (1.0 - x).powi(3),
    }
}

/// Where a swing is at `p` (0..1): position, turn, size.
pub(super) fn at(keys: &[Key], p: f32) -> ([f32; 3], [f32; 3], f32) {
    match keys.len() {
        0 => (REST, REST, 1.0),
        1 => (keys[0].pos, keys[0].rot, keys[0].size),
        _ => {
            let p = p.clamp(0.0, 1.0);
            let i = keys.iter().rposition(|k| k.t <= p).unwrap_or(0).min(keys.len() - 2);
            let (a, b) = (keys[i], keys[i + 1]);
            let span = (b.t - a.t).max(1e-4);
            let x = ease(a.ease, (p - a.t) / span);
            let l = |u: f32, v: f32| u + (v - u) * x;
            (
                [l(a.pos[0], b.pos[0]), l(a.pos[1], b.pos[1]), l(a.pos[2], b.pos[2])],
                [l(a.rot[0], b.rot[0]), l(a.rot[1], b.rot[1]), l(a.rot[2], b.rot[2])],
                l(a.size, b.size),
            )
        }
    }
}

/// The kinds of items (Java tells which a held one is) as View Model's choice names them.
pub(super) const KINDS: [&str; 6] = ["Weapons", "Bows", "Food", "Blocks", "Tools", "Other"];

/// View Model's presets: right hand's x, y, z, size, turns x, y, z.
pub(super) const VM_PRESETS: [(&str, [f32; 7]); 5] = [
    ("Classic", [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
    ("Low", [0.0, -0.12, 0.0, 1.0, 0.0, 0.0, 0.0]),
    ("Centered", [-0.28, -0.04, -0.1, 0.9, 0.0, 0.0, 0.0]),
    ("Small", [0.04, -0.04, 0.05, 0.72, 0.0, 0.0, 0.0]),
    ("Far", [0.1, 0.0, -0.3, 1.0, 0.0, 0.0, 0.0]),
];
pub(super) const VM_LABELS: [&str; 7] = ["X", "Y", "Z", "Size", "Turn X", "Turn Y", "Turn Z"];

impl Gui {
    /// Java's ask for a hand this frame: "main|off\tkind\tprogress" → "vm x y z rx ry rz size" (View Model, when on
    /// and for this kind) and "swing x y z rx ry rz size" (when a style of ours is chosen), a line each.
    pub(super) fn hand_call(&mut self, a: &str) -> String {
        let mut f = a.split('\t');
        let (hand, kind, p) = (f.next().unwrap_or("main"), f.next().unwrap_or("Other"), f.next().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0));
        let empty = f.next() == Some("1");
        let physical = f.next().unwrap_or(if hand == "off" {"left"} else {"right"});
        let equip = f.next().and_then(|v|v.parse::<f32>().ok()).unwrap_or(0.0);
        let item_kind = f.next().and_then(|v|v.parse::<u8>().ok()).unwrap_or(6);
        let active = f.next()==Some("1");
        let eligible = f.next().unwrap_or("1")=="1";
        let ticks = f.next().and_then(|v|v.parse::<f32>().ok()).unwrap_or(0.0);
        let millis = f.next().and_then(|v|v.parse::<u64>().ok()).unwrap_or(0);
        let world = f.next().unwrap_or("");
        let tick_active = f.next().map_or(active, |v|v=="1");
        let mut out = Vec::new();
        // the swing's editor open: the main hand held where its time stands (put on with the hand's place, the game's
        // swing left out), the swing the one being made
        if self.swing_ed.on && hand == "main" {
            self.swing_ed.empty = empty;
            let (pos, rot, size) = at(&self.swing_ed.keys, self.swing_ed.t);
            out.push(format!("edit {} {} {} {} {} {} {}", pos[0], pos[1], pos[2], rot[0], rot[1], rot[2], size));
        }
        if let Some(set) = self.mods.iter().find(|m| m.id == "view-model" && m.on).map(|m| m.set.clone()) {
            let kinds_ok = match theme::find(&set, "Item kinds") {
                Some(super::data::Kind::Multi { on, opts }) => opts.iter().zip(on).any(|(o, on)| *on && *o == kind),
                _ => true,
            };
            if kinds_ok {
                // the left hand as the right, mirrored, when they are the same
                let left = physical == "left" && !theme::on(&set, "Same for both hands");
                let side = if left { "Left" } else { "Right" };
                let get = |l: &str| theme::num(&set, &format!("{side} {l}"));
                let mut v = [get("X"), get("Y"), get("Z"), get("Size"), get("Turn X"), get("Turn Y"), get("Turn Z")];
                if physical == "left" && !left {
                    // (mirrored: across, and the turns about up and forward)
                    v[0] = -v[0];
                    v[5] = -v[5];
                    v[6] = -v[6];
                }
                out.push(format!("vm {} {} {} {} {} {} {}", v[0], v[1], v[2], v[4], v[5], v[6], v[3]));
            }
        }
        if let Some(set) = self.mods.iter().find(|m| m.id == "swing-animations" && m.on).map(|m| m.set.clone()) {
            let source = theme::pick(&set, "Animation source");
            if source > 0 {
                let mode = theme::pick(&set, match source {1=>"Nurik style",2=>"Catlavan style",_=>"Newcode style"});
                if hand=="main" {
                    let context = format!("{world}:{source}:{mode}:{item_kind}:{eligible}:{}",self.swing_ed.on);
                    self.swing_port.phase(&context, if eligible {p}else{0.0});
                }
                if source == 2 && hand == "main" && !empty && !self.swing_ed.on {
                    out.push(format!("equip-factor {}", theme::num(&set, "Equip motion")));
                }
                if eligible && !empty && !self.swing_ed.on && (if source==3 {physical=="right"}else{hand=="main"}) {
                    let flipped = if hand=="main" {self.swing_port.flipped()}else{false};
                    let a = super::swingport::Input {p,direction:if physical=="left" {-1.0}else{1.0},
                        angle:theme::num(&set,"Swing angle"),power:theme::num(&set,if source==1 {"Nurik strength"}else{"Swing power"}),
                        equip,time_ms:millis,time_ticks:ticks,spinning:theme::on(&set,"Spinning"),smoothness:theme::num(&set,"Spin smoothness"),
                        active,tick_active,flipped,kind:item_kind};
                    out.push(format!("classic {source}"));
                    out.push(format!("ops {}",super::swingport::render(source,mode,a).wire()));
                }
                return out.join("\n");
            }
            let style = match theme::find(&set, "Style") {
                Some(super::data::Kind::Choice { v, opts }) => opts.get(*v).copied().unwrap_or("Vanilla"),
                _ => "Vanilla",
            };
            if style != "Vanilla" {
                let keys = if style == "Custom" { keys_parse(theme::text(&set, "Keyframes")) } else { preset(style) };
                let (pos, rot, size) = at(&keys, p);
                // (the left hand mirrored)
                let m = if hand == "off" { -1.0 } else { 1.0 };
                out.push(format!("swing {} {} {} {} {} {} {}", pos[0] * m, pos[1], pos[2], rot[0], rot[1] * m, rot[2] * m, size));
            }
        }
        out.join("\n")
    }

    /// View Model's preset: one chosen sets both hands' sliders; a slider moved off it makes it Custom (each menu frame).
    pub(super) fn vm_follow(&mut self) {
        let seen = self.vm_seen;
        let Some(m) = self.mods.iter_mut().find(|m| m.id == "view-model") else { return };
        let Some(pi) = m.set.iter().position(|s| s.label == "Preset") else { return };
        let preset = if let super::data::Kind::Choice { v, .. } = m.set[pi].kind { v } else { 0 };
        let custom = VM_PRESETS.len();
        if preset != seen && preset < custom {
            for side in ["Right", "Left"] {
                for (j, l) in VM_LABELS.iter().enumerate() {
                    let label = format!("{side} {l}");
                    if let Some(st) = m.set.iter_mut().find(|st| st.label == label) {
                        if let super::data::Kind::Slider { v, .. } = &mut st.kind {
                            *v = VM_PRESETS[preset].1[j];
                        }
                    }
                }
            }
        }
        // (still the preset's, or the player's own now)
        let base = if preset < custom { preset } else { seen.min(custom) };
        let same = base < custom
            && VM_LABELS.iter().enumerate().all(|(j, l)| {
                m.set.iter().find(|st| st.label == format!("Right {l}")).is_some_and(|st| matches!(st.kind, super::data::Kind::Slider { v, step, .. } if (v - VM_PRESETS[base].1[j]).abs() <= step * 0.5))
            });
        let now = if same { base } else { custom };
        if let super::data::Kind::Choice { v, .. } = &mut m.set[pi].kind {
            *v = now;
        }
        self.vm_seen = now;
    }

    /// A style's keyframes (the editor starts from one).
    pub(super) fn swing_preset(style: &str) -> Vec<Key> {
        preset(style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choose(g: &mut Gui, module: &str, label: &str, value: usize) {
        let i=g.mod_index(module);g.mods[i].on=true;
        for s in &mut g.mods[i].set {
            if s.label==label {
                if let super::super::data::Kind::Choice {v,..}=&mut s.kind {*v=value;}
            }
        }
    }

    #[test]
    fn source_poses_are_present_at_rest_but_not_during_use_or_on_the_wrong_hand() {
        let mut g=Gui::new();
        choose(&mut g,"swing-animations","Animation source",2);
        let idle=g.hand_call("main\tWeapons\t0\t0\tleft\t0.4\t0\t0\t1\t20\t141\tworld\t0");
        assert!(idle.contains("classic 2\nops r 0 1 0 -20"),"{idle}");
        assert!(g.hand_call("off\tWeapons\t0\t0\tright\t0\t0\t0\t1").is_empty());
        assert!(!g.hand_call("main\tWeapons\t0.5\t0\tleft\t0\t0\t1\t0").contains("classic"));
        assert!(g.hand_call("main\tWeapons\t0.5\t1\tleft\t0\t0\t1\t1").is_empty());
        choose(&mut g,"swing-animations","Animation source",3);
        assert!(g.hand_call("main\tWeapons\t0.5\t0\tleft\t0\t0\t1\t1").is_empty());
        assert!(g.hand_call("off\tWeapons\t0.5\t0\tright\t0\t0\t1\t1").contains("classic 3"));
    }

    #[test]
    fn view_model_uses_physical_left_hand_even_when_it_is_the_main_hand() {
        let mut g=Gui::new();let i=g.mod_index("view-model");g.mods[i].on=true;
        for s in &mut g.mods[i].set {
            if s.label=="Same for both hands" {s.kind=super::super::data::Kind::Toggle(false);}
            if s.label=="Left X" {if let super::super::data::Kind::Slider {v,..}=&mut s.kind {*v=0.33;}}
        }
        assert!(g.hand_call("main\tWeapons\t0\t0\tleft").starts_with("vm 0.33 "));
        assert!(g.hand_call("off\tWeapons\t0\t0\tright").starts_with("vm 0 "));
    }

    #[test]
    fn keyframes_are_kept_and_eased_between() {
        let k = preset("Smooth");
        let back = keys_parse(&keys_dump(&k));
        assert_eq!(back.len(), k.len());
        assert!((back[1].rot[0] - k[1].rot[0]).abs() < 0.1);
        let (p0, _, _) = at(&k, 0.0);
        let (p1, r1, _) = at(&k, 0.5);
        let (p2, _, _) = at(&k, 1.0);
        assert_eq!(p0, REST);
        assert!((p1[0] + 0.22).abs() < 1e-4 && (r1[0] + 55.0).abs() < 1e-3);
        assert!(p2.iter().all(|v| v.abs() < 1e-5));
        // a broken line is left out, the rest in time's order
        let v = keys_parse("1 0 0 0 0 0 0 1 1;bad;0 0 0 0 0 0 0 1 1");
        assert_eq!(v.len(), 2);
        assert!(v[0].t < v[1].t);
    }

    #[test]
    fn a_hand_is_told_its_pose_and_swing() {
        let mut g = Gui::new();
        assert_eq!(g.hand_call("main\tWeapons\t0.5"), "");
        let i = g.mod_index("swing-animations");
        g.mods[i].on = true;
        for s in g.mods[i].set.iter_mut() {
            if s.label == "Style" {
                if let super::super::data::Kind::Choice { v, opts } = &mut s.kind {
                    *v = opts.iter().position(|o| *o == "Smooth").unwrap();
                }
            }
        }
        let main = g.hand_call("main\tWeapons\t0.5");
        let off = g.hand_call("off\tWeapons\t0.5");
        assert!(main.starts_with("swing -0.22 "), "{main}");
        assert!(off.starts_with("swing 0.22 "), "{off}");
        let v = g.mod_index("view-model");
        g.mods[v].on = true;
        assert!(g.hand_call("main\tWeapons\t0.0").lines().any(|l| l.starts_with("vm ")));
    }
}
