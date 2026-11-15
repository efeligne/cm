use std::{
    error::Error,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use tempfile::TempDir;

#[derive(Clone, Copy)]
pub struct TextRef(usize);

enum Entry {
    Ram(String),
    Disk { path: PathBuf, len: usize },
}

pub struct TextStore {
    entries: Vec<Entry>,
    ram_used: usize,
    budget: usize,
    _directory: TempDir,
    next_file: usize,
}

impl TextStore {
    pub fn new(budget: usize) -> Result<Self, Box<dyn Error>> {
        let cache_root = match std::env::var_os("CM_CACHE_DIR") {
            Some(path) => PathBuf::from(path),
            None => dirs::cache_dir()
                .ok_or("Could not determine the user cache directory; set CM_CACHE_DIR.")?
                .join("cm"),
        };
        fs::create_dir_all(&cache_root)?;
        let directory = tempfile::Builder::new()
            .prefix("spill-")
            .tempdir_in(cache_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            entries: Vec::new(),
            ram_used: 0,
            budget,
            _directory: directory,
            next_file: 0,
        })
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn insert(&mut self, text: String) -> Result<TextRef, Box<dyn Error>> {
        let len = text.len();
        if len > self.budget {
            return self.insert_disk(text);
        }

        while self.ram_used.saturating_add(len) > self.budget {
            let Some(index) = self
                .entries
                .iter()
                .position(|entry| matches!(entry, Entry::Ram(_)))
            else {
                break;
            };
            self.spill(index)?;
        }

        let reference = TextRef(self.entries.len());
        self.entries.push(Entry::Ram(text));
        self.ram_used += len;
        Ok(reference)
    }

    fn insert_disk(&mut self, text: String) -> Result<TextRef, Box<dyn Error>> {
        let path = self.write_spill(&text)?;
        let reference = TextRef(self.entries.len());
        self.entries.push(Entry::Disk {
            path,
            len: text.len(),
        });
        Ok(reference)
    }

    fn spill(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        let Entry::Ram(text) =
            std::mem::replace(&mut self.entries[index], Entry::Ram(String::new()))
        else {
            return Ok(());
        };
        let len = text.len();
        let path = self.write_spill(&text)?;
        self.entries[index] = Entry::Disk { path, len };
        self.ram_used -= len;
        Ok(())
    }

    fn write_spill(&mut self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            ._directory
            .path()
            .join(format!("{:06}.txt", self.next_file));
        self.next_file += 1;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(path)
    }

    pub fn len(&self, reference: TextRef) -> usize {
        match &self.entries[reference.0] {
            Entry::Ram(text) => text.len(),
            Entry::Disk { len, .. } => *len,
        }
    }

    pub fn read_all(&self, reference: TextRef) -> Result<String, Box<dyn Error>> {
        self.read_range(reference, 0, self.len(reference))
    }

    pub fn read_range(
        &self,
        reference: TextRef,
        start: usize,
        len: usize,
    ) -> Result<String, Box<dyn Error>> {
        let end = start
            .checked_add(len)
            .ok_or("Text range overflow while reading spilled data.")?;
        if end > self.len(reference) {
            return Err("Text range exceeds the stored block.".into());
        }

        match &self.entries[reference.0] {
            Entry::Ram(text) => Ok(text[start..end].to_string()),
            Entry::Disk { path, .. } => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(start as u64))?;
                let mut bytes = vec![0; len];
                file.read_exact(&mut bytes)?;
                Ok(String::from_utf8(bytes)?)
            }
        }
    }

    pub fn read_limited(
        &self,
        reference: TextRef,
        start: usize,
        max_bytes: usize,
        prefer_newline: bool,
    ) -> Result<(String, usize), Box<dyn Error>> {
        if max_bytes == 0 {
            return Err("Cannot read a text block with a zero byte limit.".into());
        }
        let total = self.len(reference);
        if start >= total {
            return Ok((String::new(), start));
        }
        let end = start.saturating_add(max_bytes).min(total);
        let mut bytes = self.read_bytes(reference, start, end - start)?;
        let valid_len = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(error) => return Err(error.into()),
        };
        bytes.truncate(valid_len);
        let mut text = String::from_utf8(bytes)?;
        if end < total && prefer_newline {
            if let Some(position) = text.rfind('\n') {
                text.truncate(position + 1);
            }
        }
        let next = start + text.len();
        Ok((text, next))
    }

    fn read_bytes(
        &self,
        reference: TextRef,
        start: usize,
        len: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        match &self.entries[reference.0] {
            Entry::Ram(text) => Ok(text.as_bytes()[start..start + len].to_vec()),
            Entry::Disk { path, .. } => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(start as u64))?;
                let mut bytes = vec![0; len];
                file.read_exact(&mut bytes)?;
                Ok(bytes)
            }
        }
    }
}
