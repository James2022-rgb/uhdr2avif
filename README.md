
<div align="center">

# `uhdr2avif`

**CLI tool and core library written in 🦀Rust for [Ultra HDR](https://developer.android.com/media/platform/hdr-image-format) JPEG to AVIF conversion**

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

## 📦 The CLI tool binary

`uhdr2avif` is a command-line tool that processes Ultra HDR JPEGs and converts them to AVIF, preserving HDR (High Dynamic Range) with optional tonemapping controls.

### Command line options

#### Input 
- Accepts a file path via `--input` / `-i`, or raw data via `--stdin`.
- If `--input` is not provided, the program reads from stdin only if `--stdin` is explicitly set.

#### Output
- Writes to a file path specified via `--output` / `-o`, or to stdout if `--stdout` is set.
- If `--output` is not provided, the program writes to stdout only if `--stdout` is explicitly set.

#### HDR parameters
- `--max-display-boost`, defaulting to `10`, specifies maximum available boost supported by a display, as described in [Ultra HDR Image Format v1.1](https://developer.android.com/media/platform/hdr-image-format#definitions). This constant determines the strength of the Ultra HDR _HDR rendition_.
- `--target-sdr-white-level`, defaulting to `80`, specifies the SDR white level in nits that the RGB value (1, 1, 1) should map to. The _HDR rendition_ value is scaled accordingly.

`--max-display-boost` is required to compute what is called _weight factor_, which determined how much of the gain map to apply based on the target display's HDR capacity.

Since PQ (Perceptual Quantizer) encodes absolute luminance, we need a way to map the computed _HDR rendition_ value to it.
`--target-sdr-white-level` is used here to determine the absolute luminance value in nits the RGB value (1, 1, 1) should map to.

#### The help `-h, --help` option

The output of `uhdr2avif -h` is quoted verbatim here:
```bash
-i, --input <INPUT_FILE_PATH>
        The input file to process. If not specified, the program will read from stdin if `--stdin` is enabled
    --stdin
        Read input from stdin if true
-o, --output <OUTPUT_FILE_PATH>
        The output file to write to
    --stdout
        Write output to stdout if true. If not specified, the program will write to stdout if `--stdout` is provided
    --max-display-boost <MAX_DISPLAY_BOOST>
        The maximum available boost supported by a display, at a given point in time. This is a constant value that shouldbe set based on the display's capabilities. This value is used to compute the boosted Ultra HDR "HDR rendition"value [default: 10]
    --target-sdr-white-level <TARGET_SDR_WHITE_LEVEL>
        The target SDR white level in nits to scale (1, 1, 1) to. The boosted Ultra HDR "HDR rendition" value is scaled bythis value [default: 80]
-h, --help
        Print help
-V, --version
        Print version
```
