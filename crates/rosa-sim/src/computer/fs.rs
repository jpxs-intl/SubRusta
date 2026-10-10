/// The computers' file system: 1024 volumes (0x6e067a0, 0x2828 each) of up to 256 directories and 256 files, and
/// 8192 file contents (0xdaa2080, 0x488 each) the files point at.
pub const VOLUMES: usize = 1024;
pub const CONTENTS: usize = 8192;
const MAX_FILES: usize = 256;
const NAME_LEN: usize = 15;
/// A volume's capacity (volume +0x4): a disk holds 360, a computer 10000. COPY onto a volume of 399 or less fails
/// once it has 3 files.
pub const DISK_CAPACITY: i32 = 360;
pub const COMPUTER_CAPACITY: i32 = 10000;
pub const SANDBOX_CAPACITY: i32 = 20000;
pub const SMALL_VOLUME: i32 = 399;
pub const SMALL_VOLUME_FILES: usize = 2;

/// A directory (16 bytes from volume +0xc): its name and its parent. Entry 0 is the root.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dir {
    pub name: String,
    pub parent: i32,
}

/// A file (24 bytes from volume +0x1010): the content it holds, its name and its directory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct File {
    pub data: i32,
    pub name: String,
    pub dir: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Volume {
    pub capacity: i32,
    pub dirs: Vec<Dir>,
    pub files: Vec<File>,
}

/// A file's content (0x488): whether it is taken (+0), its text (+0x4), whether it is enciphered (+0x404) and the
/// cipher, a permutation of the 26 letters (+0x408). Some code writes a content without taking it.
#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    pub active: bool,
    pub text: Vec<u8>,
    pub encrypted: bool,
    pub cipher: [i32; 26],
}

impl Default for Content {
    fn default() -> Self {
        Self { active: false, text: Vec::new(), encrypted: false, cipher: [0; 26] }
    }
}

#[derive(Clone, Debug)]
pub struct FileSystem {
    pub volumes: Vec<Option<Volume>>,
    pub contents: Vec<Content>,
}

impl Default for FileSystem {
    fn default() -> Self {
        Self { volumes: vec![None; VOLUMES], contents: vec![Content::default(); CONTENTS] }
    }
}

impl FileSystem {
    /// The reset_game part: every volume and content goes, and content 0 is kept taken.
    pub fn reset(&mut self) {
        self.volumes.iter_mut().for_each(|v| *v = None);
        self.contents.iter_mut().for_each(|c| c.active = false);
        self.contents[0].active = true;
    }

    /// computer_fs_alloc_slot: the first free volume, emptied, with the given capacity.
    pub fn alloc_volume(&mut self, capacity: i32) -> i32 {
        let Some(i) = self.volumes.iter().position(Option::is_none) else { return -1 };
        self.volumes[i] = Some(Volume { capacity, dirs: Vec::new(), files: Vec::new() });
        i as i32
    }

    pub fn volume(&self, v: i32) -> Option<&Volume> {
        usize::try_from(v).ok().and_then(|v| self.volumes.get(v)).and_then(Option::as_ref)
    }

    pub fn volume_mut(&mut self, v: i32) -> Option<&mut Volume> {
        usize::try_from(v).ok().and_then(|v| self.volumes.get_mut(v)).and_then(Option::as_mut)
    }

    /// The first free content, taken and plain (its old text stays).
    pub fn alloc_content(&mut self) -> i32 {
        let Some(i) = self.contents.iter().position(|c| !c.active) else { return -1 };
        self.contents[i].active = true;
        self.contents[i].encrypted = false;
        i as i32
    }

    pub fn content(&self, c: i32) -> Option<&Content> {
        usize::try_from(c).ok().and_then(|c| self.contents.get(c))
    }

    pub fn content_mut(&mut self, c: i32) -> Option<&mut Content> {
        usize::try_from(c).ok().and_then(|c| self.contents.get_mut(c))
    }

    /// computer_fs_add_program: a file in `dir` holding `data`; none past 256. Returns its index.
    pub fn add_file(&mut self, v: i32, dir: i32, name: &str, data: i32) -> i32 {
        let Some(vol) = self.volume_mut(v) else { return -1 };
        if vol.files.len() >= MAX_FILES {
            return -1;
        }
        vol.files.push(File { data, name: name.chars().take(NAME_LEN).collect(), dir });
        vol.files.len() as i32 - 1
    }

    /// computer_fs_find_dir: the directory `name` in `parent`.
    pub fn find_dir(&self, v: i32, parent: i32, name: &str) -> i32 {
        self.volume(v).and_then(|vol| vol.dirs.iter().enumerate().skip(1).find(|(_, d)| d.parent == parent && d.name == name)).map_or(-1, |(i, _)| i as i32)
    }

    /// computer_fs_find_program: the file `name` in `dir`.
    pub fn find_file(&self, v: i32, dir: i32, name: &str) -> i32 {
        self.volume(v).and_then(|vol| vol.files.iter().position(|f| f.dir == dir && f.name == name)).map_or(-1, |i| i as i32)
    }

    /// decrypt_init_cipher_permutation: content `c` becomes enciphered with the letters shuffled by 1024 swaps.
    pub fn encipher(&mut self, c: i32) {
        let Some(content) = self.content_mut(c) else { return };
        content.encrypted = true;
        content.cipher = std::array::from_fn(|i| i as i32);
        for _ in 0..1024 {
            let a = (crate::rng::rand() % 26) as usize;
            let b = (crate::rng::rand() % 26) as usize;
            content.cipher.swap(a, b);
        }
    }
}
