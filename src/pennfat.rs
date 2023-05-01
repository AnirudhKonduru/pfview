use std::{cmp::min, fs::File, time::SystemTime};

use chrono::{TimeZone, Utc};
use memmap2::Mmap;

/// PennFat filesystem representation
pub struct PennFat {
    /// The filesystem file
    file: File,
    /// The block size of the filesystem
    block_size: u16,
    /// The number of FAT blocks in the filesystem
    num_fat_blocks: u8,
    /// The filesystem file as a memmap
    bytes: Mmap,
    /// The time of the last update to the filesystem file
    last_update: SystemTime,
}

/// PennFat filesystem errors
#[derive(thiserror::Error, Debug)]
pub enum PfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File size does not match FAT configuration")]
    FileSize,
    #[error("Invalid block number {0}, must be >=1 and <= {1}")]
    InvalidBlockNumber(u16, u16),
}

type Result<T> = std::result::Result<T, PfError>;

impl PennFat {
    /// Load a PennFat filesystem from a file on disk
    /// This will mmap the file, so it will be updated if the file changes
    pub fn load(path: &str) -> Result<Self> {
        let file = File::open(path).unwrap();
        // make sure the mmap updates if the file changes
        let bytes = unsafe { Mmap::map(&file).unwrap() };
        let last_update = file.metadata()?.modified()?;

        let block_size_config = bytes[0];
        // second byte is the number of blocks, as an unsigned 8-bit integer
        let num_fat_blocks: u8 = bytes[1];
        let block_size: u16 = 256 << block_size_config;

        let s = Self {
            file,
            block_size,
            num_fat_blocks,
            bytes,
            last_update,
        };

        if s.file.metadata()?.len() != s.fat_size() as u64 + s.data_size() {
            return Err(PfError::FileSize);
        }

        Ok(s)
    }

    /// Reload the filesystem from disk if it has changed since the last load
    pub fn reload(&mut self) -> Result<()> {
        // reload the file, but only if it has changed
        if self.file.metadata()?.modified()? == self.last_update {
            return Ok(());
        }
        self.bytes = unsafe { Mmap::map(&self.file).unwrap() };
        self.last_update = self.file.metadata()?.modified()?;

        Ok(())
    }

    /// Get the block size of the filesystem
    pub fn block_size(&self) -> u16 {
        self.block_size
    }

    /// Get the time of the last update to the filesystem file
    pub fn last_update_time(&self) -> SystemTime {
        self.last_update
    }

    /// Get the size of the FAT in bytes
    pub fn fat_size(&self) -> u32 {
        self.block_size as u32 * self.num_fat_blocks as u32
    }

    /// Get the number of entries in the FAT table
    pub fn num_fat_entries(&self) -> u32 {
        self.fat_size() / 2
    }

    /// Get the number of data blocks in the filesystem
    pub fn data_block_count(&self) -> u16 {
        min((self.num_fat_entries() - 1) as u16, 0xFFFF - 1)
    }

    /// Get the size of the data in bytes (not including the FAT)
    fn data_size(&self) -> u64 {
        self.block_size as u64 * self.data_block_count() as u64
    }

    /// Get the FAT table as a vector of (block_num, next_block) tuples
    pub fn get_fat_table(&self) -> Vec<(u16, u16)> {
        let mut fat_table = Vec::new();
        for i in 0..self.num_fat_entries() {
            let offset = (i * 2) as usize;
            let entry = u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]]);
            if entry != 0 {
                fat_table.push((i as u16, entry));
            }
        }
        fat_table
    }

    /// Get a block from the filesystem by block number
    pub fn get_block(&self, block_num: u16) -> Result<Block> {
        if block_num == 0 || block_num > self.data_block_count() {
            return Err(PfError::InvalidBlockNumber(
                block_num,
                self.data_block_count(),
            ));
        }
        let offset: usize =
            self.fat_size() as usize + (block_num as usize - 1) * self.block_size as usize;
        Ok(Block::from(
            &self.bytes[offset..offset + self.block_size as usize],
        ))
    }

    /// Get a file from the filesystem, starting at the given block number
    #[allow(dead_code)]
    pub fn get_file(&self, block_num: u16) -> Result<Vec<u8>> {
        let mut file = Vec::new();
        let mut block = block_num;
        loop {
            let next_block = u16::from_le_bytes([
                self.bytes[2 + block as usize * 2],
                self.bytes[2 + block as usize * 2 + 1],
            ]);
            file.extend_from_slice(&self.get_block(block)?.data);
            if next_block == 0xFFFF {
                break;
            }
            block = next_block;
        }
        Ok(file)
    }
}

/// A PennFat block
pub struct Block {
    /// The block data
    pub data: Vec<u8>,
}

impl From<&[u8]> for Block {
    /// Create a block from a slice of bytes
    fn from(block: &[u8]) -> Self {
        Block {
            data: block.to_vec(),
        }
    }
}

impl Block {
    /// Get the block as a string, replacing non-printable characters with '.'
    pub fn as_raw(&self) -> String {
        let mut string = String::new();
        for byte in &self.data {
            if *byte < 32 || *byte > 176 {
                string.push('.');
            } else {
                string.push(*byte as char);
            }
        }
        string
    }

    /// Get the block as a vector of dentries
    pub fn as_dentries(&self) -> Vec<Dentry> {
        self.data
            .chunks(64)
            .map(|chunk| Dentry::from(chunk))
            .collect()
    }
}

/// A PennFat directory entry
pub struct Dentry {
    /// The name of the file
    pub name: [u8; 32],
    /// The size of the file in bytes
    pub size: u32,
    /// The first block of the file
    pub first_block: u16,
    /// The type of the file (0 = file, 1 = directory, 2 = symlink)
    pub type_: u8,
    /// The permissions of the file
    pub perm: u8,
    /// The modification time of the file
    pub mtime: u64,
    /// Reserved bytes
    pub _reserved: [u8; 16],
}

impl std::fmt::Display for Dentry {
    /// Format a dentry for printing
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = String::from_utf8_lossy(&self.name);
        let size = self.size;
        let first_block = self.first_block;
        let _type = self.type_;
        let perm = self.perm;
        // check if mtime is valid unix timestamp
        let time = if self.mtime > 253402300799 {
            "invalid".to_owned()
        } else {
            let naive = Utc
                .timestamp_millis_opt(self.mtime.try_into().unwrap())
                .single()
                .unwrap();
            // format to human readable form
            naive.format("%Y-%m-%d %H:%M:%S").to_string()
        };

        write!(
            f,
            "name: {}, size: {}, first_block: {}, type: {}, perm: {}, mtime: {},",
            name, size, first_block, _type, perm, time
        )
    }
}

impl From<&[u8]> for Dentry {
    /// Create a dentry from a slice of bytes
    fn from(block: &[u8]) -> Self {
        Dentry {
            name: block[0..32].try_into().unwrap(),
            size: u32::from_le_bytes(block[32..36].try_into().unwrap()),
            first_block: u16::from_le_bytes(block[36..38].try_into().unwrap()),
            type_: block[38],
            perm: block[39],
            mtime: u64::from_le_bytes(block[40..48].try_into().unwrap()),
            _reserved: block[48..64].try_into().unwrap(),
        }
    }
}
