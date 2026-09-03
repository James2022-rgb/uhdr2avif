#![cfg(feature = "exr")]

use exr::meta::attribute::Chromaticities;
use exr::prelude::*;

use crate::colorspace::ColorGamut;
use crate::pixel::FloatImageContent;

pub fn write_hdr10_linear_pixels_to_exr<W: std::io::Write + std::io::Seek>(
    writer: W,
    width: usize,
    height: usize,
    content: &FloatImageContent,
) -> std::io::Result<()> {
    let bt2020 = ColorGamut::bt2020();
    let primaries = bt2020.primaries();

    let chromaticities = Chromaticities {
        red: Vec2(primaries.red_xy()[0] as f32, primaries.red_xy()[1] as f32),
        green: Vec2(
            primaries.green_xy()[0] as f32,
            primaries.green_xy()[1] as f32,
        ),
        blue: Vec2(primaries.blue_xy()[0] as f32, primaries.blue_xy()[1] as f32),
        white: Vec2(
            bt2020.white_point_xy()[0] as f32,
            bt2020.white_point_xy()[1] as f32,
        ),
    };

    let mut image_attributes =
        ImageAttributes::new(IntegerBounds::from_dimensions((width, height)));
    image_attributes.chromaticities = Some(chromaticities);

    let channels = SpecificChannels::rgb(|Vec2(x, y)| {
        let pixel = content.get_at(x as usize, y as usize);
        (pixel.r(), pixel.g(), pixel.b())
    });

    let mut image = Image::from_channels((width, height), channels);
    image.attributes = image_attributes;

    image.layer_data.encoding.compression = Compression::PIZ;

    image
        .write()
        .to_unbuffered(writer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}
