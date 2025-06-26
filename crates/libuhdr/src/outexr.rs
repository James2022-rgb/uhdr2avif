#![cfg(feature = "exr")]

use exr::prelude::*;
use exr::meta::attribute::Chromaticities;

use crate::colorspace::ColorGamut;

pub fn write_rgb_image_to_exr<F: Fn(usize, usize) -> (f32, f32, f32) + Sync>(
    filename: &str,
    width: usize,
    height: usize,
    color_gamut: &ColorGamut,
    f: F,
) -> std::io::Result<()> {
    let primaries = color_gamut.primaries();

    let chromaticities = Chromaticities {
        red: Vec2(primaries.red_xy()[0] as f32, primaries.red_xy()[1] as f32),
        green: Vec2(primaries.green_xy()[0] as f32, primaries.green_xy()[1] as f32),
        blue: Vec2(primaries.blue_xy()[0] as f32, primaries.blue_xy()[1] as f32),
        white: Vec2(color_gamut.white_point_xy()[0] as f32, color_gamut.white_point_xy()[1] as f32),
    };

    let mut image_attributes = ImageAttributes::new(IntegerBounds::from_dimensions((width, height)));  
    image_attributes.chromaticities = Some(chromaticities);

    let channels = SpecificChannels::rgb(|Vec2(x, y)| {
        f(x as usize, y as usize)
    });

    let mut image = Image::from_channels((width, height), channels);
    image.attributes = image_attributes;

    image.layer_data.encoding.compression = Compression::PIZ;

    image.write().to_file(filename).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    
    Ok(())
}
