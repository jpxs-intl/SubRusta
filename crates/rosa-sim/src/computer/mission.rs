use super::{Computer, Mode, SCREEN_LINES, WHITE, text_len};
use crate::sim::Sim;

/// The mission screen: rows 5 to 11 list the day's missions, row 3 asks for intel, row 4 uploads a file and row 12
/// exits; a mission's own page shows its status lines and BACK on row 12.
const MISSION_ROWS: i32 = 7;
const INTEL_ROW: i32 = -2;
const UPLOAD_ROW: i32 = -1;
const EXIT_ROW: i32 = 7;
const CONNECT_TICKS: i32 = 0x100;
const UPLOAD_TICKS: i32 = 0xff;
const HIGHLIGHT: u8 = 0xe8;
const BUTTON: u8 = 0xb8;
const TITLE: u8 = 0xf0;
const KEY_UP: i32 = 0x10;
const KEY_DOWN: i32 = 0x11;
const KEY_ENTER: i32 = 0xa;
const KEY_BACK: i32 = 8;
const PAGE: i32 = 16;
/// An intel request can be made from states 1 to 8; 0 asked, 11 faxed.
const INTEL_FAXED: i32 = 0xb;
const INTEL_ASKABLE: i32 = 8;

/// MISSION.EXE's state (item +0x1660 on): what the connection is for (0 the menu, 1 an upload) and its ticks left,
/// the page (0 menu, 1 a mission, 2 the upload list), the menu row picked, the mission page's button, the file picked,
/// the directory listed and the files listed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MissionScreen {
    pub kind: i32,
    pub timer: i32,
    pub page: i32,
    pub selected: i32,
    pub button: i32,
    pub file: i32,
    pub dir: i32,
    pub files: Vec<i32>,
}

impl Computer {
    /// Writes `text` over the first `width` columns of line `l`, the rest spaces, in `color`, ended at `width`.
    fn field(&mut self, l: usize, text: &[u8], width: usize, color: u8) {
        let n = text_len(text);
        for k in 0..width {
            self.lines[l][k] = if k < n { text[k] } else { b' ' };
            self.colors[l][k] = color;
        }
        self.lines[l][width] = 0;
    }
}

impl Sim {
    /// computer_mission_screen_init: CONNECTING... while the modem dials for 256 ticks.
    pub(crate) fn mission_init(&mut self, id: usize, c: &mut Computer) {
        c.current_line = 0;
        c.top_line = 0;
        c.mission = MissionScreen { selected: -1, ..Default::default() };
        for l in 0..SCREEN_LINES {
            c.lines[l][0] = 0;
            if l == 0 {
                super::put_text(&mut c.lines[0], b"CONNECTING...");
            }
            c.colors[l][..63].fill(WHITE);
            self.register_line(id, c, l as i32);
        }
        c.mission.kind = 0;
        c.mission.timer = CONNECT_TICKS;
        self.phone_sound(rosa_protocol::clientbound::game::events::sound::Sound::Modem, id, 1.0);
    }

    /// The menu row of intel: what can be asked for.
    fn intel_text(&self, team: usize) -> &'static [u8] {
        match self.corp_state[team].intel {
            1..=INTEL_ASKABLE => b"REQUEST INTEL",
            0 => b"INTEL REQUESTED",
            INTEL_FAXED => b"INTEL FAXED",
            _ => b"NO INTEL AVAILABLE",
        }
    }

    /// computer_mission_render_menu.
    fn mission_menu(&mut self, id: usize, c: &mut Computer) {
        let Some(team) = usize::try_from(c.team).ok() else {
            c.mission.page = 0;
            return;
        };
        for row in 0..SCREEN_LINES as i32 {
            let l = row as usize;
            if row == 1 {
                c.field(1, b"MISSION SELECT", 16, WHITE);
            } else if (3..=12).contains(&row) {
                let r = row - 5;
                let text = match r {
                    INTEL_ROW => self.intel_text(team).to_vec(),
                    UPLOAD_ROW => b"UPLOAD FILE".to_vec(),
                    EXIT_ROW => b"EXIT".to_vec(),
                    _ => {
                        let m = self.world_missions.missions.get(r as usize).cloned().unwrap_or_default();
                        let mut s = if m.state != 0 { format!("{}. PROJECT {}                ", r + 1, m.names[3]) } else { format!("{}. UNKNOWN                   ", r + 1) };
                        s.truncate(24);
                        s += &m.teams[team].word;
                        s.into_bytes()
                    }
                };
                c.field(l, &text, 32, if c.mission.selected == r { HIGHLIGHT } else { WHITE });
            } else {
                c.lines[l][0] = 0;
            }
        }
        for l in 0..SCREEN_LINES as i32 {
            self.register_line(id, c, l);
        }
    }

    /// computer_mission_render_submenu: the mission's name, the corporation's status lines and BACK.
    fn mission_page(&mut self, id: usize, c: &mut Computer) {
        let (Some(team), Ok(m)) = (usize::try_from(c.team).ok(), usize::try_from(c.mission.selected)) else {
            c.mission.page = 0;
            return;
        };
        let mission = self.world_missions.missions.get(m).cloned().unwrap_or_default();
        for row in 0..SCREEN_LINES {
            match row {
                0 => {
                    let t = if mission.state != 0 { format!("PROJECT {}", mission.names[3]) } else { "UNKNOWN                     ".into() };
                    c.field(0, t.as_bytes(), 20, WHITE);
                }
                3 => c.field(3, b"STATUS", 20, WHITE),
                4..=7 => match mission.teams[team].lines.get(row - 4) {
                    Some(s) => c.field(row, s.as_bytes(), 60, WHITE),
                    None => c.lines[row][0] = 0,
                },
                12 => c.field(12, b"BACK", 16, if c.mission.button == 0 { BUTTON } else { WHITE }),
                _ => c.lines[row][0] = 0,
            }
        }
        for l in 0..SCREEN_LINES as i32 {
            self.register_line(id, c, l);
        }
    }

    /// computer_prompt_file: the files of the computer to pick from (16 a page, then CANCEL), or the upload's
    /// progress bar.
    fn mission_upload(&mut self, id: usize, c: &mut Computer) {
        let vol = self.items.get(id).map_or(-1, |i| i.volume);
        if c.mission.timer <= 0 {
            for row in 0..SCREEN_LINES {
                if row == 0 {
                    c.field(0, b"UPLOAD FILE", 20, TITLE);
                    continue;
                }
                let r = row as i32 - 4;
                if !(0..PAGE).contains(&r) {
                    c.lines[row][0] = 0;
                    continue;
                }
                let k = r + (c.mission.file & !(PAGE - 1));
                let count = c.mission.files.len() as i32;
                let color = if k == c.mission.file { BUTTON } else { WHITE };
                if k < count {
                    let f = c.mission.files[k as usize];
                    let name = self.fs.volume(vol).and_then(|v| v.files.get(f as usize)).map_or(String::new(), |f| f.name.clone());
                    c.field(row, format!(" {name}").as_bytes(), 16, color);
                } else if k == count {
                    c.field(row, b" CANCEL", 16, color);
                } else {
                    c.lines[row][0] = 0;
                }
            }
        } else {
            for row in 0..SCREEN_LINES {
                match row {
                    0 => c.field(0, b"UPLOADING", 16, TITLE),
                    1 => {
                        let done = (0xff - c.mission.timer) >> 4;
                        let bar: Vec<u8> = (0..16).map(|k| if done < k { b' ' } else { 0x7f }).collect();
                        c.field(1, &bar, 16, TITLE);
                    }
                    _ => c.lines[row][0] = 0,
                }
            }
        }
        for l in 0..SCREEN_LINES as i32 {
            self.register_line(id, c, l);
        }
    }

    /// Back to the shell with the screen blanked.
    fn mission_exit(&mut self, id: usize, c: &mut Computer) {
        c.mode = Mode::Shell;
        for l in 0..SCREEN_LINES {
            c.lines[l][0] = 0;
            c.colors[l][..63].fill(WHITE);
            self.register_line(id, c, l as i32);
        }
    }

    /// Redraws the page now open.
    fn mission_redraw(&mut self, id: usize, c: &mut Computer, menu_first: bool) {
        if menu_first && c.mission.page == 0 {
            self.mission_menu(id, c);
        }
        if c.mission.page == 1 {
            self.mission_page(id, c);
        }
        if c.mission.page == 2 {
            self.mission_upload(id, c);
        }
    }

    /// computer_mission_handle_key.
    pub(crate) fn mission_key(&mut self, id: usize, c: &mut Computer, key: i32) {
        let vol = self.items.get(id).map_or(-1, |i| i.volume);
        let Some(team) = usize::try_from(c.team).ok().filter(|_| vol != -1) else { return self.mission_exit(id, c) };
        if key & 0x100 != 0 {
            if (key as u8).to_ascii_uppercase() == b'X' {
                self.mission_exit(id, c);
            }
            return;
        }
        let s = &mut c.mission;
        match s.page {
            0 => match key {
                KEY_UP => {
                    if s.selected >= -1 {
                        s.selected -= 1;
                    }
                }
                KEY_DOWN => {
                    if s.selected <= 6 {
                        s.selected += 1;
                    }
                }
                KEY_ENTER => match s.selected {
                    UPLOAD_ROW => {
                        let dir = s.dir;
                        s.files = self.fs.volume(vol).map_or(Vec::new(), |v| v.files.iter().enumerate().filter(|(_, f)| f.dir == dir).map(|(k, _)| k as i32).collect());
                        s.page = 2;
                        s.button = 0;
                        return self.mission_upload(id, c);
                    }
                    INTEL_ROW => {
                        if self.corp_state[team].intel <= INTEL_ASKABLE {
                            self.corp_state[team].intel = 0;
                        }
                    }
                    0..MISSION_ROWS => {
                        s.page = 1;
                        s.button = 0;
                        return self.mission_redraw(id, c, false);
                    }
                    _ => return self.mission_exit(id, c),
                },
                _ => {}
            },
            1 => {
                match key {
                    KEY_UP => {
                        if s.button > 0 {
                            s.button -= 1;
                        }
                    }
                    KEY_DOWN => {
                        if s.button < 0 {
                            s.button += 1;
                        }
                    }
                    KEY_ENTER if s.button == 0 => s.page = 0,
                    _ => {}
                }
                if s.page == 1 {
                    return self.mission_page(id, c);
                }
            }
            _ => {
                match key {
                    KEY_UP => {
                        if s.file > 0 {
                            s.file -= 1;
                        }
                    }
                    KEY_DOWN => {
                        if s.file < s.files.len() as i32 {
                            s.file += 1;
                        }
                    }
                    KEY_ENTER => {
                        if s.file >= s.files.len() as i32 {
                            s.page = 0;
                        } else {
                            s.kind = 1;
                            s.timer = UPLOAD_TICKS;
                        }
                    }
                    KEY_BACK => s.page = 0,
                    _ => {}
                }
                if s.page == 2 {
                    return self.mission_upload(id, c);
                }
            }
        }
        self.mission_redraw(id, c, true);
    }

    /// computer_mission_tick: the connection counts down (the upload's bar every 16 ticks); a finished upload of a
    /// mission's document settles it for the corporation; otherwise the page is redrawn every 16 ticks.
    pub(crate) fn mission_tick(&mut self, id: usize, c: &mut Computer) {
        let vol = self.items.get(id).map_or(-1, |i| i.volume);
        let Some(team) = usize::try_from(c.team).ok().filter(|_| vol != -1) else { return };
        if c.mission.timer <= 0 {
            if (self.tick ^ id as u32) & 0xf != 0 {
                return;
            }
            return self.mission_redraw(id, c, true);
        }
        c.mission.timer -= 1;
        if c.mission.kind == 1 {
            if c.mission.timer & 0xf != 0 {
                return;
            }
            self.mission_upload(id, c);
        }
        if c.mission.timer != 0 {
            return;
        }
        match c.mission.kind {
            0 => self.mission_menu(id, c),
            1 => {
                c.mission.page = 0;
                let f = c.mission.files.get(c.mission.file as usize).copied().unwrap_or(0);
                let data = self.fs.volume(vol).and_then(|v| v.files.get(f as usize)).map_or(-1, |f| f.data);
                for m in 0..MISSION_ROWS as usize {
                    if self.world_missions.missions.get(m).is_some_and(|x| x.document == data) {
                        if !self.world_missions.missions[m].teams[team].uploaded {
                            self.file_trade(m, team);
                        }
                        c.mission.selected = m as i32;
                        c.mission.page = 1;
                        c.mission.button = 0;
                    }
                }
                self.mission_redraw(id, c, true);
            }
            _ => {}
        }
    }
}
