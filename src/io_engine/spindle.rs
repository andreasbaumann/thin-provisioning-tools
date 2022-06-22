use anyhow::{anyhow, Context, Result};
use roaring::RoaringBitmap;
use std::alloc::{alloc, dealloc, Layout};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::checksum::*;
use crate::io_engine::*;
use crate::pack::node_encode::*;

//------------------------------------------

/// Examining BTrees can lead to a lot of random io, which can be
/// very slow on spindle devices.  This io engine reads in all
/// metadata blocks of interest (we know these from the metadata
/// space map), compresses them and caches them in memory.  This
/// greatly speeds up performance but obviously uses a lot of memory.
/// Writes are not supported.

pub struct SpindleIoEngine {
    nr_blocks: u64,
    compressed: BTreeMap<u32, Vec<u8>>,
}

// Because we use O_DIRECT we need to use page aligned blocks.  Buffer
// manages allocation of this aligned memory.
struct Buffer {
    size: usize,
    align: usize,
    data: *mut u8,
}

impl Buffer {
    fn new(size: usize, align: usize) -> Self {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "out of memory");

        Self {
            size,
            align,
            data: ptr,
        }
    }

    pub fn get_data<'a>(&self) -> &'a mut [u8] {
        unsafe { std::slice::from_raw_parts_mut::<'a>(self.data, self.size) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.size, self.align).unwrap();
        unsafe {
            dealloc(self.data, layout);
        }
    }
}

unsafe impl Send for Buffer {}

fn pack_block<W: io::Write>(w: &mut W, kind: BT, buf: &[u8]) -> Result<()> {
    match kind {
        BT::THIN_SUPERBLOCK | BT::CACHE_SUPERBLOCK | BT::ERA_SUPERBLOCK => {
            pack_superblock(w, buf).context("unable to pack superblock")?
        }
        BT::NODE => pack_btree_node(w, buf).context("unable to pack btree node")?,
        BT::INDEX => pack_index(w, buf).context("unable to pack space map index")?,
        BT::BITMAP => pack_bitmap(w, buf).context("unable to pack space map bitmap")?,
        BT::ARRAY => pack_array(w, buf).context("unable to pack array block")?,
        BT::UNKNOWN => return Err(anyhow!("asked to pack an unknown block type")),
    }

    Ok(())
}

fn unpack_block(z: &[u8], loc: u64) -> Result<Block> {
    // FIXME: remove this copy
    let b = Block::new(loc);
    let mut c = std::io::Cursor::new(z);
    let data = crate::pack::vm::unpack(&mut c, BLOCK_SIZE as usize)?;
    unsafe {
        std::ptr::copy(data.as_ptr(), b.get_raw_ptr(), BLOCK_SIZE);
    }
    Ok(b)
}

impl SpindleIoEngine {
    pub fn new(path: &Path, blocks: &RoaringBitmap, excl: bool) -> Result<Self> {
        let nr_blocks = get_nr_blocks(path)?;
        let mut input = OpenOptions::new()
            .read(true)
            .custom_flags(if excl {
                libc::O_EXCL | libc::O_DIRECT
            } else {
                libc::O_DIRECT
            })
            .open(path)?;

        const CHUNK_SIZE: usize = 64 * 1024 * 1024;
        let buffer = Buffer::new(CHUNK_SIZE, 4096);

        let blocks_per_chunk = CHUNK_SIZE / BLOCK_SIZE;
        let complete_blocks = nr_blocks as usize / blocks_per_chunk;
        let mut total_packed = 0;
        let mut compressed = BTreeMap::new();

        for big_block in 0..complete_blocks {
            let data = buffer.get_data();
            input.read_exact(data)?;

            for b in 0..(CHUNK_SIZE / BLOCK_SIZE) {
                let block = (big_block * blocks_per_chunk) + b;

                if !blocks.contains(block as u32) {
                    continue;
                }

                let offset = b * BLOCK_SIZE;
                let data = &data[offset..(offset + BLOCK_SIZE)];
                let kind = metadata_block_type(data);
                if kind != BT::UNKNOWN {
                    let mut packed = Vec::with_capacity(64);
                    pack_block(&mut packed, kind, data)?;
                    total_packed += packed.len();

                    compressed.insert(block as u32, packed);
                }
            }
        }
        eprintln!("total packed = {}", total_packed);

        Ok(Self {
            nr_blocks,
            compressed,
        })
    }

    fn read_(&self, loc: u64) -> io::Result<Block> {
        if let Some(z) = self.compressed.get(&(loc as u32)) {
            unpack_block(z, loc).map_err(|_| io::Error::new(io::ErrorKind::Other, "unpack failed"))
        } else {
            todo!();
        }
    }
}

impl IoEngine for SpindleIoEngine {
    fn get_nr_blocks(&self) -> u64 {
        self.nr_blocks
    }

    fn get_batch_size(&self) -> usize {
        1
    }

    fn read(&self, loc: u64) -> io::Result<Block> {
        self.read_(loc)
    }

    fn read_many(&self, blocks: &[u64]) -> io::Result<Vec<io::Result<Block>>> {
        let mut bs = Vec::new();
        for b in blocks {
            bs.push(self.read_(*b));
        }
        Ok(bs)
    }

    fn write(&self, _b: &Block) -> io::Result<()> {
        todo!();
    }

    fn write_many(&self, _blocks: &[Block]) -> io::Result<Vec<io::Result<()>>> {
        todo!();
    }
}

//------------------------------------------
