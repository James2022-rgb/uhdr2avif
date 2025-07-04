
mod logging;

use std::fs::File;
use std::io::{Read, Write};

use log::{trace, error};
use clap::Parser;

use libuhdr::UhdrConverter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input file to process.
    /// If not specified, the program will read from stdin if `--stdin` is enabled.
    #[arg(short='i', long="input")]
    input_file_path: Option<String>,
    /// Read input from stdin if true.
    #[arg(long="stdin", default_value_t = false)]
    stdin: bool,
    /// The output file to write to.
    #[arg(short='o', long="output")]
    output_file_path: Option<String>,
    /// Write output to stdout if true.
    /// If not specified, the program will write to stdout if `--stdout` is provided.
    #[arg(long="stdout", default_value_t = false)]
    stdout: bool,
}

fn main() -> Result<(), String> {
    /// Luminance level in nits for sRGB (1, 1, 1) by Windows convention.
    const WINDOWS_SDR_WHITE_LEVEL: f32 = 80.0f32;

    // FIXME: The maximum brightness of the display in nits.
    const ASSUMED_DISPLAY_MAX_BRIGHTNESS :f32 = 930.0f32;

    // FIXME: The maximum available boost supported by a display, at a given point in time.
    const MAX_DISPLAY_BOOST: f32 = ASSUMED_DISPLAY_MAX_BRIGHTNESS / WINDOWS_SDR_WHITE_LEVEL;

    // FIXME: Test value:
    const TARGET_SDR_WHITE_LEVEL: f32 = 240.0;

    logging::LoggingConfig::default().apply();

    let args = Args::parse();
    
    let mut reader : Box<dyn Read> = if let Some(input_file_path) = args.input_file_path {
        trace!("Reading input from file: {}", input_file_path);
        Box::new(File::open(input_file_path).map_err(|e| format!("Failed to open input file: {}", e))?)
    } else if args.stdin {
        trace!("Reading input from stdin");
        Box::new(std::io::stdin())
    } else {
        return Err("No input file specified and stdin not enabled".to_string());
    };

    let uhdr_converter = UhdrConverter::new(&mut reader, MAX_DISPLAY_BOOST)
        .map_err(|e| format!("Failed to create UHDR converter: {}", e))?;

    let mut writer: Box<dyn Write> = if let Some(output_file_path) = args.output_file_path {
        trace!("Writing output to file: {}", output_file_path);
        Box::new(File::create(output_file_path).map_err(|e| format!("Failed to create output file: {}", e))?)
    } else if args.stdout {
        trace!("Writing output to stdout");
        Box::new(std::io::stdout())
    } else {
        return Err("No output file specified and stdout not enabled".to_string());
    };

    uhdr_converter.convert_to_avif(&mut writer, TARGET_SDR_WHITE_LEVEL)
        .map_err(|e| format!("Failed to convert UHDR JPEG to AVIF: {}", e))?;
    
    Ok(())
}
