use std::io::{self, Seek, Write};

use byteorder::{LittleEndian, WriteBytesExt};

use crate::archive::format::{flags, ArchiveHeader, FileMapEntry, TocEntry, MAGIC, VERSION};
use crate::error::Result;

/// Streaming archive writer.
pub struct ArchiveWriter<W: Write + Seek> {
    writer: W,
    toc_entries: Vec<TocEntry>,
    file_map_entries: Vec<FileMapEntry>,
    data_offset: u64,
    header: ArchiveHeader,
}

/// Size of the fixed header on disk.
const HEADER_SIZE: u64 = 88;

impl<W: Write + Seek> ArchiveWriter<W> {
    /// Create a new archive writer. Writes the header placeholder immediately.
    pub fn new(mut writer: W, original_size: u64, checksum: [u8; 32], archive_flags: u32) -> Result<Self> {
        // Write placeholder header (will be rewritten at finalize).
        let header = ArchiveHeader {
            version: VERSION,
            flags: archive_flags,
            original_size,
            chunk_count: 0,
            file_map_count: 0,
            toc_offset: 0,
            file_map_offset: 0,
            manifest_offset: 0,
            checksum,
        };
        Self::write_header(&mut writer, &header)?;

        Ok(Self {
            writer,
            toc_entries: Vec::new(),
            file_map_entries: Vec::new(),
            data_offset: HEADER_SIZE,
            header,
        })
    }

    /// Write a compressed chunk's data. Returns the offset where it was written.
    pub fn write_chunk_data(
        &mut self,
        toc: TocEntry,
        compressed_data: &[u8],
    ) -> Result<()> {
        self.writer.write_all(compressed_data)?;
        self.toc_entries.push(toc);
        self.data_offset += compressed_data.len() as u64;
        Ok(())
    }

    /// Add a file map entry.
    pub fn add_file_map_entry(&mut self, entry: FileMapEntry) {
        self.file_map_entries.push(entry);
    }

    /// Current write position (offset for next chunk data).
    pub fn current_data_offset(&self) -> u64 {
        self.data_offset
    }

    /// Finalize the archive: write TOC, file map, optional manifest, then rewrite header.
    pub fn finalize(mut self, manifest: Option<&[u8]>) -> Result<()> {
        // Write TOC. Clone to avoid borrow conflict.
        let toc_offset = self.data_offset;
        let toc_entries = std::mem::take(&mut self.toc_entries);
        for entry in &toc_entries {
            Self::write_toc_entry_to(&mut self.writer, entry)?;
        }

        // Write file map.
        let file_map_offset = self.writer.stream_position().map_err(io::Error::from)?;
        let file_map_entries = std::mem::take(&mut self.file_map_entries);
        for entry in &file_map_entries {
            Self::write_file_map_entry_to(&mut self.writer, entry)?;
        }

        // Write manifest if provided.
        let manifest_offset = if let Some(manifest_data) = manifest {
            let offset = self.writer.stream_position().map_err(io::Error::from)?;
            self.writer
                .write_u32::<LittleEndian>(manifest_data.len() as u32)?;
            self.writer.write_all(manifest_data)?;
            offset
        } else {
            0
        };

        // Rewrite header with final offsets.
        self.header.chunk_count = toc_entries.len() as u32;
        self.header.file_map_count = file_map_entries.len() as u32;
        self.header.toc_offset = toc_offset;
        self.header.file_map_offset = file_map_offset;
        self.header.manifest_offset = manifest_offset;
        if manifest.is_some() {
            self.header.flags |= flags::HAS_MANIFEST;
        }

        self.writer.seek(io::SeekFrom::Start(0))?;
        Self::write_header(&mut self.writer, &self.header)?;

        Ok(())
    }

    fn write_header(w: &mut W, h: &ArchiveHeader) -> Result<()> {
        w.write_all(&MAGIC)?;
        w.write_u16::<LittleEndian>(h.version)?;
        w.write_u32::<LittleEndian>(h.flags)?;
        w.write_u64::<LittleEndian>(h.original_size)?;
        w.write_u32::<LittleEndian>(h.chunk_count)?;
        w.write_u32::<LittleEndian>(h.file_map_count)?;
        w.write_u64::<LittleEndian>(h.toc_offset)?;
        w.write_u64::<LittleEndian>(h.file_map_offset)?;
        w.write_u64::<LittleEndian>(h.manifest_offset)?;
        w.write_all(&h.checksum)?;
        // 2 bytes reserved padding to reach 88 bytes.
        w.write_all(&[0u8; 2])?;
        Ok(())
    }

    fn write_toc_entry_to(w: &mut W, entry: &TocEntry) -> Result<()> {
        w.write_all(entry.chunk_id.as_bytes())?;
        w.write_u8(entry.backend as u8)?;
        w.write_u64::<LittleEndian>(entry.data_offset)?;
        w.write_u32::<LittleEndian>(entry.compressed_size)?;
        w.write_u32::<LittleEndian>(entry.original_size)?;
        match &entry.base_chunk_id {
            Some(base) => {
                w.write_u8(1)?;
                w.write_all(base.as_bytes())?;
            }
            None => {
                w.write_u8(0)?;
            }
        }
        w.write_all(&entry.stratum_summary)?;
        Ok(())
    }

    fn write_file_map_entry_to(w: &mut W, entry: &FileMapEntry) -> Result<()> {
        w.write_u64::<LittleEndian>(entry.file_offset)?;
        w.write_all(entry.chunk_id.as_bytes())?;
        w.write_u32::<LittleEndian>(entry.length)?;
        Ok(())
    }
}
