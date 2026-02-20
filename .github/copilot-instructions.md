# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust CLI tool and library for converting HDR gain map images to HDR10 AVIF or OpenEXR. Supports two input formats:

- **Ultra HDR JPEG** - Google's format for embedding HDR gain maps in standard JPEG files
- **Apple HDR HEIC** - Apple's HDR gain map format used by iPhone (requires `heif` feature)

The tool extracts the gain map, computes the HDR rendition, and encodes to either 10-bit PQ AVIF or BT.2020 linear float OpenEXR (nits).

## Language

All code comments MUST be written in English.

## Build Commands

```bash
# Build the CLI tool (release)
cargo build --release -p uhdr2avif

# Build with HEIC support
cargo build --release -p uhdr2avif -F heif

# Run tests
cargo test

# Run a single test
cargo test --package libuhdr -- tests::it_works

# Run Apple HDR HEIC tests
cargo test -F avif,heif --package libuhdr --lib -- apple_hdr
```

## Architecture

### Workspace Structure

- **`crates/uhdr2avif`** - CLI binary using clap for argument parsing
  - `exr` (default) - Enables OpenEXR output support (forwards to `libuhdr/exr`)
  - `heif` feature - Enables Apple HDR HEIC input support (forwards to `libuhdr/heif`)
- **`crates/libuhdr`** - Core library with optional feature flags:
  - `avif` (default) - AVIF output via rav1e/avif-serialize
  - `exr` - OpenEXR output support via exr crate
  - `heif` - Apple HDR HEIC input support via libheif-rs

### Ultra HDR JPEG Pipeline

1. **JPEG Parsing** (`uhdr/jpeg.rs`) - Uses `zune-jpeg` to decode JPEG, extract XMP metadata, ICC profile, and MPF (Multi-Picture Format) data
2. **Gain Map Extraction** (`uhdr/jpeg.rs:extract_gain_map_jpeg`) - Parses MPF to locate the embedded gain map JPEG within the Ultra HDR file
3. **Metadata Parsing** (`uhdr/gainmap.rs`) - Extracts HDR parameters from XMP: gamma, gain_map_min/max, offset_sdr/hdr, hdr_capacity_min/max
4. **HDR Boost Computation** (`uhdr.rs:UhdrBoostComputer`) - Applies the Ultra HDR algorithm to compute boosted HDR values from SDR + gain map
5. **Color Space Conversion** (`colorspace.rs`) - Converts from source gamut (typically sRGB from ICC profile) to BT.2020
6. **Output Encoding**:
   - **AVIF** (`outavif.rs`) - Converts linear RGB to PQ-encoded Y'CbCr, encodes AV1 via rav1e, and writes 10-bit HDR10 AVIF with EXIF passthrough via avif-serialize
   - **EXR** (`outexr.rs`, `exr` feature) - Writes BT.2020 linear float RGB (nits) with chromaticity metadata and PIZ compression

### Apple HDR HEIC Pipeline (`heif` feature)

1. **HEIC Parsing** (`apple_hdr/heic.rs`) - Uses `libheif-rs` to decode HEIC, extract SDR image (linearized with sRGB EOTF), gain map (Rec.709 EOTF), and color gamut (NCLX/ICC)
2. **Headroom Extraction** (`apple_hdr/heic.rs`) - Parses Apple MakerNote from EXIF (tags 0x0021/0x0030) via the TIFF IFD parser (`tiff.rs`) to compute HDR headroom
3. **HDR Boost** (`apple_hdr.rs:AppleHdrHeicConverter`) - Applies `sdr * (1 + (headroom - 1) * gainmap)`, converts gamut to BT.2020, encodes to AVIF or EXR

### Key Types

- `UhdrConverter` (`uhdr.rs`) - Ultra HDR JPEG orchestrator: parsing, boost computation, and output
- `AppleHdrHeicConverter` (`apple_hdr.rs`) - Apple HDR HEIC orchestrator: parsing, boost, and output (`heif` feature)
- `UhdrJpeg` (`uhdr/jpeg.rs`) - Represents a decoded JPEG with pixel access and bilinear sampling
- `UhdrBoostComputer` (`uhdr.rs`) - Precomputes gain map parameters and applies the HDR boost formula
- `GainMapMetadata` (`uhdr/gainmap.rs`) - Ultra HDR XMP metadata structure
- `Heic` (`apple_hdr/heic.rs`) - Parsed Apple HDR HEIC: SDR image, gain map, headroom, color gamut
- `ColorGamut` (`colorspace.rs`) - Represents color primaries and white point with gamut conversion
- `FloatImageContent` / `FloatPixel` (`pixel.rs`) - Linear float pixel storage with bilinear sampling

### External Dependencies Note

AVIF encoding uses `rav1e` (AV1 encoder) and `avif-serialize` (AVIF container serialization) directly, without the `ravif` wrapper. This allows direct control over EXIF passthrough via `avif_serialize::Aviffy::set_exif()`. EXR encoding uses the `exr` crate; its writer requires `Write + Seek` so stdout is not supported for EXR output. The `zune-jpeg` dependency requires a specific git revision that supports `multi_picture_information` for MPF parsing. The `libheif-rs` dependency uses a specific git revision for HEIC decoding.
