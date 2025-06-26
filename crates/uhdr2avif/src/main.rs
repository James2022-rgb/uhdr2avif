
mod logging;

use std::fs::File;
use std::io::Read as _;

use libuhdr::UhdrJpeg;

use log::error;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input file to process.
    #[arg(short='i', long="input")]
    input_file_path: String,
    /// The output file to write.
    /// If not specified, the output will be written to stdout.
    #[arg(short='o', long="output")]
    output_file_path: Option<String>,
}

fn main() {
    logging::LoggingConfig::default().apply();

    let args = Args::parse();
    
    let Ok(content) = std::fs::read(&args.input_file_path) else {
        error!("Error reading input file: {}", args.input_file_path);
        std::process::exit(1);
    };

    let Ok(uhdr_jpeg) = UhdrJpeg::new_from_bytes(&content) else {
        error!("Error parsing input file as UHDR JPEG: {}", args.input_file_path);
        std::process::exit(1);
    };
}
