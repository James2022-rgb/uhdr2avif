
pub use crate::colorspace::{IccColorSpace, ColorGamut};
pub use crate::gainmap::GainMapMetadata;
pub use crate::jpeg::UhdrJpeg;
pub use crate::uhdr::UhdrBoostComputer;

pub mod colorspace;
pub mod gainmap;
pub mod jpeg;
pub mod uhdr;

#[cfg(feature = "avif")]
pub mod outavif;

mod mpf;
#[cfg(feature = "exr")]
mod outexr;
#[cfg(feature = "heif")]
mod outheif;
mod pixel;
mod tiff;

use std::io::{Read, Write};

use log::warn;

use crate::pixel::{FloatImageContent, FloatPixel};

#[derive(Clone)]
pub struct UhdrConverter {
    uhdr_jpeg: UhdrJpeg,
    gain_map_jpeg: UhdrJpeg,
    src_color_gamut: ColorGamut,
    uhdr_boost_computer: UhdrBoostComputer,
}

impl UhdrConverter {
    pub fn new<R: Read>(
        reader: &mut R,
        max_display_boost: f32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let jpeg_bytes = {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            bytes
        };
        let uhdr_jpeg = UhdrJpeg::new_from_bytes(&jpeg_bytes)
            .map_err(|e| format!("Failed to parse JPEG: {}", e))?;

        let gain_map_jpeg = uhdr_jpeg.extract_gain_map_jpeg(&jpeg_bytes)
            .ok_or_else(|| "Failed to extract gain map JPEG".to_string())?;
        let gain_map_jpeg_xmp_bytes = gain_map_jpeg.xmp_bytes()
            .ok_or_else(|| "Gain Map JPEG does not contain XMP metadata".to_string())?;
        let gain_map_metadata = GainMapMetadata::new_from_xmp_bytes(&gain_map_jpeg_xmp_bytes)
            .ok_or_else(|| "Failed to parse gain map metadata from XMP".to_string())?;

        let src_color_gamut = uhdr_jpeg.icc_color_space()
            .as_ref()
            .map(|icc| icc.color_gamut)
            .unwrap_or_else(|| {
                warn!("No ICC profile found, using default sRGB color gamut");
                ColorGamut::srgb()
            });
        
        let uhdr_boost_computer = UhdrBoostComputer::new(&gain_map_metadata, max_display_boost.log2());

        Ok(Self {
            uhdr_jpeg,
            gain_map_jpeg,
            src_color_gamut,
            uhdr_boost_computer,
        })
    }

    #[cfg(feature = "avif")]
    pub fn convert_to_avif<W: Write>(
        &self,
        writer: &mut W,
        target_sdr_white_level: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        const DST_COLOR_GAMUT: ColorGamut = ColorGamut::bt2020();

        let (width, height) = self.uhdr_jpeg.extent();

        let mut linear_pixels = FloatImageContent::with_extent(width, height);
        for y in 0..height {
            for x in 0..width {
                // RGB value after EOTF.
                let in_rgb: FloatPixel = self.uhdr_jpeg.fetch_pixel_linear(x, y).into();

                let gain_map_rgb: FloatPixel = {
                    let (u, v) = {
                        let texel_width = 1.0 / width as f32;
                        let texel_height = 1.0 / height as f32;

                        // Use texel center.
                        let u_offset = texel_width * 0.5;
                        let v_offset = texel_height * 0.5;
                        let u = texel_width * x as f32 + u_offset;
                        let v = texel_height * y as f32 + v_offset;

                        (u, v)
                    };

                    self.gain_map_jpeg.sample_bilinear(u, v)
                        .unwrap_or_else(|| panic!("Failed to sample gain map at ({}, {})", u, v))
                        .into()
                };

                let boosted = self.uhdr_boost_computer.compute_boosted(in_rgb, gain_map_rgb);

                // Map 1 to `target_sdr_white_level` nits.
                let scaled_boosted = boosted * target_sdr_white_level;

                let [r, g , b] = ColorGamut::convert(scaled_boosted.rgb(), &self.src_color_gamut, &DST_COLOR_GAMUT);

                linear_pixels.set_at(x, y, FloatPixel::from([r, g, b]));
            }
        }

        crate::outavif::write_hdr10_linear_pixels_to_avif(
            writer,
            width as usize,
            height as usize,
            &linear_pixels,
        ).map_err(|e| format!("Failed to write AVIF: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn it_works() {
        /// Luminance level in nits for sRGB (1, 1, 1) by Windows convention.
        const WINDOWS_SDR_WHITE_LEVEL: f32 = 80.0f32;

        // FIXME: The maximum brightness of the display in nits.
        const ASSUMED_DISPLAY_MAX_BRIGHTNESS :f32 = 930.0f32;

        // FIXME: The maximum available boost supported by a display, at a given point in time.
        const MAX_DISPLAY_BOOST: f32 = ASSUMED_DISPLAY_MAX_BRIGHTNESS / WINDOWS_SDR_WHITE_LEVEL;

        let manifest_dir = env!("CARGO_MANIFEST_DIR");

        let test_dir_path = Path::new(manifest_dir).join("..").join("..").join("test");

        let jpeg_file_paths: Vec<_> = std::fs::read_dir(test_dir_path)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                if entry.path().extension().map_or(false, |ext| ext == "jpg" || ext == "jpeg") {
                    Some(entry.path())
                } else {
                    None
                }
            })
            .collect();

        println!("JPEG files found: {:?}", jpeg_file_paths);

        for file_path in &jpeg_file_paths {
            let mut in_file = std::fs::File::open(file_path).unwrap();

            let uhdr_converter = crate::UhdrConverter::new(&mut in_file, MAX_DISPLAY_BOOST)
                .expect("Failed to create UHDR converter");

            let mut out_file = {
                let output_file_name = file_path.file_stem().unwrap().to_str().unwrap();
                let output_file_name = format!("{}.avif", output_file_name);

                std::fs::File::create(&output_file_name).unwrap()
            };
            
            uhdr_converter.convert_to_avif(&mut out_file)
                .expect("Failed to convert UHDR JPEG to AVIF");
        }
    }
}
