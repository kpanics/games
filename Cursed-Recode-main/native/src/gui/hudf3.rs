//! Custom F3's cards (Interface → Custom F3): what the game's F3 shows that a player uses, in the HUD's style — where
//! you are, how the game runs (with its frames' graph), the world round you, what the crosshair is on — each a card of
//! its own, dragged where you like, each line switched on or off. They show while F3 is on (the game's own words are
//! not drawn then: Java's HudF3), or always if a card is set so. The values are Java's (HudVars: the placeholders).

use super::*;

/// How many of the frames' samples the graph keeps, and how often one is taken (s).
const GRAPH: usize = 72;
const GRAPH_STEP: f32 = 0.08;

/// A line of a card.
enum Row {
    /// a name and its value
    Kv(String, String),
    /// a name, its value and a bar under it (0..1)
    Bar(String, String, f32),
    /// a name, its value and the frames' graph under it
    Graph(String, String),
    /// a name and its value of several lines
    Lines(String, Vec<String>),
}

impl Gui {
    /// A placeholder's value now (HudVars), or "".
    fn f3v(&self, k: &str) -> String {
        self.hud.facts.vars.get(k).cloned().unwrap_or_default()
    }

    /// Whether a card shows now: F3 on (or always, if it is set so); the editor's example always.
    fn f3_shown(&self, x: &Ctx) -> bool {
        x.sample || self.hud.facts.f3 || !x.on("Only with F3")
    }

    /// The frames a second over the last few seconds (a sample every GRAPH_STEP).
    fn f3_sample(&mut self, fps: f32) {
        let t = self.now;
        if t - self.hud.f3_at < GRAPH_STEP && !self.hud.f3_fps.is_empty() {
            return;
        }
        self.hud.f3_at = t;
        self.hud.f3_fps.push_back(fps);
        while self.hud.f3_fps.len() > GRAPH {
            self.hud.f3_fps.pop_front();
        }
    }

    /// A card: its title and its lines on the element's plate; its size.
    fn f3_card(&mut self, c: &mut Canvas, x: &Ctx, title: &str, rows: Vec<Row>) -> Option<[f32; 2]> {
        if rows.is_empty() {
            return None;
        }
        let s = x.s;
        let p = x.pad();
        let (ls, vs) = (11.0 * s, 11.5 * s);
        let rh = 18.5 * s;
        let graph_h = 34.0 * s;
        let bar_h = 7.0 * s;
        let titled = x.on("Title");
        let th = if titled { 22.0 * s } else { 0.0 };
        // the names' column, the values' column
        let lw = rows
            .iter()
            .map(|r| match r {
                Row::Kv(l, _) | Row::Bar(l, _, _) | Row::Graph(l, _) | Row::Lines(l, _) => fonts().width(Face::Regular, ls, l),
            })
            .fold(0.0, f32::max)
            + 12.0 * s;
        let vw = rows
            .iter()
            .map(|r| match r {
                Row::Kv(_, v) | Row::Bar(_, v, _) | Row::Graph(_, v) => fonts().width(Face::Mono, vs, v),
                Row::Lines(_, vs_) => vs_.iter().map(|v| fonts().width(Face::Mono, vs, v)).fold(0.0, f32::max),
            })
            .fold(0.0, f32::max);
        let tw = if titled { fonts().width(Face::Semi, 12.0 * s, title) + 18.0 * s } else { 0.0 };
        let w = (2.0 * p + lw + vw).max(2.0 * p + tw).max(196.0 * s);
        let body: f32 = rows
            .iter()
            .map(|r| match r {
                Row::Kv(..) => rh,
                Row::Bar(..) => rh + bar_h,
                Row::Graph(..) => rh + graph_h + 4.0 * s,
                Row::Lines(_, v) => rh * v.len().max(1) as f32,
            })
            .sum();
        let h = 2.0 * p + th + body - 3.0 * s;
        x.plate(c, [M, M, M + w, M + h]);
        let mut y = M + p;
        if titled {
            // a mark in the accent, the title, a hairline under it
            c.rect([M + p, y + 3.0 * s, M + p + 3.0 * s, y + 13.0 * s], 1.5 * s, x.el.accent);
            c.text(M + p + 9.0 * s, y + 12.0 * s, 12.0 * s, Face::Semi, title, x.el.text);
            c.rect([M + p, y + 19.0 * s, M + w - p, y + 19.0 * s + 1.0], 0.0, fade(x.el.text2, 0.18));
            y += th;
        }
        let base = |y: f32| y - 1.5 * s + rh / 2.0 + 4.2 * s;
        let x0 = M + p;
        let xv = M + p + lw;
        let right = M + w - p;
        for r in rows {
            match r {
                Row::Kv(l, v) => {
                    c.text(x0, base(y), ls, Face::Regular, &l, x.el.text2);
                    c.text(xv, base(y), vs, Face::Mono, &v, x.el.text);
                    y += rh;
                }
                Row::Bar(l, v, k) => {
                    c.text(x0, base(y), ls, Face::Regular, &l, x.el.text2);
                    c.text(xv, base(y), vs, Face::Mono, &v, x.el.text);
                    let by = y + rh - 1.0 * s;
                    c.rect([xv, by, right, by + 3.0 * s], 1.5 * s, fade(x.el.text2, 0.16));
                    c.rect([xv, by, xv + (right - xv) * k.clamp(0.0, 1.0), by + 3.0 * s], 1.5 * s, x.el.accent);
                    y += rh + bar_h;
                }
                Row::Graph(l, v) => {
                    c.text(x0, base(y), ls, Face::Regular, &l, x.el.text2);
                    c.text(xv, base(y), vs, Face::Mono, &v, x.el.text);
                    y += rh;
                    let g = [x0, y + 2.0 * s, right, y + 2.0 * s + graph_h];
                    self.f3_graph(c, x, g);
                    y += graph_h + 4.0 * s;
                }
                Row::Lines(l, vs_) => {
                    c.text(x0, base(y), ls, Face::Regular, &l, x.el.text2);
                    for v in &vs_ {
                        c.text(xv, base(y), vs, Face::Mono, v, x.el.text);
                        y += rh;
                    }
                    if vs_.is_empty() {
                        y += rh;
                    }
                }
            }
        }
        Some([w, h])
    }

    /// The frames' graph in `r`: each sample a column from its foot (the highest the top), the newest at the right; a
    /// faint line where 60 frames are; a frame slow enough to be felt (under 40 a second) in a warm colour.
    fn f3_graph(&self, c: &mut Canvas, x: &Ctx, r: [f32; 4]) {
        let s = x.s;
        c.rect(r, 4.0 * s, fade(x.el.text2, 0.06));
        let samples: Vec<f32> = if x.sample && self.hud.f3_fps.len() < 8 {
            (0..GRAPH).map(|i| 240.0 + 60.0 * ((i as f32) * 0.37).sin() + 30.0 * ((i as f32) * 1.3).cos()).collect()
        } else {
            self.hud.f3_fps.iter().copied().collect()
        };
        if samples.is_empty() {
            return;
        }
        let most = samples.iter().copied().fold(60.0, f32::max) * 1.1;
        let col = x.col("Graph colour", x.el.accent);
        let w = (r[2] - r[0]) / GRAPH as f32;
        let n = samples.len();
        for (i, v) in samples.iter().enumerate() {
            let k = (v / most).clamp(0.0, 1.0);
            let x1 = r[2] - (n - i) as f32 * w;
            let top = r[3] - (r[3] - r[1]) * k;
            // the slow ones warm, the newest brightest
            let a = 0.35 + 0.65 * (i as f32 / n as f32);
            let cc: Rgba = if *v < 40.0 { [1.0, 0.45, 0.35, col[3]] } else { col };
            c.rect([x1 + 0.6 * s, top, x1 + w - 0.6 * s, r[3] - 1.0 * s], 0.8 * s, fade(cc, 0.55 * a));
        }
        let y60 = r[3] - (r[3] - r[1]) * (60.0 / most);
        if y60 > r[1] {
            c.rect([r[0], y60, r[2], y60 + 1.0], 0.0, fade(x.el.text2, 0.25));
        }
    }

    pub(super) fn el_f3_position(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if !self.f3_shown(x) {
            return None;
        }
        let f = &self.hud.facts;
        let d = x.pick("Decimals");
        let mut rows = Vec::new();
        // (Streamer Mode hiding the coordinates: dots in their place)
        let hide = self.streamer_hides("Coordinates");
        let dots = |v: String| if hide { "•••".to_string() } else { v };
        if x.on("XYZ") {
            rows.push(Row::Kv("XYZ".into(), dots(format!("{:.*}  {:.*}  {:.*}", d, f.pos[0], d, f.pos[1], d, f.pos[2]))));
        }
        if x.on("Block") {
            rows.push(Row::Kv(tr("Block").into(), dots(format!("{}  {}  {}", self.f3v("bx"), self.f3v("by"), self.f3v("bz")))));
        }
        if x.on("Chunk") {
            rows.push(Row::Kv(tr("Chunk").into(), dots(format!("{}  {}  {}", self.f3v("cx"), self.f3v("cy"), self.f3v("cz")))));
        }
        if x.on("In the chunk") {
            rows.push(Row::Kv(tr("In the chunk").into(), dots(format!("{}  {}  {}", self.f3v("ix"), self.f3v("iy"), self.f3v("iz")))));
        }
        let f = &self.hud.facts;
        if x.on("Facing") {
            rows.push(Row::Kv(tr("Facing").into(), format!("{}  {:.1}°", tr(facing_of(f.yaw)), f.yaw.rem_euclid(360.0))));
        }
        if x.on("Pitch") {
            rows.push(Row::Kv(tr("Pitch").into(), format!("{}°", self.f3v("pitch"))));
        }
        let dim = self.f3v("dimension");
        if x.on("Dimension") {
            let name = match dim.as_str() {
                "nether" => tr("Nether"),
                "end" => tr("The End"),
                _ => tr("Overworld"),
            };
            rows.push(Row::Kv(tr("Dimension").into(), name.into()));
        }
        if x.on("Other dimension") && dim != "end" {
            let name = if dim == "nether" { tr("Overworld") } else { tr("Nether") };
            rows.push(Row::Kv(name.into(), format!("{}  {}", self.f3v("nx"), self.f3v("nz"))));
        }
        self.f3_card(c, x, tr("Position"), rows)
    }

    pub(super) fn el_f3_performance(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let fps = self.hud.facts.fps;
        self.f3_sample(fps);
        if !self.f3_shown(x) {
            return None;
        }
        let f = &self.hud.facts;
        let mut rows = Vec::new();
        if x.on("FPS") {
            let fps = if x.sample && f.fps <= 0.0 { 240.0 } else { f.fps };
            let v = format!("{:.0}", fps);
            rows.push(if x.on("Graph") { Row::Graph("FPS".into(), v) } else { Row::Kv("FPS".into(), v) });
        }
        if x.on("Frame time") {
            rows.push(Row::Kv(tr("Frame time").into(), format!("{} {}", self.f3v("frametime"), tr("ms"))));
        }
        if x.on("Ping") {
            rows.push(Row::Kv(tr("Ping").into(), format!("{:.0} {}", self.hud.facts.ping, tr("ms"))));
        }
        if x.on("Memory") {
            let used: f32 = self.f3v("memoryused").parse().unwrap_or(0.0);
            let max: f32 = self.f3v("memorymax").parse().unwrap_or(0.0);
            let v = format!("{:.0} / {:.0} MB", used, max);
            rows.push(if x.on("Memory bar") && max > 0.0 { Row::Bar(tr("Memory").into(), v, used / max) } else { Row::Kv(tr("Memory").into(), v) });
        }
        if x.on("Video card") {
            let g = self.f3v("gpu");
            rows.push(Row::Kv(tr("Video card").into(), fonts().fit(Face::Mono, 11.5 * x.s, &g, 220.0 * x.s)));
        }
        if x.on("Entities") {
            rows.push(Row::Kv(tr("Entities").into(), self.f3v("entities")));
        }
        self.f3_card(c, x, tr("Performance"), rows)
    }

    pub(super) fn el_f3_world(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if !self.f3_shown(x) {
            return None;
        }
        let mut rows = Vec::new();
        if x.on("Biome") {
            rows.push(Row::Kv(tr("Biome").into(), self.f3v("biome")));
        }
        if x.on("Light") {
            rows.push(Row::Kv(tr("Light").into(), format!("{} {}  ·  {} {}", tr("sky"), self.f3v("skylight"), tr("blocks"), self.f3v("blocklight"))));
        }
        if x.on("Time") {
            rows.push(Row::Kv(tr("Time").into(), self.f3v("gametime")));
        }
        if x.on("Day") {
            rows.push(Row::Kv(tr("Day").into(), self.f3v("day")));
        }
        if x.on("Weather") {
            let w = match self.f3v("weather").as_str() {
                "thunder" => tr("Thunder"),
                "rain" => tr("Rain"),
                _ => tr("Clear"),
            };
            rows.push(Row::Kv(tr("Weather").into(), w.into()));
        }
        if x.on("Moon") {
            const MOON: [&str; 8] = ["Full moon", "Waning gibbous", "Third quarter", "Waning crescent", "New moon", "Waxing crescent", "First quarter", "Waxing gibbous"];
            let k: usize = self.f3v("moon").parse().unwrap_or(0);
            rows.push(Row::Kv(tr("Moon").into(), tr(MOON[k % 8]).into()));
        }
        if x.on("Difficulty") {
            rows.push(Row::Kv(tr("Difficulty").into(), self.f3v("difficulty")));
        }
        self.f3_card(c, x, tr("World"), rows)
    }

    pub(super) fn el_f3_target(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        if !self.f3_shown(x) {
            return None;
        }
        let mut block = self.f3v("lookblock");
        let mut entity = self.f3v("lookentity");
        // the editor's example: a block and a creature
        let sample = x.sample && block.is_empty() && entity.is_empty();
        if sample {
            block = tr("Example block").into();
            entity = tr("Example creature").into();
        }
        let mut rows = Vec::new();
        if !entity.is_empty() {
            if x.on("Creature") {
                rows.push(Row::Kv(tr("Creature").into(), entity));
            }
            if x.on("Creature id") {
                rows.push(Row::Kv("ID".into(), if sample { "minecraft:zombie".into() } else { self.f3v("lookentitytype") }));
            }
            let hp: f32 = if sample { 14.0 } else { self.f3v("lookentityhp").parse().unwrap_or(-1.0) };
            let max: f32 = if sample { 20.0 } else { self.f3v("lookentitymax").parse().unwrap_or(0.0) };
            if x.on("Health") && hp >= 0.0 && max > 0.0 {
                rows.push(Row::Bar(tr("Health").into(), format!("{:.1} / {:.0}", hp, max), hp / max));
            }
            if x.on("Distance") {
                let d = if sample { "3.4".into() } else { self.f3v("lookentitydist") };
                rows.push(Row::Kv(tr("Distance").into(), format!("{} {}", d, tr("b"))));
            }
        } else if !block.is_empty() {
            if x.on("Block") {
                rows.push(Row::Kv(tr("Block").into(), block));
            }
            if x.on("Block id") {
                rows.push(Row::Kv("ID".into(), self.f3v("lookblockid")));
            }
            if x.on("Block place") {
                rows.push(Row::Kv(tr("Block place").into(), format!("{}  {}  {}", self.f3v("lookbx"), self.f3v("lookby"), self.f3v("lookbz"))));
            }
            if x.on("State") {
                let most = x.num("State lines").max(1.0) as usize;
                let st: Vec<String> = self.f3v("lookstate").split('\u{2}').filter(|l| !l.is_empty()).take(most).map(str::to_string).collect();
                if !st.is_empty() {
                    rows.push(Row::Lines(tr("State").into(), st));
                }
            }
            let liquid = self.f3v("lookfluid");
            if x.on("Liquid") && !liquid.is_empty() {
                rows.push(Row::Kv(tr("Liquid").into(), liquid));
            }
        }
        self.f3_card(c, x, tr("Looking at"), rows)
    }
}
