use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Args;

use uor_compress::archive::reader::ArchiveReader;
use uor_compress::chunk::ChunkParams;
use uor_compress::pipeline::config::{CompressConfig, CompressionLevel, CompressionMode};

#[derive(Args)]
pub struct CompressArgs {
    /// Input file to compress
    pub input: PathBuf,

    /// Output file (default: <input>.uorc)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Compression mode
    #[arg(short, long, default_value = "lossless")]
    pub mode: String,

    /// Compression level: fast, default, best
    #[arg(short, long, default_value = "default")]
    pub level: String,

    /// Minimum fidelity for lossy mode (0.0-1.0)
    #[arg(long, default_value = "0.95")]
    pub fidelity: f64,

    /// Stratum threshold for lossy quantization
    #[arg(long, default_value = "2")]
    pub stratum_threshold: u8,

    /// Include JSON-LD manifest in archive
    #[arg(long)]
    pub manifest: bool,

    /// Include derivation certificates
    #[arg(long)]
    pub certificates: bool,

    /// Verify decompression after compression
    #[arg(long)]
    pub verify: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args)]
pub struct DecompressArgs {
    /// Input .uorc file
    pub input: PathBuf,

    /// Output file (default: strip .uorc extension)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Verify chunk integrity during decompression
    #[arg(long)]
    pub verify: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args)]
pub struct InspectArgs {
    /// Archive file to inspect
    pub archive: PathBuf,

    /// Output format: text, json, jsonld
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Print only the JSON-LD manifest
    #[arg(long)]
    pub manifest: bool,

    /// Print per-chunk details
    #[arg(long)]
    pub chunks: bool,
}

#[derive(Args)]
pub struct VerifyArgs {
    /// Archive file to verify
    pub archive: PathBuf,
}

#[derive(Args)]
pub struct BenchArgs {
    /// Input file to benchmark
    pub input: PathBuf,
}

pub fn compress(args: CompressArgs) -> Result<()> {
    let output = args
        .output
        .unwrap_or_else(|| PathBuf::from(format!("{}.uorc", args.input.display())));

    let mode = match args.mode.as_str() {
        "lossless" => CompressionMode::Lossless,
        "lossy" => CompressionMode::Lossy {
            min_fidelity: args.fidelity,
            stratum_threshold: args.stratum_threshold,
        },
        other => anyhow::bail!("unknown mode: {other} (use 'lossless' or 'lossy')"),
    };

    let level = match args.level.as_str() {
        "fast" => CompressionLevel::Fast,
        "default" => CompressionLevel::Default,
        "best" => CompressionLevel::Best,
        other => anyhow::bail!("unknown level: {other} (use 'fast', 'default', or 'best')"),
    };

    let config = CompressConfig {
        mode,
        level,
        chunk_params: ChunkParams::default(),
        emit_manifest: args.manifest,
        emit_certificates: args.certificates,
        verify_on_compress: args.verify,
    };

    let start = Instant::now();
    let stats = uor_compress::pipeline::compress::compress_file(&args.input, &output, &config)
        .with_context(|| format!("failed to compress {}", args.input.display()))?;
    let elapsed = start.elapsed();

    println!("Compressed {} -> {}", args.input.display(), output.display());
    println!(
        "  Original:   {} bytes",
        stats.original_size
    );
    println!(
        "  Compressed: {} bytes",
        stats.compressed_size
    );
    println!("  Ratio:      {:.2}x", stats.ratio());
    println!(
        "  Chunks:     {} total, {} unique, {} duplicates, {} deltas",
        stats.chunk_count, stats.unique_chunks, stats.duplicate_chunks, stats.delta_chunks
    );
    println!("  Time:       {:.2?}", elapsed);

    Ok(())
}

pub fn decompress(args: DecompressArgs) -> Result<()> {
    let output = args.output.unwrap_or_else(|| {
        let s = args.input.to_string_lossy();
        if let Some(stripped) = s.strip_suffix(".uorc") {
            PathBuf::from(stripped)
        } else {
            PathBuf::from(format!("{s}.out"))
        }
    });

    let start = Instant::now();
    let stats =
        uor_compress::pipeline::decompress::decompress_file(&args.input, &output, args.verify)
            .with_context(|| format!("failed to decompress {}", args.input.display()))?;
    let elapsed = start.elapsed();

    println!(
        "Decompressed {} -> {}",
        args.input.display(),
        output.display()
    );
    println!("  Original size:    {} bytes", stats.original_size);
    println!("  Chunks processed: {}", stats.chunks_decompressed);
    println!("  Lossy:            {}", stats.is_lossy);
    println!("  Time:             {:.2?}", elapsed);

    Ok(())
}

pub fn inspect(args: InspectArgs) -> Result<()> {
    let file = std::fs::File::open(&args.archive)
        .with_context(|| format!("failed to open {}", args.archive.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)
        .with_context(|| format!("failed to read archive {}", args.archive.display()))?;

    if args.manifest {
        match archive.read_manifest()? {
            Some(manifest) => println!("{manifest}"),
            None => println!("No manifest in archive."),
        }
        return Ok(());
    }

    let h = &archive.header;
    println!("UOR Archive: {}", args.archive.display());
    println!("  Version:       {}", h.version);
    println!("  Original size: {} bytes", h.original_size);
    println!("  Lossy:         {}", h.is_lossy());
    println!("  Has manifest:  {}", h.has_manifest());
    println!("  Chunks:        {} unique", h.chunk_count);
    println!("  File map:      {} entries", h.file_map_count);

    if args.chunks {
        println!("\nChunks:");
        for (i, entry) in archive.toc.iter().enumerate() {
            println!(
                "  [{i:4}] {} {:?} orig={} comp={} ratio={:.2}x",
                entry.chunk_id,
                entry.backend,
                entry.original_size,
                entry.compressed_size,
                if entry.compressed_size > 0 {
                    entry.original_size as f64 / entry.compressed_size as f64
                } else {
                    0.0
                }
            );
        }
    }

    Ok(())
}

pub fn verify(args: VerifyArgs) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let temp_output = temp_dir.path().join("verify_output");

    let stats = uor_compress::pipeline::decompress::decompress_file(
        &args.archive,
        &temp_output,
        true, // verify = true
    )
    .with_context(|| format!("verification failed for {}", args.archive.display()))?;

    println!("Archive verified: {}", args.archive.display());
    println!("  Original size:    {} bytes", stats.original_size);
    println!("  Chunks verified:  {}", stats.chunks_decompressed);
    println!("  Integrity:        OK");

    Ok(())
}

pub fn bench(args: BenchArgs) -> Result<()> {
    use uor_compress::pipeline::compress::compress_file;

    let temp_dir = tempfile::tempdir()?;

    let levels = [
        ("fast", CompressionLevel::Fast),
        ("default", CompressionLevel::Default),
        ("best", CompressionLevel::Best),
    ];

    println!("Benchmarking: {}", args.input.display());
    println!("{:<10} {:>12} {:>8} {:>12} {:>12}", "Level", "Compressed", "Ratio", "Compress", "Decompress");
    println!("{}", "-".repeat(58));

    for (name, level) in &levels {
        let output = temp_dir.path().join(format!("bench_{name}.uorc"));
        let config = CompressConfig {
            level: *level,
            emit_manifest: false,
            emit_certificates: false,
            ..Default::default()
        };

        let start = Instant::now();
        let stats = compress_file(&args.input, &output, &config)?;
        let compress_time = start.elapsed();

        let decomp_output = temp_dir.path().join(format!("bench_{name}.out"));
        let start = Instant::now();
        uor_compress::pipeline::decompress::decompress_file(&output, &decomp_output, false)?;
        let decompress_time = start.elapsed();

        println!(
            "{:<10} {:>12} {:>7.2}x {:>12.2?} {:>12.2?}",
            name, stats.compressed_size, stats.ratio(), compress_time, decompress_time
        );
    }

    Ok(())
}
