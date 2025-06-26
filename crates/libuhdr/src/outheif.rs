#![cfg(feature = "heif")]

use libheif_rs::{
    Channel, RgbChroma, ColorSpace, CompressionFormat,
    EncoderQuality, HeifContext, Image, Result, LibHeif
};


use crate::colorspace::ColorGamut;

pub fn write_rgb_image_to_heif<F: Fn(usize, usize) -> (f32, f32, f32) + Sync>(
    filename: &str,
    width: usize,
    height: usize,
    color_gamut: &ColorGamut,
    f: F,
) -> std::io::Result<()> {
    let width = width as u32;
    let height = height as u32;

    let mut image = Image::new(width, height, ColorSpace::Rgb(RgbChroma::HdrRgbLe)).unwrap();

    image.create_plane(Channel::Interleaved, width, height, 10);

    let planes = image.planes_mut();
    let plane = planes.interleaved.unwrap();
    let stride = plane.stride;
    let data = plane.data;
    
    for y in 0..height {
        let mut row_start = stride * y as usize;
        for x in 0..width {
            let (r, g, b) = f(x as usize, y as usize);
            
            let r = (r * 1023.0).round() as u16;
            let g = (g * 1023.0).round() as u16;
            let b = (b * 1023.0).round() as u16;

            data[row_start + 0 .. row_start + 2].copy_from_slice(&r.to_le_bytes());
            data[row_start + 2 .. row_start + 4].copy_from_slice(&g.to_le_bytes());
            data[row_start + 4 .. row_start + 6].copy_from_slice(&b.to_le_bytes());
        }
    }

    // Encode image and save it into file.
    let lib_heif = LibHeif::new();
    let mut context = HeifContext::new().unwrap();
    let mut encoder = lib_heif.encoder_for_format(CompressionFormat::Hevc).unwrap();
    encoder.set_quality(EncoderQuality::Lossy(100)).unwrap();
    context.encode_image(&image, &mut encoder, None).unwrap();
    context.write_to_file(filename).unwrap();

    Ok(())
}
