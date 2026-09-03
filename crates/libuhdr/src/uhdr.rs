mod gainmap;
mod jpeg;
mod mpf;

use std::io::{Read, Write};

use indicatif::{ProgressBar, ProgressStyle};
use log::warn;

use crate::colorspace::ColorGamut;
use crate::pixel::{FloatImageContent, FloatPixel};
use gainmap::GainMapMetadata;
use jpeg::UhdrJpeg;

#[derive(Clone)]
pub struct UhdrConverter {
    uhdr_jpeg: UhdrJpeg,
    gain_map_jpeg: UhdrJpeg,
    src_color_gamut: ColorGamut,
    uhdr_boost_computer: UhdrBoostComputer,
}

#[derive(Debug, Clone, Copy)]
pub struct UhdrBoostComputer {
    inv_gamma: FloatPixel,
    gain_map_min: FloatPixel,
    gain_map_max: FloatPixel,
    offset_sdr: FloatPixel,
    offset_hdr: FloatPixel,
    weight_factor: f32,
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

        let gain_map_jpeg = uhdr_jpeg
            .extract_gain_map_jpeg(&jpeg_bytes)
            .ok_or_else(|| "Failed to extract gain map JPEG".to_string())?;
        let gain_map_jpeg_xmp_bytes = gain_map_jpeg
            .xmp_bytes()
            .ok_or_else(|| "Gain Map JPEG does not contain XMP metadata".to_string())?;
        let gain_map_metadata = GainMapMetadata::new_from_xmp_bytes(&gain_map_jpeg_xmp_bytes)
            .ok_or_else(|| "Failed to parse gain map metadata from XMP".to_string())?;

        let src_color_gamut = uhdr_jpeg
            .icc_color_space()
            .as_ref()
            .map(|icc| icc.color_gamut)
            .unwrap_or_else(|| {
                warn!("No ICC profile found, using default sRGB color gamut");
                ColorGamut::srgb()
            });

        let uhdr_boost_computer =
            UhdrBoostComputer::new(&gain_map_metadata, max_display_boost.log2());

        Ok(Self {
            uhdr_jpeg,
            gain_map_jpeg,
            src_color_gamut,
            uhdr_boost_computer,
        })
    }

    /// Compute BT.2020 linear nits pixels from the Ultra HDR JPEG.
    fn compute_hdr_linear_pixels(&self, target_sdr_white_level: f32) -> FloatImageContent {
        const DST_COLOR_GAMUT: ColorGamut = ColorGamut::bt2020();

        let (width, height) = self.uhdr_jpeg.extent();

        let mut linear_pixels = FloatImageContent::with_extent(width, height);
        let progress = ProgressBar::new(height as u64);
        progress.set_style(
            ProgressStyle::with_template("{msg} [{bar:40.cyan/blue}] {pos}/{len} rows ({eta})")
                .expect("valid progress bar template")
                .progress_chars("##-"),
        );
        progress.set_message("Generating HDR pixels");
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

                    self.gain_map_jpeg
                        .sample_bilinear(u, v)
                        .unwrap_or_else(|| panic!("Failed to sample gain map at ({}, {})", u, v))
                        .into()
                };

                let boosted = self
                    .uhdr_boost_computer
                    .compute_boosted(in_rgb, gain_map_rgb);

                // Map 1 to `target_sdr_white_level` nits.
                let scaled_boosted = boosted * target_sdr_white_level;

                let [r, g, b] = ColorGamut::convert(
                    scaled_boosted.rgb(),
                    &self.src_color_gamut,
                    &DST_COLOR_GAMUT,
                );

                linear_pixels.set_at(x, y, FloatPixel::from([r, g, b]));
            }
            progress.inc(1);
        }
        progress.finish_with_message("HDR pixels generated");

        linear_pixels
    }

    #[cfg(feature = "avif")]
    pub fn convert_to_avif<W: Write>(
        &self,
        writer: &mut W,
        target_sdr_white_level: f32,
        preserve_exif: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height) = self.uhdr_jpeg.extent();
        let linear_pixels = self.compute_hdr_linear_pixels(target_sdr_white_level);

        let exif = if preserve_exif {
            self.uhdr_jpeg.exif_bytes().map(|tiff_bytes| {
                let mut tiff = tiff_bytes.to_vec();
                crate::tiff::strip_ifd1(&mut tiff);
                let mut block = Vec::with_capacity(4 + tiff.len());
                block.extend_from_slice(&0u32.to_be_bytes());
                block.extend_from_slice(&tiff);
                block
            })
        } else {
            None
        };

        crate::outavif::write_hdr10_linear_pixels_to_avif(
            writer,
            width as usize,
            height as usize,
            &linear_pixels,
            exif.as_deref(),
        )
        .map_err(|e| format!("Failed to write AVIF: {}", e))?;

        Ok(())
    }

    #[cfg(feature = "exr")]
    pub fn convert_to_exr<W: Write + std::io::Seek>(
        &self,
        writer: W,
        target_sdr_white_level: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (width, height) = self.uhdr_jpeg.extent();
        let linear_pixels = self.compute_hdr_linear_pixels(target_sdr_white_level);

        crate::outexr::write_hdr10_linear_pixels_to_exr(
            writer,
            width as usize,
            height as usize,
            &linear_pixels,
        )
        .map_err(|e| format!("Failed to write EXR: {}", e))?;

        Ok(())
    }
}

impl UhdrBoostComputer {
    pub fn new(gain_map_metadata: &GainMapMetadata, log2_max_display_boost: f32) -> Self {
        let gamma: FloatPixel = gain_map_metadata.gamma.into();
        let inv_gamma = gamma.rcp();

        let weight_factor = gain_map_metadata.compute_weight_factor(log2_max_display_boost);

        Self {
            inv_gamma,
            gain_map_min: gain_map_metadata.gain_map_min.into(),
            gain_map_max: gain_map_metadata.gain_map_max.into(),
            offset_sdr: gain_map_metadata.offset_sdr.into(),
            offset_hdr: gain_map_metadata.offset_hdr.into(),
            weight_factor,
        }
    }

    pub fn compute_boosted(&self, sdr: FloatPixel, recovery: FloatPixel) -> FloatPixel {
        let log_recovery = FloatPixel::powf(&recovery, &self.inv_gamma);

        let log_boost = self.gain_map_min * (FloatPixel::one() - log_recovery)
            + self.gain_map_max * log_recovery;
        let boost = (log_boost * self.weight_factor).exp2();

        let boosted = (sdr + self.offset_sdr) * boost - self.offset_hdr;
        boosted
    }
}
