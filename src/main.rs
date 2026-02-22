use anyhow::Result;
use clap::{Parser, Subcommand};

mod cli;

#[derive(Parser)]
#[command(name = "uor-compress", about = "UOR/PRISM-based file compression")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress a file
    #[command(alias = "c")]
    Compress(cli::CompressArgs),

    /// Decompress a .uorc file
    #[command(alias = "d")]
    Decompress(cli::DecompressArgs),

    /// Show archive metadata and statistics
    #[command(alias = "i")]
    Inspect(cli::InspectArgs),

    /// Verify archive integrity
    #[command(alias = "v")]
    Verify(cli::VerifyArgs),

    /// Benchmark compression on a file
    #[command(alias = "b")]
    Bench(cli::BenchArgs),
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress(args) => cli::compress(args),
        Commands::Decompress(args) => cli::decompress(args),
        Commands::Inspect(args) => cli::inspect(args),
        Commands::Verify(args) => cli::verify(args),
        Commands::Bench(args) => cli::bench(args),
    }
}
