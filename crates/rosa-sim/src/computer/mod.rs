pub mod decrypt;
pub mod fs;
pub mod links;
pub mod mission;
pub mod shell;
pub mod snake;

pub const LINES: usize = 32;
pub const COLUMNS: usize = 64;
/// The 24 lines a program draws on.
pub const SCREEN_LINES: usize = 24;
pub const WHITE: u8 = 0xf;
/// The drives: A is the disk in the computer, C the computer's own volume; B is never ready.
pub const DRIVE_A: i32 = 0;
pub const DRIVE_C: i32 = 2;
pub const DRIVES: usize = 3;

/// What the computer is running (item +0x165c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum Mode {
    #[default]
    Shell = 0,
    Decrypt = 1,
    Mission = 2,
    Snake = 3,
}

/// A drive's current directory: the path from the root (item +0x13f8 + drive * 0x48: +4 depth, +8 on the entries,
/// entry 0 being the root).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DirStack {
    pub depth: i32,
    pub path: [i32; 17],
}

impl DirStack {
    pub fn current(&self) -> i32 {
        self.path.get(self.depth as usize).copied().unwrap_or(0)
    }
}

/// DIR or COPY running in the background (item +0x1510): which (+0x1518, 1 DIR, 0 COPY), ticks to the next step
/// (+0x151c), the drive and directory (+0x1520, +0x1524), how far DIR has listed (+0x1528 directories, +0x152c
/// files), the file COPY makes (+0x1530 name, +0x1550 content) and the keys typed meanwhile (+0x1554 count, 64 kept).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiskOp {
    pub kind: i32,
    pub timer: i32,
    pub drive: i32,
    pub dir: i32,
    pub next_dir: i32,
    pub next_file: i32,
    pub name: String,
    pub data: i32,
    pub keys: Vec<i32>,
}

pub const DISK_OP_COPY: i32 = 0;
pub const DISK_OP_DIR: i32 = 1;

/// A computer's screen and shell (the computer fields of an item record). The screen is a ring of 32 lines of 63
/// characters and colours; `top_line` is the first one shown and `current_line` the one being typed on.
#[derive(Clone, Debug, PartialEq)]
pub struct Computer {
    pub current_line: i32,
    pub top_line: i32,
    /// The cursor's line * 64 + column (item +0x370), -1 hidden.
    pub cursor: i32,
    pub lines: [[u8; COLUMNS]; LINES],
    pub colors: [[u8; COLUMNS]; LINES],
    pub prompt: [u8; COLUMNS],
    pub input: [u8; COLUMNS],
    pub drive: i32,
    pub dirs: [DirStack; DRIVES],
    pub op: DiskOp,
    /// The corporation whose base the computer is in (item +0x1658), -1 for none.
    pub team: i32,
    pub mode: Mode,
    pub decrypt: decrypt::Decrypter,
    pub snake: snake::Snake,
    pub mission: mission::MissionScreen,
}

impl Default for Computer {
    fn default() -> Self {
        Self {
            current_line: 0,
            top_line: 0,
            cursor: 0,
            lines: [[0; COLUMNS]; LINES],
            colors: [[0; COLUMNS]; LINES],
            prompt: [0; COLUMNS],
            input: [0; COLUMNS],
            drive: 0,
            dirs: [DirStack::default(); DRIVES],
            op: DiskOp::default(),
            team: 0,
            mode: Mode::Shell,
            decrypt: decrypt::Decrypter::default(),
            snake: snake::Snake::default(),
            mission: mission::MissionScreen::default(),
        }
    }
}

/// strlen within a line.
pub fn text_len(s: &[u8]) -> usize {
    s.iter().position(|&c| c == 0).unwrap_or(s.len())
}

/// strcpy into a line, cut at its end.
pub fn put_text(line: &mut [u8; COLUMNS], s: &[u8]) {
    let n = s.len().min(COLUMNS - 1);
    line[..n].copy_from_slice(&s[..n]);
    line[n] = 0;
}

impl Computer {
    pub fn line_mut(&mut self) -> &mut [u8; COLUMNS] {
        &mut self.lines[self.current_line as usize & (LINES - 1)]
    }

    /// Writes `text` across the first 62 columns of line `l`, the rest spaces, in `color`, ended at column 62.
    pub fn banner(&mut self, l: usize, text: &[u8], color: u8) {
        let n = text_len(text);
        for k in 0..62 {
            self.lines[l][k] = if k < n { text[k] } else { b' ' };
            self.colors[l][k] = color;
        }
        self.lines[l][62] = 0;
    }
}
