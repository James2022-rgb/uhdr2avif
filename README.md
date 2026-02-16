
<div align="center">

# `uhdr2avif`

**CLI tool and core library written in 🦀Rust for converting HDR gain map images to HDR10 AVIF**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

## 🖥️ Platform Support

Supports all platforms compatible with the [`lcms2`](https://crates.io/crates/lcms2) Rust crate, which relies on the native [Little CMS](https://www.littlecms.com/) C library.

CLI tool binary [releases](https://github.com/James2022-rgb/uhdr2avif/releases) are provided for:
- Windows: `x86_64-pc-windows-msvc`
- macOS ARM: `aarch64-apple-darwin`
- macOS Intel: `x86_64-apple-darwin`

## ~~⚠️ Work-in-progress~~
~~This is a work-in-progress PoC, and is currently a lot slower that it could be~~.

It mostly does its originally intended job now, and while there are many potentials for optimization, the AV1 encoding in `rav1e` seems to take up the vast majority of the CLI binary's runtime.

## 📥 Supported Input Formats

| Format | Description | Feature flag |
|--------|-------------|-------------|
| [Ultra HDR](https://developer.android.com/media/platform/hdr-image-format) JPEG | Google's HDR gain map format embedded in standard JPEG | _(always enabled)_ |
| [Apple HDR](https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos) HEIC | Apple's HDR gain map format used by iPhone | `heif` |

The input format is auto-detected from the file extension (`.jpg`/`.jpeg` or `.heic`), or can be explicitly specified with `--format`.

## 📦 The CLI tool binary

`uhdr2avif` is a command-line tool that converts HDR gain map images to AVIF, preserving HDR (High Dynamic Range) with optional tonemapping controls.

### Command line options

#### Input
- Accepts a file path via `--input` / `-i`, or raw data via `--stdin`.
- If `--input` is not provided, the program reads from stdin only if `--stdin` is explicitly set.
- `--format` / `-f` can be used to explicitly specify the input format (`jpeg` or `heic`). If omitted, the format is detected from the file extension.

#### Output
- Writes to a file path specified via `--output` / `-o`, or to stdout if `--stdout` is set.
- If `--output` is not provided, the program writes to stdout only if `--stdout` is explicitly set.

#### HDR parameters
- `--max-display-boost`, defaulting to `10`, specifies maximum available boost supported by a display, as described in [Ultra HDR Image Format v1.1](https://developer.android.com/media/platform/hdr-image-format#definitions). This constant determines the strength of the Ultra HDR _HDR rendition_.
- `--target-sdr-white-level`, defaulting to `80`, specifies the SDR white level in nits that the RGB value (1, 1, 1) should map to. The _HDR rendition_ value is scaled accordingly.

`--max-display-boost` is required to compute what is called _weight factor_, which determines how much of the gain map to apply based on the target display's HDR capacity.

Since PQ (Perceptual Quantizer) encodes absolute luminance, we need a way to map the computed _HDR rendition_ value to it.
`--target-sdr-white-level` is used here to determine the absolute luminance value in nits the RGB value (1, 1, 1) should map to.

#### EXIF metadata
By default, EXIF metadata from the input file is preserved in the output AVIF. The thumbnail IFD (IFD1) is stripped to avoid including stale SDR thumbnail data.

Use `--no-preserve-exif` to omit all EXIF metadata from the output.

#### The help `-h, --help` option

The output of `uhdr2avif -h` is quoted verbatim here:
```
Usage: uhdr2avif [OPTIONS]

Options:
  -i, --input <INPUT_FILE_PATH>
          The input file to process. If not specified, the program will read from stdin if `--stdin` is enabled
      --stdin
          Read input from stdin if true
  -f, --format <FORMAT>
          Input format. If omitted, detected from file extension [possible values: jpeg, heic]
  -o, --output <OUTPUT_FILE_PATH>
          The output file to write to
      --stdout
          Write output to stdout if true. If not specified, the program will write to stdout if `--stdout` is provided
      --max-display-boost <MAX_DISPLAY_BOOST>
          The maximum available boost supported by a display, at a given point in time. This is a constant value that should be set based on the display's capabilities. This value is used to compute the boosted Ultra HDR "HDR rendition" value [default: 10]
      --target-sdr-white-level <TARGET_SDR_WHITE_LEVEL>
          The target SDR white level in nits to scale (1, 1, 1) to. The boosted Ultra HDR "HDR rendition" value is scaled by this value [default: 80]
      --no-preserve-exif
          Do not preserve EXIF metadata from the input in the output AVIF
  -h, --help
          Print help (see more with '--help')
  -V, --version
          Print version
```

> **Note:** The `heic` format option is only available when built with the `heif` feature (`cargo build -F heif`).
