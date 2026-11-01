//! What Java's old hybrid chat (HudChat: the game's words over the client's plates) sends, read so the numbers after
//! it stay in their places. The chat is the client's own now (chat/); this is never on.

/// The chat as Java has it this frame (HudChat.append).
#[derive(Clone, Default)]
pub struct Chat {
    pub focused: bool,
    /// the words' room (px), a line's height, the lines shown (px: as many as the page holds)
    pub width: f32,
    pub line_h: f32,
    pub page: f32,
    /// where the shown part is (0 the newest .. 1 the oldest), how much of all is shown, new lines below it
    pub scroll: f32,
    pub vis: f32,
    pub new_below: bool,
    pub gs: f32,
    /// the chat's own unit (px): its letters are 8 of them high
    pub unit: f32,
    pub lines: Vec<Line>,
}

/// A line shown: its place from the last line's bottom (px), how seen, the first of its message, naming you, a face
/// before it, its time (seconds into the day, -1 none), slid by (px), its message, since a repeat counted it, its words'
/// width (px).
#[derive(Clone, Copy, Default)]
pub struct Line {
    pub top: f32,
    pub bottom: f32,
    pub alpha: f32,
    pub first: bool,
    pub mention: bool,
    pub head: bool,
    pub clock: i32,
    pub dx: f32,
    pub msg: u32,
    pub bump: f32,
    pub w: f32,
}

impl Chat {
    /// From Java's numbers at `at`: a head of 10, then 9 a line. Returns it and how many numbers it took.
    pub fn parse(g: &dyn Fn(usize) -> f64, at: usize) -> (Chat, usize) {
        let n = (g(at).max(0.0) as usize).min(512);
        let mut ch = Chat {
            focused: g(at + 1) > 0.5,
            width: g(at + 2) as f32,
            line_h: (g(at + 3) as f32).max(1.0),
            page: g(at + 4) as f32,
            scroll: (g(at + 5) as f32).clamp(0.0, 1.0),
            vis: (g(at + 6) as f32).clamp(0.0, 1.0),
            new_below: g(at + 7) > 0.5,
            gs: (g(at + 8) as f32).max(1.0),
            unit: (g(at + 9) as f32).max(0.5),
            lines: Vec::with_capacity(n),
        };
        let mut i = at + 10;
        for _ in 0..n {
            let fl = g(i + 3) as i32;
            ch.lines.push(Line {
                top: g(i) as f32,
                bottom: g(i + 1) as f32,
                alpha: (g(i + 2) as f32).clamp(0.0, 1.0),
                first: fl & 1 != 0,
                mention: fl & 2 != 0,
                head: fl & 4 != 0,
                clock: g(i + 4) as i32,
                dx: g(i + 5) as f32,
                msg: g(i + 6) as u32,
                bump: g(i + 7) as f32,
                w: g(i + 8) as f32,
            });
            i += 9;
        }
        (ch, i - at)
    }
}
