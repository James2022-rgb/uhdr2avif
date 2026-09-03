#![cfg(feature = "avif")]

use std::io::Write;

use log::trace;
use rav1e::prelude::*;

use crate::pixel::FloatImageContent;

pub fn write_hdr10_linear_pixels_to_avif<W: Write>(
    writer: &mut W,
    width: usize,
    height: usize,
    content: &FloatImageContent,
    exif: Option<&[u8]>,
) -> std::io::Result<()> {
    trace!("Converting {}x{} HDR pixels to 10-bit YCbCr", width, height);
    let mut ycbcr_pixels: Vec<[u16; 3]> = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let pixel = content.get_at(x, y);

            let [r, g, b] = pixel.rgb();

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

            ycbcr_pixels.push([
                (y * 1023.0).round() as u16,
                (cb * 1023.0).round() as u16,
                (cr * 1023.0).round() as u16,
            ]);
        }
    }

    trace!("YCbCr conversion complete: {} pixels", ycbcr_pixels.len());
    write_hdr10_ycbcr_pixels_to_avif(writer, width, height, &ycbcr_pixels, exif)
}

/// - `pixels`: A slice of HDR10 pixels, each represented as an array of 3 `u16`` values (Y', Cb, Cr).
///   The values MUST be in the range [0, 1023].
pub fn write_hdr10_ycbcr_pixels_to_avif<W: Write>(
    writer: &mut W,
    width: usize,
    height: usize,
    ycbcr_pixels: &[[u16; 3]],
    exif: Option<&[u8]>,
) -> std::io::Result<()> {
    trace!("Starting 10-bit AV1 encoding for {}x{} image", width, height);
    let av1_data = encode_av1_still_10bit(width, height, ycbcr_pixels)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    trace!("AV1 encoding complete: {} bytes", av1_data.len());

    let mut aviffy = avif_serialize::Aviffy::new();
    aviffy.transfer_characteristics(avif_serialize::constants::TransferCharacteristics::Smpte2084);
    aviffy.color_primaries(avif_serialize::constants::ColorPrimaries::Bt2020);
    aviffy.matrix_coefficients(avif_serialize::constants::MatrixCoefficients::Bt2020Ncl);
    if let Some(exif) = exif {
        aviffy.set_exif(exif.to_vec());
    }
    trace!("Writing AVIF container");
    aviffy.write(writer, &av1_data, None, width as u32, height as u32, 10)?;
    trace!("AVIF container written");

    Ok(())
}

fn encode_av1_still_10bit(
    width: usize,
    height: usize,
    ycbcr_pixels: &[[u16; 3]],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    trace!("Creating rav1e context: 10-bit 4:4:4, quantizer 0, speed preset 4");
    let cfg = Config::new().with_encoder_config(EncoderConfig {
        width,
        height,
        bit_depth: 10,
        chroma_sampling: ChromaSampling::Cs444,
        chroma_sample_position: ChromaSamplePosition::Unknown,
        pixel_range: PixelRange::Full,
        color_description: Some(ColorDescription {
            color_primaries: ColorPrimaries::BT2020,
            transfer_characteristics: TransferCharacteristics::SMPTE2084,
            matrix_coefficients: MatrixCoefficients::BT2020NCL,
        }),
        still_picture: true,
        quantizer: 0,
        min_quantizer: 0,
        tune: Tune::Psychovisual,
        time_base: Rational { num: 1, den: 1 },
        sample_aspect_ratio: Rational { num: 1, den: 1 },
        speed_settings: SpeedSettings::from_preset(4),
        ..Default::default()
    });

    let mut ctx: Context<u16> = cfg.new_context()?;
    let mut frame = ctx.new_frame();

    // Populate frame planes (Y, Cb, Cr) from interleaved YCbCr pixels.
    {
        let mut planes = frame.planes.iter_mut();
        let mut y_plane = planes.next().unwrap().mut_slice(Default::default());
        let mut u_plane = planes.next().unwrap().mut_slice(Default::default());
        let mut v_plane = planes.next().unwrap().mut_slice(Default::default());

        let mut pixel_iter = ycbcr_pixels.iter();
        for ((y_row, u_row), v_row) in y_plane
            .rows_iter_mut()
            .zip(u_plane.rows_iter_mut())
            .zip(v_plane.rows_iter_mut())
            .take(height)
        {
            for ((y, u), v) in y_row[..width]
                .iter_mut()
                .zip(&mut u_row[..width])
                .zip(&mut v_row[..width])
            {
                let px = pixel_iter.next().expect("not enough pixels");
                *y = px[0];
                *u = px[1];
                *v = px[2];
            }
        }
    }

    ctx.send_frame(frame)?;
    ctx.flush();
    trace!("AV1 frame submitted; draining encoder packets");

    let mut av1_data = Vec::new();
    let mut packet_count = 0;
    loop {
        match ctx.receive_packet() {
            Ok(packet) => {
                av1_data.extend_from_slice(&packet.data);
                packet_count += 1;
            }
            Err(EncoderStatus::Encoded) => {
                trace!("AV1 encoder finished after {} packets (Encoded)", packet_count);
                break;
            }
            Err(EncoderStatus::LimitReached) => {
                trace!("AV1 encoder finished after {} packets (LimitReached)", packet_count);
                break;
            }
            Err(e) => return Err(Box::new(e)),
        }
    }
    Ok(av1_data)
}

/// SMPTE ST.2084 PQ (Perceptual Quantizer) EOTF^-1:
/// PQ is actually defined by the EOTF. This is its inverse, divided by 10,000.
///
/// Also in [_Rec. ITU-R BT.2100-3_](https://www.itu.int/rec/R-REC-BT.2100-3-202502-I/en).
///
/// - `color`: Normalized color [0, 1] to map non-linearly to [0, 1].
fn st2084_oetf(color: f32) -> f32 {
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
