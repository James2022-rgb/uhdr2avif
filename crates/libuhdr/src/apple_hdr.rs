
#[cfg(feature = "heif")]
mod heic;

use heic::Heic;

pub struct AppleHdrHeicConverter {
    heic: Heic,
}

impl AppleHdrHeicConverter {
    pub fn new(heic_bytes: &[u8]) -> Result<Self, String> {
        let heic = Heic::new_from_bytes(heic_bytes)?;
        Ok(Self { heic })
    }

    /// Compute BT.2020 linear nits pixels from the Apple HDR HEIC.
    fn compute_hdr_linear_pixels(&self, target_sdr_white_level: f32) -> (usize, usize, crate::pixel::FloatImageContent) {
        use crate::colorspace::ColorGamut;
        use crate::pixel::{FloatPixel, FloatImageContent};

        const DST_COLOR_GAMUT: ColorGamut = ColorGamut::bt2020();

        let sdr_image = self.heic.sdr_image();
        let gainmap = self.heic.gainmap();
        let headroom = self.heic.headroom();
        let src_color_gamut = self.heic.src_color_gamut();

        let width = sdr_image.width();
        let height = sdr_image.height();

        let mut linear_pixels = FloatImageContent::with_extent(width, height);
        for y in 0..height {
            for x in 0..width {
                // SDR pixel is already linear (sRGB EOTF applied at construction time).
                let sdr = sdr_image.get_at(x, y);

                // Sample the gain map using bilinear filtering.
                let gain = {
                    let texel_width = 1.0 / width as f32;
                    let texel_height = 1.0 / height as f32;

                    // Use texel center.
                    let u = texel_width * x as f32 + texel_width * 0.5;
                    let v = texel_height * y as f32 + texel_height * 0.5;

                    gainmap.sample_bilinear(u, v)
                };

                // Apple HDR boost: hdr = sdr * (1 + (headroom - 1) * gainmap)
                let boost = 1.0 + (headroom - 1.0) * gain.r();
                let boosted = sdr * boost;

                // Scale to target SDR white level.
                let scaled = boosted * target_sdr_white_level;

                // Convert from source gamut to BT.2020.
                let [r, g, b] = ColorGamut::convert(scaled.rgb(), src_color_gamut, &DST_COLOR_GAMUT);

                linear_pixels.set_at(x, y, FloatPixel::from([r, g, b]));
            }
        }

        (width, height, linear_pixels)
    }

    #[cfg(feature = "avif")]
    pub fn convert_to_avif<W: std::io::Write>(
        &self,
        writer: &mut W,
        target_sdr_white_level: f32,
        preserve_exif: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height, linear_pixels) = self.compute_hdr_linear_pixels(target_sdr_white_level);

        let exif = if preserve_exif {
            self.heic.exif_data_block().map(|raw| {
                let mut block = raw.to_vec();
                // ExifDataBlock: [4-byte offset][TIFF data]
                // Apply strip_ifd1 to the TIFF portion.
                if block.len() > 4 {
                    let offset = u32::from_be_bytes(block[0..4].try_into().unwrap()) as usize;
                    crate::tiff::strip_ifd1(&mut block[4 + offset..]);
                }
                block
            })
        } else {
            None
        };

        crate::outavif::write_hdr10_linear_pixels_to_avif(
            writer,
            width,
            height,
            &linear_pixels,
            exif.as_deref(),
        ).map_err(|e| format!("Failed to write AVIF: {}", e))?;

        Ok(())
    }

    #[cfg(feature = "exr")]
    pub fn convert_to_exr<W: std::io::Write + std::io::Seek>(
        &self,
        writer: W,
        target_sdr_white_level: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height, linear_pixels) = self.compute_hdr_linear_pixels(target_sdr_white_level);

        crate::outexr::write_hdr10_linear_pixels_to_exr(
            writer,
            width,
            height,
            &linear_pixels,
        ).map_err(|e| format!("Failed to write EXR: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_convert_heic_to_avif() {
        const TARGET_SDR_WHITE_LEVEL: f32 = 80.0;

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let test_dir_path = Path::new(manifest_dir).join("..").join("..").join("test");

        let heic_file_paths: Vec<_> = std::fs::read_dir(&test_dir_path)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("heic")) {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        println!("HEIC files found: {:?}", heic_file_paths);
        assert!(!heic_file_paths.is_empty(), "No .heic files found in test directory");

        for file_path in &heic_file_paths {
            println!("Converting: {}", file_path.display());

            let heic_bytes = std::fs::read(file_path).unwrap();
            let converter = AppleHdrHeicConverter::new(&heic_bytes)
                .expect("Failed to create AppleHdrHeicConverter");

            let output_file_name = format!("{}.avif", file_path.file_stem().unwrap().to_str().unwrap());
            let mut out_file = std::fs::File::create(&output_file_name).unwrap();

            converter.convert_to_avif(&mut out_file, TARGET_SDR_WHITE_LEVEL, true)
                .expect("Failed to convert HEIC to AVIF");

            println!("Written: {}", output_file_name);
        }
    }
}
