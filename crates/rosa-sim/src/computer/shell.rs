use super::{COLUMNS, Computer, DISK_OP_COPY, DISK_OP_DIR, DRIVE_A, DRIVE_C, LINES, Mode, WHITE, fs, put_text, text_len};
use crate::sim::Sim;

/// The last column the shell reads and types into.
const LAST: i32 = 0x3d;
const INPUT_LEN: usize = 0x3e;
const KEY_ENTER: u8 = 0xa;
const KEY_BACKSPACE: i32 = 8;
const MAX_KEPT_KEYS: usize = 64;
/// DIR prints an entry every 4 ticks; COPY takes 256.
const DIR_STEP: i32 = 4;
const COPY_TICKS: i32 = 256;
const MAX_NAME: usize = 12;
const MAX_PATH_DEPTH: usize = 8;
/// The rows below the two the computer starts with.
const BOOT_LINES: std::ops::Range<i32> = 2..24;

fn is_alpha(c: u8) -> bool {
    (c & 0xdf).wrapping_sub(b'A') <= 25
}

fn is_digit(c: u8) -> bool {
    c.wrapping_sub(b'0') <= 9
}

fn at(s: &[u8], k: i32) -> u8 {
    usize::try_from(k).ok().and_then(|k| s.get(k)).copied().unwrap_or(0)
}

/// computer_parse_token / computer_parse_filename_token: past anything else, a run of letters and digits (and dots
/// for a file name). Returns the token, empty when there is none.
pub fn parse_token(s: &[u8], pos: &mut i32, dots: bool) -> Vec<u8> {
    if *pos > LAST {
        return Vec::new();
    }
    let mut c = at(s, *pos);
    if !is_alpha(c) && !is_digit(c) && c != 0 {
        let mut k = *pos + 1;
        loop {
            *pos = k;
            if k == LAST + 1 {
                return Vec::new();
            }
            c = at(s, k);
            if is_alpha(c) {
                break;
            }
            k += 1;
            if is_digit(c) || c == 0 {
                break;
            }
        }
    }
    let mut out = Vec::new();
    loop {
        if !is_alpha(c) && !is_digit(c) && !(dots && c == b'.') {
            return out;
        }
        out.push(c);
        *pos += 1;
        if *pos > LAST {
            return out;
        }
        c = at(s, *pos);
    }
}

/// computer_parse_symbol_token: past spaces, the next one or two characters when they are not a letter or digit.
pub fn parse_symbol(s: &[u8], pos: &mut i32) -> Option<Vec<u8>> {
    if *pos <= LAST && at(s, *pos) == b' ' {
        loop {
            *pos += 1;
            if *pos == LAST + 1 || at(s, *pos) != b' ' {
                break;
            }
        }
    }
    let c = at(s, *pos);
    if is_alpha(c) || is_digit(c) || c == 0 || *pos > LAST {
        return None;
    }
    let mut out = vec![c];
    *pos += 1;
    if *pos > LAST || at(s, *pos) == 0 {
        return Some(out);
    }
    out.push(at(s, *pos));
    *pos += 1;
    Some(out)
}

impl Sim {
    /// Runs `f` on item `id`'s computer, taken out of the item meanwhile.
    pub(crate) fn with_computer<R>(&mut self, id: usize, f: impl FnOnce(&mut Self, &mut Computer) -> R) -> Option<R> {
        let item = self.items.get_mut(id)?;
        let crate::sim::item_state::ItemState::Computer(_) = item.state else { return None };
        let crate::sim::item_state::ItemState::Computer(mut c) = std::mem::replace(&mut item.state, crate::sim::item_state::ItemState::Plain) else { return None };
        let r = f(self, &mut c);
        if let Some(i) = self.items.get_mut(id) {
            i.state = crate::sim::item_state::ItemState::Computer(c);
        }
        Some(r)
    }

    /// A key pressed on the computer in a player's hand.
    pub fn computer_keypress(&mut self, id: usize, key: i32) {
        self.with_computer(id, |sim, c| sim.computer_key(id, c, key));
    }

    /// The volume a drive reads: C the computer's own, A the disk in it.
    pub(crate) fn drive_volume(&self, id: usize, drive: i32) -> i32 {
        match drive {
            DRIVE_C => self.items.get(id).map_or(-1, |i| i.volume),
            DRIVE_A => self.disk_volume(id),
            _ => -1,
        }
    }

    /// computer_get_disk_fs_slot: the volume of the disk in the computer.
    pub(crate) fn disk_volume(&self, id: usize) -> i32 {
        self.items.get(id).and_then(|i| i.children.first()).and_then(|&d| self.items.get(d)).map_or(-1, |d| d.volume)
    }

    /// create_computer: boots into the shell on drive C.
    pub(crate) fn create_computer(&mut self, id: usize) {
        let mut c = Box::new(Computer { drive: DRIVE_C, team: -1, ..Default::default() });
        put_text(c.line_mut(), b"STARTING CS-DOS...");
        self.commit_line(id, &mut c);
        self.commit_line(id, &mut c);
        self.computer_key(id, &mut c, 0);
        for l in BOOT_LINES {
            self.register_line(id, &c, l);
        }
        for colors in &mut c.colors {
            colors[..COLUMNS - 1].fill(WHITE);
        }
        if let Some(i) = self.items.get_mut(id) {
            i.state = crate::sim::item_state::ItemState::Computer(c);
        }
    }

    /// computer_commit_line: the line is done; the screen scrolls when it reaches 10 lines above the top.
    pub(crate) fn commit_line(&mut self, id: usize, c: &mut Computer) {
        let cur = c.current_line as usize & (LINES - 1);
        c.colors[cur][..COLUMNS - 1].fill(WHITE);
        self.register_line(id, c, c.current_line);
        if c.top_line == (c.current_line + 10) & 31 {
            c.top_line = (c.top_line + 1) & 31;
        }
        c.current_line = (c.current_line + 1) & 31;
        c.cursor = c.current_line << 6;
        if let Some(item) = self.items.get_mut(id) {
            item.events += 1;
        }
    }

    /// Writes a message on the current line and commits it.
    fn say(&mut self, id: usize, c: &mut Computer, text: &[u8]) {
        put_text(c.line_mut(), text);
        self.commit_line(id, c);
    }

    /// computer_handle_keypress: a key goes to the running program; the shell keeps it for later while DIR or COPY
    /// runs, runs the line on enter and redraws the prompt.
    pub(crate) fn computer_key(&mut self, id: usize, c: &mut Computer, key: i32) {
        let mut key = key;
        if c.mode == Mode::Decrypt {
            c.cursor = -1;
            self.decrypt_key(id, c, key);
            if c.mode == Mode::Decrypt {
                return;
            }
            key = KEY_BACKSPACE;
        }
        if c.mode == Mode::Mission {
            c.cursor = -1;
            if c.mission.timer <= 0 {
                self.mission_key(id, c, key);
            }
            if c.mode == Mode::Mission {
                return;
            }
            key = KEY_BACKSPACE;
        }
        if c.mode == Mode::Snake {
            c.cursor = -1;
            self.snake_key(id, c, key);
            if c.mode == Mode::Snake {
                return;
            }
            key = KEY_BACKSPACE;
        }
        let b = key as u8;
        let len = text_len(&c.input[..INPUT_LEN]);
        if c.op.timer > 0 {
            if c.op.keys.len() < MAX_KEPT_KEYS {
                c.op.keys.push(b as i32);
            }
            return;
        }
        if b == KEY_ENTER {
            self.commit_line(id, c);
            self.execute_command(id, c);
            c.input[0] = 0;
            if c.op.timer > 0 || c.mode != Mode::Shell {
                return;
            }
        } else if b == KEY_BACKSPACE as u8 {
            if len > 0 {
                c.input[len - 1] = 0;
            }
        } else if len != INPUT_LEN {
            c.input[len] = b;
            c.input[len + 1] = 0;
        }
        self.draw_prompt(id, c);
    }

    /// The prompt (drive and path, then '>') and what has been typed, on the current line.
    fn draw_prompt(&mut self, id: usize, c: &mut Computer) {
        let vol = match c.drive {
            DRIVE_C => self.items.get(id).map_or(-1, |i| i.volume),
            DRIVE_A if self.items.get(id).is_some_and(|i| !i.children.is_empty()) => self.disk_volume(id),
            _ => 0,
        };
        let stack = c.dirs[c.drive as usize];
        let mut p = format!("{}:\\", (c.drive as u8 + b'A') as char).into_bytes();
        for k in 1..=stack.depth {
            let name = self.fs.volume(vol).and_then(|v| v.dirs.get(stack.path[k as usize] as usize)).map_or("", |d| d.name.as_str());
            p.extend_from_slice(name.as_bytes());
            if stack.depth > k {
                p.push(b'\\');
            }
        }
        p.push(b'>');
        put_text(&mut c.prompt, &p);
        let cur = c.current_line as usize & (LINES - 1);
        let pl = text_len(&c.prompt) + 1;
        c.lines[cur][..pl].copy_from_slice(&c.prompt[..pl]);
        let mut n = text_len(&c.lines[cur]);
        if n <= LAST as usize {
            for &ch in c.input.iter().take_while(|&&ch| ch != 0) {
                c.lines[cur][n] = ch;
                n += 1;
                if n == INPUT_LEN {
                    break;
                }
            }
        }
        c.lines[cur][n] = 0;
        c.cursor = n as i32 + (c.current_line << 6);
        c.colors[cur][..COLUMNS - 1].fill(WHITE);
        self.register_line(id, c, c.current_line);
    }

    /// computer_resolve_path: walks the path at `pos` (a drive letter, a leading '\', '.' and '..') from the drive's
    /// current directory. 1 when it ends on a directory, 0 when a name is not one (`pos` left at it), -1 on error.
    fn resolve_path(&self, id: usize, c: &Computer, drive: &mut i32, dir: &mut i32, s: &[u8], pos: &mut i32) -> i32 {
        let (mut r13, mut rbx) = (*pos, *pos);
        let mut ch = at(s, r13);
        if r13 <= LAST && ch == b' ' {
            r13 += 1;
            loop {
                rbx += 1;
                ch = at(s, r13);
                if rbx == LAST + 1 || ch != b' ' {
                    break;
                }
                r13 += 1;
            }
        }
        let up = ch.to_ascii_uppercase();
        if up.wrapping_sub(b'A') <= 25 && at(s, r13 + 1) == b':' {
            rbx += 2;
            *drive = (up - b'A') as i32;
        }
        *pos = rbx;
        let vol = match *drive {
            DRIVE_C | DRIVE_A => self.drive_volume(id, *drive),
            _ => return -1,
        };
        if vol == -1 {
            return -1;
        }
        *dir = c.dirs[*drive as usize].current();
        if at(s, rbx) == b'\\' {
            rbx += 1;
            *dir = 0;
        }
        loop {
            *pos = rbx;
            let mut dl = at(s, rbx);
            if dl == b'.' {
                let mut eax = rbx + 1;
                dl = at(s, eax);
                if dl == b'.' {
                    let n = at(s, eax + 1);
                    if n != b'\\' && n != 0 {
                        return -1;
                    }
                    *dir = self.fs.volume(vol).and_then(|v| v.dirs.get(*dir as usize)).map_or(0, |d| d.parent);
                    eax = rbx + 2;
                    dl = at(s, eax);
                }
                if dl == b'\\' {
                    rbx = eax + 1;
                    continue;
                }
                if eax > LAST {
                    return 1;
                }
                rbx = eax;
            } else if rbx > LAST {
                return 1;
            }
            if dl & 0xdf == 0 || dl == b'\\' {
                return 1;
            }
            let start = rbx;
            let mut name = Vec::new();
            let mut k = 1;
            let after = loop {
                name.push(dl);
                if start + k == LAST + 1 {
                    break start + k;
                }
                dl = at(s, start + k);
                if dl & 0xdf == 0 || dl == b'\\' {
                    break start + k;
                }
                k += 1;
                if k as usize == MAX_NAME + 1 {
                    return -1;
                }
            };
            let found = self.fs.find_dir(vol, *dir, std::str::from_utf8(&name).unwrap_or(""));
            if found == -1 {
                return 0;
            }
            *dir = found;
            if at(s, after) != b'\\' {
                return 1;
            }
            rbx = after + 1;
        }
    }

    /// computer_set_current_dir: the drive's path becomes the directory's ancestry (8 deep at most).
    fn set_current_dir(&self, id: usize, c: &mut Computer, drive: i32, dir: i32) {
        let vol = self.drive_volume(id, drive);
        if vol == -1 {
            return;
        }
        let stack = &mut c.dirs[drive as usize];
        stack.depth = 0;
        if dir == 0 {
            return;
        }
        let mut chain = vec![dir];
        let mut d = dir;
        while chain.len() < MAX_PATH_DEPTH {
            d = self.fs.volume(vol).and_then(|v| v.dirs.get(d as usize)).map_or(0, |x| x.parent);
            if d == 0 {
                break;
            }
            chain.push(d);
        }
        for &x in chain.iter().rev() {
            stack.depth += 1;
            stack.path[stack.depth as usize] = x;
        }
    }

    /// computer_build_path_string: the drive and its current path.
    fn path_string(&self, id: usize, c: &Computer, drive: i32) -> String {
        let vol = match drive {
            DRIVE_C => self.items.get(id).map_or(-1, |i| i.volume),
            DRIVE_A => self.disk_volume(id),
            _ => -1,
        };
        if vol == -1 {
            return String::new();
        }
        let stack = c.dirs[drive as usize];
        let mut s = format!("{}:\\", (drive as u8 + b'A') as char);
        for k in 1..=stack.depth {
            s += self.fs.volume(vol).and_then(|v| v.dirs.get(stack.path[k as usize] as usize)).map_or("", |d| d.name.as_str());
            if stack.depth > k {
                s.push('\\');
            }
        }
        s
    }

    /// computer_execute_command: the typed line.
    fn execute_command(&mut self, id: usize, c: &mut Computer) {
        let s = c.input;
        let drive = c.drive;
        let mut pos = 0;
        let vol = self.drive_volume(id, drive);
        let token = parse_token(&s, &mut pos, false);
        if drive != DRIVE_A && drive != DRIVE_C && token.len() > 1 {
            return self.say(id, c, b"DRIVE NOT READY");
        }
        if token.len() <= 1 {
            let Some(sym) = parse_symbol(&s, &mut pos) else { return };
            if sym != b":" {
                return;
            }
            let letter = token.first().copied().unwrap_or(0);
            let d = letter.wrapping_sub(b'A') as i32;
            if !(0..=2).contains(&d) || (d != DRIVE_A && d != DRIVE_C) || self.drive_volume(id, d) == -1 {
                return self.say(id, c, b"DRIVE NOT READY");
            }
            c.drive = d;
            return;
        }
        if vol == -1 {
            return self.say(id, c, b"DRIVE NOT READY");
        }
        let cwd = c.dirs[drive as usize].current();
        match token.as_slice() {
            b"CD" => {
                let (mut d, mut dir) = (drive, cwd);
                if self.resolve_path(id, c, &mut d, &mut dir, &s, &mut pos) == 1 {
                    self.set_current_dir(id, c, d, dir);
                } else {
                    self.say(id, c, b"INVALID DIRECTORY");
                }
            }
            b"DIR" => {
                c.op = super::DiskOp { kind: DISK_OP_DIR, timer: DIR_STEP, drive, dir: cwd, next_dir: 1, next_file: 0, name: c.op.name.clone(), data: c.op.data, keys: Vec::new() };
                c.line_mut()[0] = 0;
                self.commit_line(id, c);
                let path = self.path_string(id, c, drive);
                self.say(id, c, format!("Directory of {path}").as_bytes());
                c.line_mut()[0] = 0;
            }
            b"COPY" => self.copy_command(id, c, &s, &mut pos, cwd),
            b"MISSION" | b"MISSION.EXE" => {
                if c.team == -1 {
                    return self.say(id, c, b"BAD COMMAND OR FILE NAME");
                }
                c.mode = Mode::Mission;
                c.cursor = -1;
                self.mission_init(id, c);
            }
            b"DECRYPT" | b"DECRYPT.EXE" => {
                let (mut d, mut dir) = (drive, cwd);
                let file = (self.resolve_path(id, c, &mut d, &mut dir, &s, &mut pos) == 0).then(|| self.drive_volume(id, d)).filter(|&v| v != -1).and_then(|v| {
                    let name = parse_token(&s, &mut pos, true);
                    if name.is_empty() {
                        return None;
                    }
                    let name = String::from_utf8_lossy(&name).into_owned();
                    self.fs.volume(v)?.files.iter().rposition(|f| f.dir == dir && f.name == name).map(|k| self.fs.volume(v).unwrap().files[k].data)
                });
                let Some(data) = file else { return self.say(id, c, b"USAGE: DECRYPT \"filename\"") };
                if !self.fs.content(data).is_some_and(|x| x.encrypted) {
                    return self.say(id, c, b"INCORRECT FILE TYPE");
                }
                c.mode = Mode::Decrypt;
                c.cursor = -1;
                c.decrypt.content = data;
                self.decrypt_init(id, c);
            }
            b"SNAKE" | b"SNAKE.EXE" => {
                c.mode = Mode::Snake;
                c.cursor = -1;
                self.snake_init(id, c);
            }
            _ => {
                if s[0] != 0 {
                    self.say(id, c, b"BAD COMMAND OR FILE NAME");
                }
            }
        }
    }

    /// COPY: the named file to the directory given after it, in the background; a small volume takes 3 files.
    fn copy_command(&mut self, id: usize, c: &mut Computer, s: &[u8; COLUMNS], pos: &mut i32, cwd: i32) {
        const NO_PATH: &[u8] = b"THE SYSTEM CANNOT FIND THE PATH SPECIFIED";
        const NO_FILE: &[u8] = b"THE SYSTEM CANNOT FIND THE FILE SPECIFIED";
        let (mut d, mut dir) = (c.drive, cwd);
        if self.resolve_path(id, c, &mut d, &mut dir, s, pos) != 0 {
            return self.say(id, c, NO_PATH);
        }
        let src = self.drive_volume(id, d);
        if src == -1 {
            return self.say(id, c, NO_FILE);
        }
        let name = parse_token(s, pos, true);
        if name.is_empty() {
            return self.say(id, c, NO_FILE);
        }
        let name = String::from_utf8_lossy(&name).into_owned();
        let Some(data) = self.fs.volume(src).and_then(|v| v.files.iter().rev().find(|f| f.dir == dir && f.name == name)).map(|f| f.data) else {
            return self.say(id, c, NO_FILE);
        };
        let (mut d2, mut dir2) = (c.drive, c.dirs[c.drive as usize].current());
        if self.resolve_path(id, c, &mut d2, &mut dir2, s, pos) != 1 {
            return self.say(id, c, NO_PATH);
        }
        let dst = self.drive_volume(id, d2);
        let Some(v) = self.fs.volume(dst) else { return };
        if self.fs.find_file(dst, dir2, &name) != -1 {
            return self.say(id, c, b"FILE ALREADY EXISTS");
        }
        if !(v.capacity > fs::SMALL_VOLUME || v.files.len() <= fs::SMALL_VOLUME_FILES) {
            return self.say(id, c, b"INSUFFICIENT DISK SPACE");
        }
        c.op = super::DiskOp { kind: DISK_OP_COPY, timer: COPY_TICKS, drive: d2, dir: dir2, next_dir: c.op.next_dir, next_file: c.op.next_file, name: name.chars().take(31).collect(), data, keys: Vec::new() };
        c.line_mut()[0] = 0;
        self.phone_sound(rosa_protocol::clientbound::game::events::sound::Sound::Floppy, id, 1.0);
    }

    /// computer_run_disk_operation: the next step of DIR, or the end of COPY; when done the shell takes the keys
    /// typed meanwhile.
    fn run_disk_operation(&mut self, id: usize, c: &mut Computer) {
        if c.op.kind == DISK_OP_COPY {
            let vol = self.drive_volume(id, c.op.drive);
            if vol == -1 {
                self.say(id, c, b"DISK ERROR");
            } else {
                let (dir, name, data) = (c.op.dir, c.op.name.clone(), c.op.data);
                self.fs.add_file(vol, dir, &name, data);
                self.say(id, c, b"  1 FILE(S) COPIED.");
            }
            self.replay_keys(id, c);
        }
        if c.op.kind != DISK_OP_DIR {
            return;
        }
        let vol = self.drive_volume(id, c.op.drive);
        match self.fs.volume(vol).cloned() {
            Some(v) => {
                while (c.op.next_dir as usize) < v.dirs.len() {
                    let d = &v.dirs[c.op.next_dir as usize];
                    if d.parent == c.op.dir {
                        let mut line = format!("{}            ", d.name).into_bytes();
                        line.truncate(11);
                        line.extend_from_slice(b"<DIR>");
                        self.say(id, c, &line);
                        c.line_mut()[0] = 0;
                        c.op.timer = DIR_STEP;
                        c.op.next_dir += 1;
                        return;
                    }
                    c.op.next_dir += 1;
                }
                while (c.op.next_file as usize) < v.files.len() {
                    let f = &v.files[c.op.next_file as usize];
                    if f.dir == c.op.dir {
                        let name = f.name.clone();
                        self.say(id, c, name.as_bytes());
                        c.line_mut()[0] = 0;
                        c.op.next_file += 1;
                        c.op.timer = DIR_STEP;
                        return;
                    }
                    c.op.next_file += 1;
                }
            }
            None => self.say(id, c, b"DISK ERROR"),
        }
        if c.op.timer != 0 {
            return;
        }
        self.commit_line(id, c);
        c.line_mut()[0] = 0;
        c.input[0] = 0;
        self.replay_keys(id, c);
    }

    /// The keys typed while DIR or COPY ran, or a backspace to redraw the prompt.
    fn replay_keys(&mut self, id: usize, c: &mut Computer) {
        let keys = std::mem::take(&mut c.op.keys);
        if keys.is_empty() {
            self.computer_key(id, c, KEY_BACKSPACE);
        }
        for k in keys {
            self.computer_key(id, c, k);
        }
        c.op.keys.clear();
    }

    /// logic_computer: the background disk work, the decrypter's reveal timer and the running program's tick.
    pub(crate) fn logic_computer(&mut self, id: usize, c: &mut Computer) {
        if c.mode == Mode::Shell && c.op.timer > 0 {
            c.op.timer -= 1;
            if c.op.timer == 0 {
                self.run_disk_operation(id, c);
            }
        }
        if c.mode == Mode::Decrypt {
            self.decrypt_tick(id, c);
        }
        if c.mode == Mode::Mission {
            self.mission_tick(id, c);
        }
        if c.mode == Mode::Snake {
            self.snake_tick(id, c);
        }
    }
}

impl Sim {
    pub fn computer(&self, id: usize) -> Option<&Computer> {
        match &self.items.get(id)?.state {
            crate::sim::item_state::ItemState::Computer(c) => Some(c),
            _ => None,
        }
    }

    pub fn file_system(&self) -> &fs::FileSystem {
        &self.fs
    }

    pub fn file_system_mut(&mut self) -> &mut fs::FileSystem {
        &mut self.fs
    }

    pub fn line_links(&self) -> &crate::computer::links::LinkPool {
        &self.links
    }

    pub fn set_item_volume(&mut self, id: usize, volume: i32) {
        if let Some(i) = self.items.get_mut(id) {
            i.volume = volume;
        }
    }

    pub fn insert_item_in(&mut self, parent: usize, child: usize) -> bool {
        crate::sim::items::attach_child(&mut self.items, &self.item_types, parent, child)
    }

    pub fn run_logic_computer(&mut self, id: usize) {
        self.with_computer(id, |sim, c| sim.logic_computer(id, c));
    }
}
