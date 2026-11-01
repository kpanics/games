use super::{queue, view, Job};
use crate::gui::draw::{grey, mix, Canvas};
use crate::gui::i18n::{tr, trn};
use crate::gui::text::{fonts, Face};
use crate::gui::theme::{fade, pal};
use crate::gui::{ak, Card, Gui, Hit, A_HOV, A_POP};

fn timestamp(t: u64) -> String {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    if let Some(ticks) = t.checked_add(11_644_473_600).and_then(|n| n.checked_mul(10_000_000)) {
        let file = FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
        let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
        if unsafe { FileTimeToSystemTime(&file, &mut utc) != 0 && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0 } {
            return format!(
                "{:02}.{:02}.{:04} {:02}:{:02}:{:02}",
                local.wDay, local.wMonth, local.wYear, local.wHour, local.wMinute, local.wSecond
            );
        }
    }
    super::super::configs::when_text(t)
}

impl Gui {
    pub(in crate::gui) fn cloud_open(&mut self, at: [f32; 4]) {
        let tab = if view().conflict { 2 } else { 0 };
        self.card = Some(Card::Cloud { at, tab, page: 0 });
        self.card_at = self.now;
        self.card_arm = false;
        self.cloud_revoke = None;
        self.anim.remove(&ak(A_POP, 2));
        self.anim.remove(&ak(A_POP, 50_001));
        self.pop = None;
        self.pick = None;
        self.sound_tick(0.6);
        queue(Job::Refresh);
    }

    fn cloud_button(&mut self, c: &mut Canvas, b: [f32; 4], label: &str, part: u8, id: i64, value: i64, enabled: bool, primary: bool) {
        let hit = Hit::Cloud(part, id, value);
        let key = 50_000 + (((id as u64).wrapping_mul(97) ^ (value as u64).rotate_left(11) ^ (part as u64) * 101) & 0xfffff) as usize;
        let hover = self.an(ak(A_HOV, key), if enabled && self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
        let r = self.r;
        let fill = if primary {
            mix(fade(pal().accent, 0.85), pal().accent, hover)
        } else {
            mix(pal().f2, pal().f4, hover)
        };
        c.rect(b, 0.6 * r, fade(fill, if enabled { 1.0 } else { 0.45 }));
        let text = fonts().fit(Face::Semi, 0.76 * r, label, b[2] - b[0] - 1.3 * r);
        let color = if primary { grey(0.04, 1.0) } else { pal().t1 };
        c.text_c(
            (b[0] + b[2]) * 0.5,
            b[1] + (b[3] - b[1]) * 0.5 + 0.25 * r,
            0.76 * r,
            Face::Semi,
            &text,
            fade(color, if enabled { 1.0 } else { 0.35 }),
        );
        if enabled {
            self.hits.push((b, hit));
        }
    }

    pub(in crate::gui) fn cloud_card(&mut self, c: &mut Canvas, at: [f32; 4], tab: usize, page: usize) {
        let v = view();
        self.cloud_seen_version = v.version;
        let r = self.r;
        let open = self.an(ak(A_POP, 2), 1.0, 14.0, 0.0);
        let w = (35.0 * r).min(self.w - 20.0);
        let h = (34.5 * r).min(self.h - 24.0);
        let center = [
            (self.win[0] + self.win[2] * 0.5).clamp(w * 0.5 + 8.0, self.w - w * 0.5 - 8.0),
            (self.win[1] + self.win[3] * 0.5).clamp(h * 0.5 + 8.0, self.h - h * 0.5 - 8.0),
        ];
        let x = center[0] - w * 0.5;
        let y = center[1] - h * 0.5 + (1.0 - open) * 0.6 * r;
        let rr = [x, y, x + w, y + h];
        self.card_rect = rr;
        let original = c.alpha;
        c.alpha *= open;
        c.shadow(rr, 1.1 * r, 2.2 * r, grey(0.0, 0.55));
        let glass = pal().glass;
        c.rect(rr, 1.1 * r, [glass[0].max(0.05), glass[1].max(0.05), glass[2].max(0.055), 0.985]);
        c.rect_line(rr, 1.1 * r, 1.0, grey(1.0, 0.065));
        self.hits.push((rr, Hit::Cloud(0, 0, 0)));
        let left = x + 1.25 * r;
        let right = x + w - 1.25 * r;
        c.text(left, y + 2.0 * r, 1.15 * r, Face::Semi, tr("Cloud configs"), pal().t1);
        let who = if v.account.is_empty() {
            tr("Recode").to_string()
        } else if v.name.is_empty() {
            v.account.clone()
        } else {
            format!("{} · {}", v.account, v.name)
        };
        c.text(
            left,
            y + 3.3 * r,
            0.72 * r,
            Face::Regular,
            &fonts().fit(Face::Regular, 0.72 * r, &who, w - 6.0 * r),
            pal().t3,
        );
        self.cloud_button(c, [right - 2.1 * r, y + 1.1 * r, right, y + 3.2 * r], "×", 1, 0, 0, true, false);
        let status = fonts().fit(Face::Regular, 0.74 * r, tr(v.status), right - left);
        c.text(left, y + 4.8 * r, 0.74 * r, Face::Regular, &status, pal().t2);
        let mut tabs = vec![("Configs", 0usize), ("History", 1)];
        if v.conflict {
            tabs.push(("Conflict", 2));
        }
        tabs.push(("Sharing", 3));
        let current = if tab == 2 && !v.conflict { 0 } else { tab.min(3) };
        let tw = (right - left - 0.35 * r * (tabs.len() - 1) as f32) / tabs.len() as f32;
        for (i, (label, target)) in tabs.iter().enumerate() {
            let bx = left + i as f32 * (tw + 0.35 * r);
            let part = match target {
                0 => 2,
                1 => 3,
                2 => 13,
                _ => 14,
            };
            self.cloud_button(c, [bx, y + 5.8 * r, bx + tw, y + 7.85 * r], tr(label), part, 0, 0, true, current == *target);
        }
        let bottom = y + h - 1.2 * r;
        c.alpha *= self.an(ak(A_POP, 50_001), 1.0, 16.0, 0.0);
        if current == 2 {
            let top = y + 9.2 * r;
            let gap = 0.7 * r;
            let cw = (right - left - gap) * 0.5;
            for (i, (title, on, files)) in [("On this PC", v.local_on, v.local_files), ("In the cloud", v.remote_on, v.remote_files)]
                .into_iter()
                .enumerate()
            {
                let cx = left + i as f32 * (cw + gap);
                let card = [cx, top, cx + cw, (top + 14.0 * r).min(bottom - 3.2 * r)];
                c.rect(card, 0.8 * r, pal().f2);
                c.text(cx + r, top + 2.0 * r, 0.93 * r, Face::Semi, tr(title), pal().t1);
                let details = trn("{n} modules on", &[("n", on.to_string())]);
                c.text(cx + r, top + 4.0 * r, 0.76 * r, Face::Regular, &details, pal().t3);
                let details = trn("{n} saved configs", &[("n", files.to_string())]);
                c.text(cx + r, top + 5.5 * r, 0.76 * r, Face::Regular, &details, pal().t3);
                if i == 1 {
                    let line = trn("Version {v}", &[("v", v.version.to_string())]);
                    c.text(cx + r, top + 7.1 * r, 0.72 * r, Face::Regular, &line, pal().t4);
                }
                let (part, label) = if i == 0 {
                    (8, if v.own { "Keep local settings" } else { "Save my own copy" })
                } else {
                    (9, "Load cloud settings")
                };
                self.cloud_button(
                    c,
                    [cx + 0.65 * r, card[3] - 3.1 * r, cx + cw - 0.65 * r, card[3] - 0.8 * r],
                    tr(label),
                    part,
                    v.active,
                    v.version,
                    v.online && !v.busy,
                    i == 1,
                );
            }
            let name = fonts().fit(Face::Regular, 0.72 * r, &v.name, right - left);
            c.text(left, top + 16.0 * r, 0.72 * r, Face::Regular, &name, pal().t4);
        } else if current == 0 {
            let count = 6usize;
            let pages = v.entries.len().max(1).div_ceil(count);
            let page = page.min(pages - 1);
            let mut yy = y + 9.0 * r;
            if v.entries.is_empty() {
                c.text(left, yy + 2.0 * r, 0.82 * r, Face::Regular, tr("No cloud configs yet"), pal().t3);
            }
            for entry in v.entries.iter().skip(page * count).take(count) {
                let row = [left, yy, right, yy + 2.65 * r];
                let active = v.active == entry.id;
                c.rect(row, 0.65 * r, if active { pal().f3 } else { pal().f2 });
                let name = fonts().fit(Face::Semi, 0.82 * r, &entry.name, right - left - 7.5 * r);
                c.text(left + 0.8 * r, yy + 1.15 * r, 0.82 * r, Face::Semi, &name, pal().t1);
                let sub = format!(
                    "{} · {}",
                    tr(if entry.own { "Your config" } else { "Shared config" }),
                    trn("Version {v}", &[("v", entry.version.to_string())])
                );
                c.text(left + 0.8 * r, yy + 2.15 * r, 0.65 * r, Face::Regular, &sub, pal().t4);
                self.cloud_button(
                    c,
                    [right - 6.0 * r, yy + 0.4 * r, right - 0.45 * r, yy + 2.25 * r],
                    tr(if active { "Selected" } else { "Load" }),
                    6,
                    entry.id,
                    0,
                    v.online && !v.busy && !active,
                    false,
                );
                yy += 3.05 * r;
            }
            let by = bottom - 5.0 * r;
            self.cloud_button(c, [left, by, left + 2.3 * r, by + 2.0 * r], "‹", 11, 0, 0, page > 0, false);
            self.cloud_button(
                c,
                [left + 2.7 * r, by, left + 5.0 * r, by + 2.0 * r],
                "›",
                12,
                pages as i64,
                0,
                page + 1 < pages,
                false,
            );
            c.text(left + 6.0 * r, by + 1.35 * r, 0.7 * r, Face::Mono, &format!("{}/{}", page + 1, pages), pal().t4);
            self.cloud_button(
                c,
                [left, bottom - 2.2 * r, right - 6.5 * r, bottom],
                tr("Save a cloud copy"),
                5,
                0,
                0,
                v.online && !v.busy,
                true,
            );
            self.cloud_button(c, [right - 6.0 * r, bottom - 2.2 * r, right, bottom], tr("Refresh"), 4, 0, 0, !v.busy, false);
        } else if current == 3 {
            let top = y + 9.0 * r;
            c.text(left, top + 0.8 * r, 0.76 * r, Face::Semi, tr("Invitation code"), pal().t2);
            let field = [left, top + 1.5 * r, right - 14.0 * r, top + 4.1 * r];
            self.field(c, field, &self.card_field.clone(), "ABCD-EFGH");
            self.hits.push((field, Hit::Cloud(15, 0, 0)));
            self.cloud_button(
                c,
                [right - 13.5 * r, field[1], right - 8.5 * r, field[3]],
                tr("Paste"),
                20,
                0,
                0,
                !v.busy,
                false,
            );
            self.cloud_button(
                c,
                [right - 8.0 * r, field[1], right, field[3]],
                tr("Load"),
                19,
                0,
                0,
                v.online && !v.busy && !self.card_field.trim().is_empty(),
                true,
            );
            let heading = if v.own {
                format!("{} · {}/10", tr("Your invitation codes"), v.invitations.len())
            } else {
                tr("Invitations are managed by the owner").to_string()
            };
            if self.cloud_revoke.as_ref().is_some_and(|(id, _, until)| *id == v.active && *until >= self.now) {
                c.text(left, top + 5.1 * r, 0.65 * r, Face::Regular, tr("Revoking also removes existing access"), pal().t2);
            }
            c.text(
                left,
                top + 6.1 * r,
                0.76 * r,
                Face::Semi,
                &fonts().fit(Face::Semi, 0.76 * r, &heading, right - left),
                pal().t3,
            );
            let pages = v.invitations.len().max(1).div_ceil(3);
            let page = page.min(pages - 1);
            let mut yy = top + 7.1 * r;
            self.cloud_codes = v.invitations.iter().skip(page * 3).take(3).map(|i| i.code.clone()).collect();
            for (i, code) in v.invitations.iter().skip(page * 3).take(3).enumerate() {
                let armed = self.cloud_revoke.as_ref().is_some_and(|(id, key, until)| *id == v.active && *key == code.code && *until >= self.now);
                let shown = fonts().fit(Face::Mono, 0.9*r, &code.code, right-left-14.5*r);
                c.text(left, yy + 1.1 * r, 0.9 * r, Face::Mono, &shown, pal().t1);
                let count = trn(
                    "{used} of {max} activations",
                    &[("used", code.uses.to_string()), ("max", code.max_uses.to_string())],
                );
                c.text(left, yy + 2.25 * r, 0.66 * r, Face::Regular, &count, pal().t4);
                self.cloud_button(
                    c,
                    [right - 13.8 * r, yy + 0.3 * r, right - 7.5 * r, yy + 2.5 * r],
                    tr("Copy"),
                    17,
                    i as i64,
                    0,
                    !v.busy && code.uses < code.max_uses,
                    false,
                );
                self.cloud_button(
                    c,
                    [right - 7.0 * r, yy + 0.3 * r, right, yy + 2.5 * r],
                    tr(if armed { "Revoke?" } else { "Revoke" }),
                    18,
                    i as i64,
                    v.active,
                    v.own && v.online && !v.busy,
                    false,
                );
                yy += 3.6 * r;
            }
            if v.own && v.invitations.is_empty() {
                c.text(left, yy + 1.5 * r, 0.75 * r, Face::Regular, tr("No invitation codes yet"), pal().t4);
            }
            let py = bottom - 5.0 * r;
            self.cloud_button(c, [left, py, left + 2.3 * r, py + 1.8 * r], "‹", 11, 0, 0, page > 0, false);
            self.cloud_button(
                c,
                [left + 2.7 * r, py, left + 5.0 * r, py + 1.8 * r],
                "›",
                12,
                pages as i64,
                0,
                page + 1 < pages,
                false,
            );
            c.text(left + 6.0 * r, py + 1.25 * r, 0.7 * r, Face::Mono, &format!("{}/{}", page + 1, pages), pal().t4);
            self.cloud_button(
                c,
                [left, bottom - 2.2 * r, right - 6.5 * r, bottom],
                tr("Create a one-use code"),
                16,
                v.active,
                0,
                v.own && v.online && !v.busy && v.invitations.len() < 10,
                true,
            );
            self.cloud_button(c, [right - 6.0 * r, bottom - 2.2 * r, right, bottom], tr("Refresh"), 4, 0, 0, !v.busy, false);
        } else {
            let count = 4usize;
            let pages = v.versions.len().div_ceil(count).max(v.backups.len().div_ceil(2)).max(1);
            let page = page.min(pages - 1);
            let mut yy = y + 9.0 * r;
            c.text(left, yy + 0.8 * r, 0.74 * r, Face::Semi, tr("Cloud versions"), pal().t3);
            yy += 1.5 * r;
            if v.versions.is_empty() {
                c.text(left, yy + 1.0 * r, 0.75 * r, Face::Regular, tr("No previous cloud versions"), pal().t4);
                yy += 2.8 * r;
            }
            for version in v.versions.iter().skip(page * count).take(count) {
                let label = trn("Version {v}", &[("v", version.version.to_string())]);
                c.text(left, yy + 1.0 * r, 0.8 * r, Face::Semi, &label, pal().t1);
                let sub = format!("{} · {} KB", timestamp(version.at), version.bytes.div_ceil(1024));
                c.text(left, yy + 2.0 * r, 0.65 * r, Face::Regular, &sub, pal().t4);
                self.cloud_button(
                    c,
                    [right - 7.0 * r, yy + 0.15 * r, right, yy + 2.25 * r],
                    tr("Restore"),
                    7,
                    v.active,
                    version.version,
                    v.online && v.own && !v.busy,
                    false,
                );
                yy += 2.8 * r;
            }
            self.cloud_button(c, [left, yy, left + 2.3 * r, yy + 1.8 * r], "‹", 11, 0, 0, page > 0, false);
            self.cloud_button(
                c,
                [left + 2.7 * r, yy, left + 5.0 * r, yy + 1.8 * r],
                "›",
                12,
                pages as i64,
                0,
                page + 1 < pages,
                false,
            );
            c.text(left + 6.0 * r, yy + 1.25 * r, 0.7 * r, Face::Mono, &format!("{}/{}", page + 1, pages), pal().t4);
            yy += 3.1 * r;
            c.text(left, yy, 0.74 * r, Face::Semi, tr("Local copies"), pal().t3);
            self.cloud_local = v.backups.iter().skip(page * 2).take(2).map(|b| b.id.clone()).collect();
            yy += 0.75 * r;
            for (i, backup) in v.backups.iter().skip(page * 2).take(2).enumerate() {
                c.text(left, yy + 1.1 * r, 0.75 * r, Face::Regular, &timestamp(backup.at), pal().t2);
                self.cloud_button(c, [right - 7.0 * r, yy, right, yy + 2.05 * r], tr("Restore"), 10, i as i64, 0, !v.busy, false);
                yy += 2.5 * r;
            }
            if v.backups.is_empty() {
                c.text(left, yy + 1.1 * r, 0.75 * r, Face::Regular, tr("No local copies yet"), pal().t4);
            }
            self.cloud_button(c, [right - 6.0 * r, bottom - 2.2 * r, right, bottom], tr("Refresh"), 4, 0, 0, !v.busy, false);
        }
        c.alpha = original;
        let _ = at; // Like the reset dialog, this card is centred rather than anchored to a row.
    }

    pub(in crate::gui) fn cloud_paste(&mut self, text: &str) {
        self.card_field = text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(32)
            .map(|c| c.to_ascii_uppercase())
            .collect();
    }

    pub(in crate::gui) fn cloud_press(&mut self, hit: Hit, button: i32) -> bool {
        let Hit::Cloud(part, id, value) = hit else {
            self.card_shut = self.card.take();
            return false;
        };
        if button != 0 {
            return true;
        }
        let Some(Card::Cloud { at, mut tab, mut page }) = self.card.clone() else {
            return false;
        };
        let previous = (tab, page);
        if matches!(part, 16 | 18 | 19) && view().busy {
            return true;
        }
        match part {
            0 => return true,
            1 => {
                self.card = None;
                return true;
            }
            2 => {
                tab = 0;
                page = 0;
            }
            3 => {
                tab = 1;
                page = 0;
                queue(Job::History);
            }
            13 => {
                tab = 2;
                page = 0;
            }
            14 => {
                tab = 3;
                page = 0;
                self.card_field.clear();
                queue(Job::Refresh);
            }
            15 => return true,
            16 => queue(Job::MakeInvitation(id)),
            17 => {
                if let Some(code) = self.cloud_codes.get(id.max(0) as usize).cloned() {
                    self.copy(&code);
                    self.toast(tr("The code is copied: paste it to a friend").to_string());
                }
            }
            18 => {
                if let Some(code) = self.cloud_codes.get(id.max(0) as usize).cloned() {
                    if self.cloud_revoke.as_ref().is_some_and(|(cfg, key, until)| *cfg == value && *key == code && *until >= self.now) {
                        self.cloud_revoke = None;
                        queue(Job::RevokeInvitation(value, code));
                    } else {
                        self.cloud_revoke = Some((value, code, self.now + 4.0));
                    }
                }
            }
            19 => queue(Job::RedeemInvitation(self.card_field.trim().to_ascii_uppercase())),
            20 => self.outbox.push("paste".into()),
            4 => queue(if tab == 1 { Job::History } else { Job::Refresh }),
            5 => queue(Job::Create(self.own_config_name().unwrap_or_else(|| "Recode".into()))),
            6 => {
                queue(Job::Activate(id));
                self.card = None;
                return true;
            }
            7 => {
                queue(Job::RestoreVersion(id, value, self.cloud_seen_version));
                self.card = None;
                return true;
            }
            8 => {
                queue(Job::KeepLocal(id, value));
                self.card = None;
                return true;
            }
            9 => {
                queue(Job::TakeCloud(id, value));
                self.card = None;
                return true;
            }
            10 => {
                if let Some(name) = self.cloud_local.get(id.max(0) as usize).cloned() {
                    queue(Job::RestoreLocal(name));
                }
                self.card = None;
                return true;
            }
            11 => page = page.saturating_sub(1),
            12 => page = (page + 1).min(id.saturating_sub(1).max(0) as usize),
            _ => {}
        }
        self.card = Some(Card::Cloud { at, tab, page });
        if previous != (tab, page) {
            self.cloud_revoke = None;
            self.anim.remove(&ak(A_POP, 50_001));
        }
        self.sound_tick(0.4);
        true
    }
}
