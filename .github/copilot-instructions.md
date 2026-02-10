# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust CLI tool and library for converting Ultra HDR JPEGs to HDR10 AVIF. Ultra HDR is Google's format for embedding HDR gain maps in standard JPEG files. This tool extracts the gain map, computes the HDR rendition, and encodes to 10-bit PQ AVIF in BT.2020 color space.

## Language

All code comments MUST be written in English.

## Build Commands

```bash
# Build the CLI tool (release)
cargo build --release -p uhdr2avif

# Build with all features
cargo build --release

# Run tests
cargo test

# Run a single test
cargo test --package libuhdr -- tests::it_works
```

## Architecture

### Workspace Structure

- **`crates/uhdr2avif`** - CLI binary using clap for argument parsing
- **`crates/libuhdr`** - Core library with optional feature flags:
  - `avif` (default) - AVIF output via ravif/rav1e
  - `exr` - EXR output support
  - `heif` - HEIF output support

### Core Processing Pipeline

1. **JPEG Parsing** (`jpeg.rs`) - Uses `zune-jpeg` to decode JPEG, extract XMP metadata, ICC profile, and MPF (Multi-Picture Format) data
2. **Gain Map Extraction** (`jpeg.rs:extract_gain_map_jpeg`) - Parses MPF to locate the embedded gain map JPEG within the Ultra HDR file
3. **Metadata Parsing** (`gainmap.rs`) - Extracts HDR parameters from XMP: gamma, gain_map_min/max, offset_sdr/hdr, hdr_capacity_min/max
4. **HDR Boost Computation** (`uhdr.rs:UhdrBoostComputer`) - Applies the Ultra HDR algorithm to compute boosted HDR values from SDR + gain map
5. **Color Space Conversion** (`colorspace.rs`) - Converts from source gamut (typically sRGB from ICC profile) to BT.2020
6. **AVIF Encoding** (`outavif.rs`) - Converts linear RGB to PQ-encoded Y'CbCr and writes 10-bit HDR10 AVIF

### Key Types

- `UhdrConverter` (`lib.rs`) - Main orchestrator that ties together parsing, boost computation, and output
- `UhdrJpeg` (`jpeg.rs`) - Represents a decoded JPEG with pixel access and bilinear sampling
- `UhdrBoostComputer` (`uhdr.rs`) - Precomputes gain map parameters and applies the HDR boost formula
- `GainMapMetadata` (`gainmap.rs`) - Ultra HDR XMP metadata structure
- `ColorGamut` (`colorspace.rs`) - Represents color primaries and white point with gamut conversion

### External Dependencies Note

The project uses a forked version of `ravif` with a custom `encode_raw_plane_10_with_params` method for HDR10 encoding with explicit color parameters. The `zune-jpeg` dependency requires a specific git revision that supports `multi_picture_information` for MPF parsing.
