use super::{COLUMNS, Computer, Mode, SCREEN_LINES, WHITE};
use crate::sim::Sim;

/// The text is wrapped into rows of 31 to 60 characters, 8 shown in pairs (guessed above, enciphered below).
const ROWS: usize = 16;
const SHOWN_ROWS: usize = 8;
const DRAWN_ROWS: std::ops::Range<usize> = 2..17;
const WRAP_MIN: i32 = 0x1e;
const WRAP_MAX: i32 = 0x3b;
const MAX_TEXT: usize = 0x200;
const ROW_COLUMNS: usize = 0x3e;
const HEADER: &[u8] = b"DECRYPTER     Ctrl-R RESET   Ctrl-P PRINT    Ctrl-X EXIT";
const HEADER_COLOR: u8 = 0x1f;
const SELECTED: u8 = 0xc;
const COUNT_COLOR: u8 = 0x8f;
/// The letter key table: A to O on lines 17 and 18, P to Z on lines 20 and 21.
const FIRST_KEYS: std::ops::Range<usize> = 0..15;
const LAST_KEYS: std::ops::Range<usize> = 15..26;
const AUTO_TICKS: f32 = 600.0;
const AUTO_MIN: f32 = 20.0;
const REVEAL_TRIES: i32 = 1000;
const CTRL: i32 = 0x100;
const STATUS_LINE: usize = 1;

/// DECRYPT's state (item +0x1660 on): the wrapped plain text, the file's content, its cipher and the inverse, how
/// often each enciphered letter appears, the player's guess for each, the letter picked and the auto reveal timer.
#[derive(Clone, Debug, PartialEq)]
pub struct Decrypter {
    pub rows: [[u8; COLUMNS]; ROWS],
    pub content: i32,
    pub cipher: [i32; 26],
    pub inverse: [i32; 26],
    pub counts: [i32; 26],
    pub guesses: [i32; 26],
    pub selected: i32,
    pub timer: i32,
}

impl Default for Decrypter {
    fn default() -> Self {
        Self { rows: [[0; COLUMNS]; ROWS], content: 0, cipher: [0; 26], inverse: [0; 26], counts: [0; 26], guesses: [0; 26], selected: 0, timer: 0 }
    }
}

/// The wrap width for text starting at `from`: up to 60, backed off to a space but not below 30.
fn wrap(s: &[u8], from: i32, left: i32) -> i32 {
    let at = |k: i32| s.get(k as usize).copied().unwrap_or(0);
    let mut w = if left > 0x3a { WRAP_MAX } else { left };
    if !(left > 0x3a || w > WRAP_MIN) || at(from + w) == b' ' {
        return w;
    }
    loop {
        w -= 1;
        if w == WRAP_MIN || at(from + w) == b' ' {
            return w;
        }
    }
}

fn is_letter(c: u8) -> bool {
    c.wrapping_sub(b'A') <= 25
}

impl Sim {
    /// computer_decrypt_init: the file's text, wrapped and in capitals, its letters counted.
    pub(crate) fn decrypt_init(&mut self, id: usize, c: &mut Computer) {
        let (text, cipher) = self.fs.content(c.decrypt.content).map_or((Vec::new(), [0; 26]), |x| (x.text.clone(), x.cipher));
        let s: Vec<u8> = text.iter().copied().take_while(|&b| b != 0).take(MAX_TEXT).collect();
        let len = s.len() as i32;
        let d = &mut c.decrypt;
        for row in d.rows.iter_mut().take(SHOWN_ROWS) {
            row[0] = 0;
        }
        d.timer = 0;
        // TODO: past 8 rows the binary writes on over the content slot and cipher (item +0x1860 on)
        let mut w = wrap(&s, 0, len);
        let (mut row, mut col) = (0usize, 0usize);
        let mut k = 1i32;
        while k <= 0x100 {
            let ch = s.get(k as usize - 1).copied().unwrap_or(0);
            if ch == 0 {
                break;
            }
            if row < ROWS {
                d.rows[row][col] = ch.to_ascii_uppercase();
            }
            col += 1;
            if col as i32 <= w {
                k += 1;
                continue;
            }
            if row < ROWS {
                d.rows[row][col.min(COLUMNS - 1)] = 0;
            }
            row += 1;
            w = wrap(&s, k, len - k);
            k += 1;
            col = 0;
        }
        if row < ROWS {
            d.rows[row][col.min(COLUMNS - 1)] = 0;
        }
        d.cipher = cipher;
        d.selected = -1;
        for i in 0..26 {
            d.inverse[d.cipher[i].rem_euclid(26) as usize] = i as i32;
            d.counts[i] = 0;
            d.guesses[i] = -1;
        }
        for r in 0..SHOWN_ROWS {
            for k in 0..ROW_COLUMNS {
                let ch = d.rows[r][k];
                if ch == 0 {
                    break;
                }
                let up = ch.to_ascii_uppercase();
                d.rows[r][k] = up;
                if is_letter(up) {
                    d.counts[d.cipher[(up - b'A') as usize].rem_euclid(26) as usize] += 1;
                }
            }
        }
        c.current_line = 0;
        c.top_line = 0;
        for l in 0..SCREEN_LINES {
            if l == 0 {
                c.banner(0, HEADER, HEADER_COLOR);
            } else {
                c.lines[l][0] = 0;
            }
        }
        self.decrypt_render(id, c);
        for l in 0..SCREEN_LINES as i32 {
            self.register_line(id, c, l);
        }
    }

    /// computer_decrypt_render: each shown row enciphered above as guessed so far, then the key table.
    pub(crate) fn decrypt_render(&mut self, id: usize, c: &mut Computer) {
        for (k, line) in DRAWN_ROWS.step_by(3).enumerate() {
            let row = c.decrypt.rows[k];
            self.decrypt_row(c, line, &row, false);
            self.register_line(id, c, line as i32);
            self.decrypt_row(c, line + 1, &row, true);
            self.register_line(id, c, line as i32 + 1);
        }
        self.decrypt_footer(id, c);
    }

    /// computer_decrypt_render_row: a row's letters as the player's guesses (blank when none) or enciphered, the
    /// picked letter in red.
    fn decrypt_row(&self, c: &mut Computer, line: usize, src: &[u8; COLUMNS], guessed: bool) {
        let d = &c.decrypt;
        let mut n = 0;
        while n < ROW_COLUMNS {
            let ch = src[n];
            if ch == 0 {
                break;
            }
            if !is_letter(ch) {
                c.lines[line][n] = ch;
            } else {
                let e = d.cipher[(ch - b'A') as usize];
                if guessed {
                    let g = d.guesses[e.rem_euclid(26) as usize];
                    c.lines[line][n] = if g == -1 { b' ' } else { (g + b'A' as i32) as u8 };
                } else {
                    c.lines[line][n] = (e + b'A' as i32) as u8;
                    c.colors[line][n] = if d.selected == e { SELECTED } else { WHITE };
                }
            }
            n += 1;
        }
        c.lines[line][n] = 0;
    }

    /// computer_decrypt_render_footer: each enciphered letter, how often it appears and the guess for it.
    fn decrypt_footer(&mut self, id: usize, c: &mut Computer) {
        for (line, guess_line, keys) in [(17, 18, FIRST_KEYS), (20, 21, LAST_KEYS)] {
            let end = keys.len() * 4;
            for (j, i) in keys.enumerate() {
                let p = j * 4;
                let n = c.decrypt.counts[i];
                c.lines[line][p] = b'A' + i as u8;
                c.colors[line][p] = if c.decrypt.selected != i as i32 { 0xf } else { SELECTED };
                c.lines[line][p + 1] = if n > 9 { b'0' + (n / 10) as u8 } else { b' ' };
                c.lines[line][p + 2] = b'0' + (n % 10) as u8;
                c.colors[line][p + 1] = COUNT_COLOR;
                c.colors[line][p + 2] = COUNT_COLOR;
                c.lines[line][p + 3] = b' ';
                let g = c.decrypt.guesses[i];
                c.lines[guess_line][p] = if g == -1 { b' ' } else { (g + b'A' as i32) as u8 };
                c.lines[guess_line][p + 1..p + 4].fill(b' ');
            }
            c.lines[line][end] = 0;
            c.lines[guess_line][end] = 0;
            self.register_line(id, c, line as i32);
            self.register_line(id, c, guess_line as i32);
            if line == 17 {
                self.register_line(id, c, 19);
            }
        }
    }

    /// computer_decrypt_progress: the share of letters guessed right.
    fn decrypt_progress(c: &Computer) -> f32 {
        let d = &c.decrypt;
        let (mut right, mut total) = (0, 0);
        for i in 0..26 {
            if d.counts[i] > 0 {
                total += 1;
            }
            let g = d.guesses[d.cipher[i].rem_euclid(26) as usize];
            if g != i as i32 {
                total += (g != -1) as i32;
            } else {
                right += 1;
            }
        }
        if total == 0 { 1.0 } else { right as f32 / total as f32 }
    }

    /// computer_decrypt_handle_key: a letter picks an enciphered letter then guesses it (space clears the guess);
    /// enter starts or stops the auto reveal; Ctrl-R forgets every guess, Ctrl-P prints the screen and Ctrl-X quits.
    pub(crate) fn decrypt_key(&mut self, id: usize, c: &mut Computer, key: i32) {
        let mut k = (key as u8).to_ascii_uppercase() as i32;
        if key & CTRL != 0 {
            match k {
                0x52 => {
                    c.decrypt.guesses = [-1; 26];
                    k = 0;
                }
                0x50 => {
                    self.print_to_memo(id, c);
                    return;
                }
                0x58 => {
                    c.mode = Mode::Shell;
                    for l in 0..SCREEN_LINES {
                        c.lines[l][0] = 0;
                        self.register_line(id, c, l as i32);
                    }
                    return;
                }
                _ => {}
            }
        }
        let t = c.decrypt.timer;
        if k == 0xa {
            if t != 0 {
                c.lines[STATUS_LINE][0] = 0;
                self.register_line(id, c, STATUS_LINE as i32);
                c.decrypt.timer = 0;
                if c.decrypt.selected != -1 {
                    c.decrypt.selected = -1;
                }
                return self.decrypt_render(id, c);
            }
            super::put_text(&mut c.lines[STATUS_LINE], b"AUTO DECRYPTING...");
            self.register_line(id, c, STATUS_LINE as i32);
            let p = Self::decrypt_progress(c);
            c.decrypt.timer = ((1.0 - p) * AUTO_TICKS + AUTO_MIN) as i32;
            if c.decrypt.timer > 0 {
                return;
            }
        } else if t > 0 {
            return;
        }
        let letter = (k - b'A' as i32) as u32 <= 25;
        let sel = c.decrypt.selected;
        if sel == -1 {
            if letter {
                c.decrypt.selected = k - b'A' as i32;
            }
        } else {
            if letter {
                c.decrypt.guesses[sel as usize] = k - b'A' as i32;
            } else if k == b' ' as i32 {
                c.decrypt.guesses[sel as usize] = -1;
            }
            c.decrypt.selected = -1;
        }
        self.decrypt_render(id, c);
    }

    /// decrypt_tick_reveal_timer: when the auto timer runs out a letter is revealed.
    pub(crate) fn decrypt_tick(&mut self, id: usize, c: &mut Computer) {
        if c.decrypt.timer > 0 {
            c.decrypt.timer -= 1;
            if c.decrypt.timer == 0 {
                self.decrypt_reveal(id, c);
            }
        }
    }

    /// decrypt_reveal_random_letter: a random letter that appears and is not guessed gets its true guess.
    fn decrypt_reveal(&mut self, id: usize, c: &mut Computer) {
        let mut tries = REVEAL_TRIES;
        let mut p = (crate::rng::rand() % 26) as usize;
        let e = loop {
            let e = c.decrypt.cipher[p].rem_euclid(26) as usize;
            if c.decrypt.counts[e] != 0 && c.decrypt.guesses[e] == -1 {
                break e;
            }
            p = (crate::rng::rand() % 26) as usize;
            tries -= 1;
            if tries == 0 {
                break c.decrypt.cipher[p].rem_euclid(26) as usize;
            }
        };
        c.decrypt.guesses[e] = p as i32;
        self.decrypt_render(id, c);
        c.lines[STATUS_LINE][0] = 0;
        self.register_line(id, c, STATUS_LINE as i32);
    }
}
