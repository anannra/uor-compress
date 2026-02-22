use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::algebra::address::ChunkId;
use crate::archive::format::{ArchiveHeader, FileMapEntry, TocEntry, MAGIC, VERSION};
use crate::backend::traits::BackendTag;
use crate::error::{Error, Result};

/// Archive reader with random access support.
pub struct ArchiveReader<R: Read + Seek> {
    reader: R,
    pub header: ArchiveHeader,
    pub toc: Vec<TocEntry>,
    pub file_map: Vec<FileMapEntry>,
    /// Index from ChunkId to TOC index for fast lookup.
    toc_index: HashMap<ChunkId, usize>,
}

impl<R: Read + Seek> ArchiveReader<R> {
    /// Open an archive and read all metadata (header, TOC, file map).
    pub fn open(mut reader: R) -> Result<Self> {
        let header = Self::read_header(&mut reader)?;

        // Read TOC.
        reader.seek(SeekFrom::Start(header.toc_offset))?;
        let mut toc = Vec::with_capacity(header.chunk_count as usize);
        let mut toc_index = HashMap::new();
        for i in 0..header.chunk_count {
            let entry = Self::read_toc_entry(&mut reader)?;
            toc_index.insert(entry.chunk_id, i as usize);
            toc.push(entry);
        }

        // Read file map.
        reader.seek(SeekFrom::Start(header.file_map_offset))?;
        let mut file_map = Vec::with_capacity(header.file_map_count as usize);
        for _ in 0..header.file_map_count {
            file_map.push(Self::read_file_map_entry(&mut reader)?);
        }

        Ok(Self {
            reader,
            header,
            toc,
            file_map,
            toc_index,
        })
    }

    /// Read the compressed data for a chunk by its TOC entry.
    pub fn read_chunk_data(&mut self, toc_entry: &TocEntry) -> Result<Vec<u8>> {
        self.reader
            .seek(SeekFrom::Start(toc_entry.data_offset))?;
        let mut buf = vec![0u8; toc_entry.compressed_size as usize];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Look up a TOC entry by ChunkId.
    pub fn find_toc_entry(&self, id: &ChunkId) -> Option<&TocEntry> {
        self.toc_index.get(id).map(|&idx| &self.toc[idx])
    }

    /// Read the JSON-LD manifest, if present.
    pub fn read_manifest(&mut self) -> Result<Option<String>> {
        if !self.header.has_manifest() || self.header.manifest_offset == 0 {
            return Ok(None);
        }
        self.reader
            .seek(SeekFrom::Start(self.header.manifest_offset))?;
        let len = self.reader.read_u32::<LittleEndian>()? as usize;
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf)?;
        String::from_utf8(buf)
            .map(Some)
            .map_err(|e| Error::InvalidArchive(format!("invalid UTF-8 in manifest: {e}")))
    }

    fn read_header(r: &mut R) -> Result<ArchiveHeader> {
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(Error::InvalidArchive("bad magic bytes".to_string()));
        }

        let version = r.read_u16::<LittleEndian>()?;
        if version != VERSION {
            return Err(Error::InvalidArchive(format!(
                "unsupported version: {version}"
            )));
        }

        let flags = r.read_u32::<LittleEndian>()?;
        let original_size = r.read_u64::<LittleEndian>()?;
        let chunk_count = r.read_u32::<LittleEndian>()?;
        let file_map_count = r.read_u32::<LittleEndian>()?;
        let toc_offset = r.read_u64::<LittleEndian>()?;
        let file_map_offset = r.read_u64::<LittleEndian>()?;
        let manifest_offset = r.read_u64::<LittleEndian>()?;
        let mut checksum = [0u8; 32];
        r.read_exact(&mut checksum)?;
        let mut _reserved = [0u8; 2];
        r.read_exact(&mut _reserved)?;

        Ok(ArchiveHeader {
            version,
            flags,
            original_size,
            chunk_count,
            file_map_count,
            toc_offset,
            file_map_offset,
            manifest_offset,
            checksum,
        })
    }

    fn read_toc_entry(r: &mut R) -> Result<TocEntry> {
        let mut id_bytes = [0u8; 32];
        r.read_exact(&mut id_bytes)?;
        let chunk_id = ChunkId::from_bytes(id_bytes);

        let backend = BackendTag::from_u8(r.read_u8()?)?;
        let data_offset = r.read_u64::<LittleEndian>()?;
        let compressed_size = r.read_u32::<LittleEndian>()?;
        let original_size = r.read_u32::<LittleEndian>()?;

        let has_base = r.read_u8()?;
        let base_chunk_id = if has_base == 1 {
            let mut base_bytes = [0u8; 32];
            r.read_exact(&mut base_bytes)?;
            Some(ChunkId::from_bytes(base_bytes))
        } else {
            None
        };

        let mut stratum_summary = [0u8; 9];
        r.read_exact(&mut stratum_summary)?;

        Ok(TocEntry {
            chunk_id,
            backend,
            data_offset,
            compressed_size,
            original_size,
            base_chunk_id,
            stratum_summary,
        })
    }

    fn read_file_map_entry(r: &mut R) -> Result<FileMapEntry> {
        let file_offset = r.read_u64::<LittleEndian>()?;
        let mut id_bytes = [0u8; 32];
        r.read_exact(&mut id_bytes)?;
        let chunk_id = ChunkId::from_bytes(id_bytes);
        let length = r.read_u32::<LittleEndian>()?;

        Ok(FileMapEntry {
            file_offset,
            chunk_id,
            length,
        })
    }
}
