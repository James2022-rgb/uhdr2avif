#![cfg(feature = "avif")]

use ravif::*;
use rav1e::color::ColorPrimaries;
use rav1e::color::TransferCharacteristics as Rav1eTransferCharacteristics;

use crate::colorspace::ColorGamut;

/// - `f`: A function that takes pixel coordinates (x, y) and returns a tuple of (r, g, b) HDR values in linear nits (cd/m^2).
pub fn write_rgb_image_to_avif<F: Fn(usize, usize) -> (f32, f32, f32) + Sync>(
    filename: &str,
    width: usize,
    height: usize,
    color_gamut: &ColorGamut,
    f: F,
) -> std::io::Result<()> {
    use std::io::Write as _;

    const TRANSFER_CHARACTERISTICS: Rav1eTransferCharacteristics = Rav1eTransferCharacteristics::SMPTE2084;
    const COLOR_PRIMARIES: ColorPrimaries = ColorPrimaries::BT2020;
    const MATRIX_COEFFICIENTS: MatrixCoefficients = MatrixCoefficients::BT2020NCL;

    const COLOR_GAMUT: ColorGamut = ColorGamut::bt2020();

    let mut pixels: Vec<[u16; 3]> = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = f(x as usize, y as usize);

            let [r, g , b] = ColorGamut::convert(&[r, g, b], &color_gamut, &COLOR_GAMUT);

            // Clamp the values to the range [0, 10000] for HDR10 PQ.
            let r = r.clamp(0.0, 10000.0);
            let g = g.clamp(0.0, 10000.0);
            let b = b.clamp(0.0, 10000.0);

            // Normalize to [0, 1] for the HDR10 PQ OETF.
            let r = st2084_oetf(r / 10000.0);
            let g = st2084_oetf(g / 10000.0);
            let b = st2084_oetf(b / 10000.0);

            // Rec. ITU-R BT.2100-3,
            // "Non-Constant Luminance Y'C'bC'r signal format", Derivation of Y', Derivation of colour difference signals
            let y = 0.2627 * r + 0.6780 * g + 0.0593 * b;
            let cb = (b - y) / 1.8814 + 0.5;
            let cr = (r - y) / 1.4746 + 0.5;

            pixels.push([
                (y * 1023.0).round() as u16,
                (cb * 1023.0).round() as u16,
                (cr * 1023.0).round() as u16,
            ]);
        }
    }

    let res = Encoder::new()
        .with_quality(100.0)
        .with_speed(4)
        .encode_raw_plane_10_with_params(width, height, pixels.iter().cloned(), None::<[_; 0]>, rav1e::color::PixelRange::Full, TRANSFER_CHARACTERISTICS, COLOR_PRIMARIES, MATRIX_COEFFICIENTS)
        .unwrap()
        ;

    let mut file = std::fs::File::create(filename)?;
    file.write_all(&res.avif_file)?;

    Ok(())
}

/// SMPTE ST.2084 PQ (Perceptual Quantizer) EOTF^-1:
/// PQ is actually defined by the EOTF. This is its inverse, divided by 10,000.
/// 
/// Also in [_Rec. ITU-R BT.2100-3_](https://www.itu.int/rec/R-REC-BT.2100-3-202502-I/en).
///
/// - `color`: Normalized color [0, 1] to map non-linearly to [0, 1].
fn st2084_oetf(color: f32) -> f32
{
    const M1: f32 = 2610.0 / 16384.0;
    const M2: f32 = 2523.0 / 4096.0 * 128.0;
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 4096.0 * 32.0;
    const C3: f32 = 2392.0 / 4096.0 * 32.0;

    let cp = f32::powf(color.abs(), M1);
    let numerator = C1 + C2 * cp;
    let denominator = 1.0 + C3 * cp;

    let color = f32::powf(numerator / denominator, M2);

    return color;
}
