use super::{Computer, Mode, SCREEN_LINES, WHITE};
use crate::sim::Sim;

const WIDTH: usize = 32;
const HEIGHT: usize = 24;
/// The play field: walls on rows 0 and 21 and columns 0 and 30 above row 21; the snake wraps at x 31 and y 22.
const WALL_ROW: usize = 21;
const WALL_COLUMN: usize = 30;
const WRAP_X: i32 = 31;
const WRAP_Y: i32 = 22;
const EMPTY: u8 = 0;
const WALL: u8 = 1;
const BODY: u8 = 2;
const HEAD: u8 = 3;
const APPLE: u8 = 4;
const RING: usize = 64;
const START: (i32, i32) = (15, 10);
const START_TICKS: i32 = 0x40;
const LEVEL_DONE_TICKS: i32 = 0x20;
const APPLE_POINTS: u32 = 5;
const GROWTH: usize = 4;
const APPLE_TRIES: i32 = 0x200;
/// The four quarters apples go in (left, top) and how many each gets.
const QUARTERS: [(i32, i32); 4] = [(1, 1), (0x11, 1), (0x11, 0xb), (1, 0xb)];
const APPLES_PER_QUARTER: usize = 2;
const HEADER: &str = "  SNAKE    SCORE:{score:05}  LEVEL:{level:03}    Ctrl-X EXIT";
const GAME_OVER: &[u8] = b"                           GAME OVER!";
const KEY_RIGHT: i32 = 0x13;
const KEY_LEFT: i32 = 0x12;
const KEY_DOWN: i32 = 0x11;
const KEY_UP: i32 = 0x10;
/// Board cells (item +0x1880, 32 per row) each level adds walls at, levels counted mod 8.
const LEVEL_WALLS: [&[usize]; 7] = [
    &[0x48, 0x68, 0x88, 0xa8, 0xc8, 0xe8, 0x108, 0x276, 0x256, 0x236, 0x216, 0x1f6, 0x1d6, 0x1b6],
    &[0x56, 0x76, 0x96, 0xb6, 0xd6, 0xf6, 0x116, 0x268, 0x248, 0x228, 0x208, 0x1e8, 0x1c8, 0x1a8],
    &[0x161, 0x162, 0x163, 0x164, 0x165, 0x166, 0x167, 0x168],
    &[0x17d, 0x17c, 0x17b, 0x17a, 0x179, 0x178, 0x177, 0x176],
    &[0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93],
    &[0x22b, 0x22c, 0x22d, 0x22e, 0x22f, 0x230, 0x231, 0x232, 0x233],
    &[0x196, 0x28],
];

/// SNAKE's state (item +0x1660 on): score, apples left on the level, the level, ticks to the next move, whether the
/// game is over, the heading (0 right, 1 down, 2 left, 3 up), the body ring's head and tail and the board.
#[derive(Clone, Debug, PartialEq)]
pub struct Snake {
    pub score: u32,
    pub apples: i32,
    pub level: i32,
    pub countdown: i32,
    pub over: i32,
    pub heading: i32,
    pub head: usize,
    pub tail: usize,
    pub ring: [(i32, i32); RING],
    pub board: [[u8; WIDTH]; HEIGHT],
}

impl Default for Snake {
    fn default() -> Self {
        Self { score: 0, apples: 0, level: 0, countdown: 0, over: 0, heading: 0, head: 0, tail: 0, ring: [(0, 0); RING], board: [[0; WIDTH]; HEIGHT] }
    }
}

impl Snake {
    fn cell(&mut self, (x, y): (i32, i32)) -> &mut u8 {
        &mut self.board[(y as usize).min(HEIGHT - 1)][(x as usize).min(WIDTH - 1)]
    }
}

impl Sim {
    /// computer_snake_init: a new game from level 0.
    pub(crate) fn snake_init(&mut self, id: usize, c: &mut Computer) {
        c.current_line = 0;
        c.top_line = 0;
        c.snake = Snake::default();
        self.snake_new_game(id, c);
    }

    /// computer_snake_new_game: the next level's board, 8 apples (2 a quarter) and the snake back at the start.
    fn snake_new_game(&mut self, id: usize, c: &mut Computer) {
        let s = &mut c.snake;
        for (y, row) in s.board.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                let wall = y == WALL_ROW || y == 0 || (y < WALL_ROW && (x == 0 || x == WALL_COLUMN));
                *cell = if wall { WALL } else { EMPTY };
            }
        }
        let level = (s.level & 7) as usize;
        for walls in LEVEL_WALLS.iter().take(level) {
            for &k in walls.iter() {
                s.board[k / WIDTH][k % WIDTH] = WALL;
            }
        }
        for &(left, top) in &QUARTERS {
            for _ in 0..APPLES_PER_QUARTER {
                let roll = || ((crate::rng::rand() % 12) as i32 + left, (crate::rng::rand() % 9) as i32 + top);
                let mut p = roll();
                let mut tries = APPLE_TRIES;
                let free = loop {
                    if *s.cell(p) == EMPTY {
                        break true;
                    }
                    p = roll();
                    tries -= 1;
                    if tries == 0 {
                        break *s.cell(p) == EMPTY;
                    }
                };
                if free {
                    *s.cell(p) = APPLE;
                    s.apples += 1;
                }
            }
        }
        s.head = 0;
        s.tail = 0;
        s.ring[0] = START;
        s.head = 1;
        s.ring[1] = START;
        s.head = 2;
        s.ring[2] = START;
        *s.cell(START) = HEAD;
        s.heading = 0;
        s.countdown = START_TICKS;
        s.level += 1;
        self.snake_render(id, c);
    }

    /// computer_snake_handle_key: the arrows turn (not back on itself), Ctrl-X quits.
    pub(crate) fn snake_key(&mut self, id: usize, c: &mut Computer, key: i32) {
        if key & 0x100 != 0 {
            if (key as u8).to_ascii_uppercase() == b'X' {
                c.mode = Mode::Shell;
                for l in 0..SCREEN_LINES {
                    c.lines[l][0] = 0;
                    c.colors[l][..63].fill(WHITE);
                    self.register_line(id, c, l as i32);
                }
            }
            return;
        }
        let h = &mut c.snake.heading;
        match key {
            KEY_RIGHT if *h != 2 => *h = 0,
            KEY_LEFT if *h != 0 => *h = 2,
            KEY_DOWN if *h != 3 => *h = 1,
            KEY_UP if *h != 1 => *h = 3,
            _ => {}
        }
    }

    /// computer_snake_game: the snake moves every 16 ticks (4 fewer each 8 levels, 4 at least); an apple scores 5 and
    /// grows it by 4, running into anything ends the game, and the level is done when the apples are gone.
    pub(crate) fn snake_tick(&mut self, id: usize, c: &mut Computer) {
        let s = &mut c.snake;
        if s.over != 0 {
            return;
        }
        s.countdown -= 1;
        if s.countdown > 0 {
            return;
        }
        if s.apples == 0 {
            return self.snake_new_game(id, c);
        }
        s.countdown = (16 - (s.level >> 3 << 2)).max(4);
        let (mut x, mut y) = s.ring[s.head & (RING - 1)];
        s.head = (s.head + 1) & (RING - 1);
        match s.heading {
            0 => {
                x += 1;
                if x >= WRAP_X {
                    x = 0;
                }
            }
            1 => {
                y += 1;
                if y >= WRAP_Y {
                    y = 0;
                }
            }
            2 => {
                x -= 1;
                if x < 0 {
                    x = WRAP_X - 1;
                }
            }
            3 => {
                y -= 1;
                if y < 0 {
                    y = WRAP_Y - 1;
                }
            }
            _ => {}
        }
        s.ring[s.head] = (x, y);
        if *s.cell((x, y)) == APPLE {
            *s.cell((x, y)) = EMPTY;
            s.score = ((s.score as u16).wrapping_add(APPLE_POINTS as u16)) as u32;
            s.apples -= 1;
            if s.apples <= 0 {
                s.countdown = LEVEL_DONE_TICKS;
                s.apples = 0;
            }
            let end = s.ring[s.tail];
            for _ in 0..GROWTH {
                s.tail = (s.tail + RING - 1) & (RING - 1);
                s.ring[s.tail] = end;
            }
        }
        let head = s.ring[s.head];
        if *s.cell(head) != EMPTY {
            s.over = 1;
        }
        let tail = s.ring[s.tail];
        *s.cell(tail) = EMPTY;
        s.tail = (s.tail + 1) & (RING - 1);
        let mut k = s.tail;
        while k != s.head {
            let p = s.ring[k];
            *s.cell(p) = BODY;
            k = (k + 1) & (RING - 1);
        }
        let head = s.ring[s.head];
        *s.cell(head) = HEAD;
        self.snake_render(id, c);
    }

    /// computer_snake_render: the score line, then the board two columns a cell.
    fn snake_render(&mut self, id: usize, c: &mut Computer) {
        let header = HEADER.replace("{score:05}", &format!("{:05}", c.snake.score)).replace("{level:03}", &format!("{:03}", c.snake.level));
        c.banner(0, header.as_bytes(), 0x70);
        for row in 1..SCREEN_LINES {
            for k in 0..0x3e {
                c.lines[row][k] = b' ';
                c.colors[row][k] = match c.snake.board[row][k >> 1] {
                    EMPTY => WHITE,
                    WALL => 0x7f,
                    BODY => 0x1f,
                    HEAD => 0x9f,
                    _ => 0x2f,
                };
            }
            c.lines[row][0x3e] = 0;
        }
        if c.snake.over != 0 {
            c.banner(1, GAME_OVER, WHITE);
        }
        for l in 0..SCREEN_LINES as i32 {
            self.register_line(id, c, l);
        }
    }
}
