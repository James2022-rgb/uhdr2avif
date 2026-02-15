
mod logging;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use log::trace;
use clap::Parser;

use libuhdr::UhdrConverter;
#[cfg(feature = "heif")]
use libuhdr::AppleHdrHeicConverter;

/// Luminance level in nits for sRGB (1, 1, 1) by Windows convention.
const WINDOWS_SDR_WHITE_LEVEL: f32 = 80.0f32;
const ASSUMED_DISPLAY_MAX_BRIGHTNESS :f32 = 800.0f32;

const DEFAULT_MAX_DISPLAY_BOOST: f32 = ASSUMED_DISPLAY_MAX_BRIGHTNESS / WINDOWS_SDR_WHITE_LEVEL;

const DEFAULT_TARGET_SDR_WHITE_LEVEL: f32 = WINDOWS_SDR_WHITE_LEVEL;

#[derive(Clone, clap::ValueEnum, Debug)]
enum InputFormat {
    /// Ultra HDR JPEG input.
    Jpeg,
    /// Apple HDR HEIC input.
    #[cfg(feature = "heif")]
    Heic,
}

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
    /// Input format. If omitted, detected from file extension.
    #[arg(short='f', long="format")]
    format: Option<InputFormat>,
    /// The output file to write to.
    #[arg(short='o', long="output")]
    output_file_path: Option<String>,
    /// Write output to stdout if true.
    /// If not specified, the program will write to stdout if `--stdout` is provided.
    #[arg(long="stdout", default_value_t = false)]
    stdout: bool,
    /// The maximum available boost supported by a display, at a given point in time.
    /// This is a constant value that should be set based on the display's capabilities.
    /// This value is used to compute the boosted Ultra HDR "HDR rendition" value.
    #[arg(long="max-display-boost", default_value_t = DEFAULT_MAX_DISPLAY_BOOST)]
    max_display_boost: f32,
    /// The target SDR white level in nits to scale (1, 1, 1) to.
    /// The boosted Ultra HDR "HDR rendition" value is scaled by this value.
    #[arg(long="target-sdr-white-level", default_value_t = DEFAULT_TARGET_SDR_WHITE_LEVEL)]
    target_sdr_white_level: f32,
}

/// Detect the input format from the file extension.
fn detect_input_format(file_path: &str) -> Result<InputFormat, String> {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("jpg") | Some("jpeg") => Ok(InputFormat::Jpeg),
        #[cfg(feature = "heif")]
        Some("heic") => Ok(InputFormat::Heic),
        Some(ext) => {
            #[cfg(not(feature = "heif"))]
            let supported = "jpg, jpeg";
            #[cfg(feature = "heif")]
            let supported = "jpg, jpeg, heic";
            Err(format!(
                "Unsupported file extension '.{}'. Supported extensions: {}",
                ext,
                supported,
            ))
        }
        None => Err("Input file has no extension. Use --format to specify the input format.".to_string()),
    }
}

fn main() -> Result<(), String> {
    logging::LoggingConfig::default().apply();

    let args = Args::parse();

    let input_format = if let Some(format) = args.format {
        format
    } else {
        let file_path = args.input_file_path.as_deref()
            .ok_or("Cannot detect input format without a file path. Use --format to specify the input format.")?;
        detect_input_format(file_path)?
    };

    let mut reader: Box<dyn Read> = if let Some(ref input_file_path) = args.input_file_path {
        trace!("Reading input from file: {}", input_file_path);
        Box::new(File::open(input_file_path).map_err(|e| format!("Failed to open input file: {}", e))?)
    } else if args.stdin {
        trace!("Reading input from stdin");
        Box::new(std::io::stdin())
    } else {
        return Err("No input file specified and stdin not enabled".to_string());
    };

    let mut writer: Box<dyn Write> = if let Some(output_file_path) = args.output_file_path {
        trace!("Writing output to file: {}", output_file_path);
        Box::new(File::create(output_file_path).map_err(|e| format!("Failed to create output file: {}", e))?)
    } else if args.stdout {
        trace!("Writing output to stdout");
        Box::new(std::io::stdout())
    } else {
        return Err("No output file specified and stdout not enabled".to_string());
    };

    let target_sdr_white_level = args.target_sdr_white_level;

    match input_format {
        InputFormat::Jpeg => {
            let uhdr_converter = UhdrConverter::new(&mut reader, args.max_display_boost)
                .map_err(|e| format!("Failed to create UHDR converter: {}", e))?;

            uhdr_converter.convert_to_avif(&mut writer, target_sdr_white_level)
                .map_err(|e| format!("Failed to convert UHDR JPEG to AVIF: {}", e))?;
        }
        #[cfg(feature = "heif")]
        InputFormat::Heic => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to read input: {}", e))?;

            let converter = AppleHdrHeicConverter::new(&bytes)
                .map_err(|e| format!("Failed to create Apple HDR HEIC converter: {}", e))?;

            converter.convert_to_avif(&mut writer, target_sdr_white_level)
                .map_err(|e| format!("Failed to convert Apple HDR HEIC to AVIF: {}", e))?;
        }
    }

    Ok(())
}
