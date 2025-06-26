
mod colorspace;
mod gainmap;
mod mpf;
#[cfg(feature = "exr")]
mod outexr;
#[cfg(feature = "avif")]
mod outavif;
#[cfg(feature = "heif")]
mod outheif;
mod tiff;

use log::trace;

use zune_jpeg::ImageInfo as JpegImageInfo;
use zune_jpeg::zune_core::colorspace::ColorSpace as JpegColorSpace;

use crate::colorspace::IccColorSpace;

pub struct UhdrJpeg {
    jpeg_info: JpegImageInfo,
    xmp_bytes: Option<Vec<u8>>,
    content: JpegImageContent,
}

struct JpegImageContent {
    icc_color_space: Option<IccColorSpace>,
    jpeg_color_space: JpegColorSpace,
    pixels: Vec<u8>,
}

impl UhdrJpeg {
    pub fn new_from_bytes(jpeg_bytes: &[u8]) -> Result<Self, String> {
        use zune_jpeg::JpegDecoder;
        use zune_jpeg::zune_core::bytestream::ZCursor;

        let mut jpeg_decoder = zune_jpeg::JpegDecoder::new(ZCursor::new(jpeg_bytes));
        jpeg_decoder.decode_headers().map_err(|e| format!("Failed to decode JPEG headers: {}", e))?;

        let jpeg_info = jpeg_decoder.info().unwrap();

        let xmp_bytes = jpeg_decoder.xmp().cloned();

        let jpeg_output_color_space = jpeg_decoder.output_colorspace().ok_or_else(|| "Failed to get JPEG output ColorSpace")?;
        trace!("Output color space: {:?}", jpeg_output_color_space);

        let pixels = jpeg_decoder.decode().map_err(|e| format!("Failed to decode JPEG image: {}", e))?;
        trace!("Decoded JPEG: {}x{} with {} bytes", jpeg_info.width, jpeg_info.height, pixels.len());

        let icc_profile_bytes = jpeg_decoder.icc_profile();
        let icc_profile = if let Some(icc_profile_bytes) = &icc_profile_bytes {
            let icc_profile = lcms2::Profile::new_icc(&icc_profile_bytes).map_err(|e| format!("Failed to parse ICC profile: {}", e))?;
            Some(icc_profile)
        } else {
            None
        };
        
        let mut icc_color_space = icc_profile
            .as_ref()
            .and_then(|icc_profile| {
                IccColorSpace::from_icc_profile(icc_profile)
            });

        trace!("ICC Color space: {:?}", icc_color_space);

        Ok(Self {
            jpeg_info,
            xmp_bytes,
            content: JpegImageContent {
                icc_color_space,
                jpeg_color_space: jpeg_output_color_space,
                pixels,
            },
        })
    }

    /// Samples a pixel coordinate using bilinear filtering and clamp addressing.
    /// The U and V coordinates are in the range [0, 1].
    /// The function returns the RGB values in the range [0, 1].
    /// If the coordinates are out of bounds, it returns None.
    fn sample_bilinear(
        &self,
        u: f32,
        v: f32,
    ) -> Option<(f32, f32, f32)> {
        // U and V are in the range [0, 1]
        let width = self.jpeg_info.width as f32;
        let height = self.jpeg_info.height as f32;

        let x = u * width;
        let y = v * height;

        let base_x = if x.fract() < 0.5 {
            x.floor() - 1.0
        }
        else {
            x.floor()
        };
        let base_y = if y.fract() < 0.5 {
            y - 1.0
        }
        else {
            y.floor()
        };

        let base_x = (base_x as usize).clamp(0, self.jpeg_info.width as usize - 1);
        let base_y = (base_y as usize).clamp(0, self.jpeg_info.height as usize - 1);

        let p00 = self.get_pixel_as_rgb888_unorm(base_x, base_y);
        let p01 = self.get_pixel_as_rgb888_unorm(base_x, base_y + 1);
        let p10 = self.get_pixel_as_rgb888_unorm(base_x + 1, base_y);
        let p11 = self.get_pixel_as_rgb888_unorm(base_x + 1, base_y + 1);

        let p00 = p00.unwrap_or((0.0, 0.0, 0.0));
        let p01 = p01.unwrap_or((0.0, 0.0, 0.0));
        let p10 = p10.unwrap_or((0.0, 0.0, 0.0));
        let p11 = p11.unwrap_or((0.0, 0.0, 0.0));

        let s = (x - base_x as f32).clamp(0.0, 1.0);
        let t = (y - base_y as f32).clamp(0.0, 1.0);

        fn lerp(a: f32, b: f32, t: f32) -> f32 {
            a + (b - a) * t
        }

        fn bilinear(p00: f32, p10: f32, p01: f32, p11: f32, s: f32, t: f32) -> f32 {
            lerp(
                lerp(p00, p10, s),
                lerp(p01, p11, s),
                t,
            )
        }

        let r = bilinear(p00.0, p10.0, p01.0, p11.0, s, t);
        let g = bilinear(p00.1, p10.1, p01.1, p11.1, s, t);
        let b = bilinear(p00.2, p10.2, p01.2, p11.2, s, t);
        Some((r, g, b))
    }

    fn get_pixel_as_rgb888_unorm(&self, x: usize, y: usize) -> Option<(f32, f32, f32)> {
        let (r, g, b) = self.get_pixel_as_rgb888(x, y)?;
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;
        Some((r, g, b))
    }

    fn get_pixel_as_rgb888(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        let pixel_index = match self.content.jpeg_color_space {
            JpegColorSpace::RGB => (y * self.jpeg_info.width as usize + x) * 3,
            JpegColorSpace::Luma => (y * self.jpeg_info.width as usize + x) * 1,
            _ => return None,
        };

        if pixel_index < self.content.pixels.len() {
            let (r, g, b) = match self.content.jpeg_color_space {
                JpegColorSpace::RGB => (self.content.pixels[pixel_index], self.content.pixels[pixel_index + 1], self.content.pixels[pixel_index + 2]),
                JpegColorSpace::Luma => (self.content.pixels[pixel_index], self.content.pixels[pixel_index], self.content.pixels[pixel_index]),
                _ => return None,
            };
            Some((r, g, b))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::colorspace::ColorGamut;
    use crate::gainmap::GainMapMetadata;
    use crate::mpf::MpfInfo;

    use super::*;

    #[test]
    fn it_works() {
        const WINDOWS_SDR_WHITE_LEVEL: f32 = 80.0f32;

        // FIXME: The maximum brightness of the display in nits.
        const ASSUMED_DISPLAY_MAX_BRIGHTNESS_ :f32 = 930.0f32;

        // FIXME: The maximum available boost supported by a display, at a given point in time.
        const MAX_DISPLAY_BOOST: f32 = ASSUMED_DISPLAY_MAX_BRIGHTNESS_ / WINDOWS_SDR_WHITE_LEVEL;

        // FIXME: Test value:
        const TARGET_SDR_WHITE_LEVEL: f32 = 240.0;

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
            let jpeg_bytes = std::fs::read(file_path).unwrap();
            let jpeg = UhdrJpeg::new_from_bytes(&jpeg_bytes).unwrap();

            let jpeg_info = &jpeg.jpeg_info;

            let mpf_bytes = jpeg_info.multi_picture_information.as_ref().unwrap();
            let mpf_info = MpfInfo::new_from_bytes(mpf_bytes).unwrap();
            println!("MPF Info: {:?}", mpf_info);

            assert!(2 <= mpf_info.mp_entries().len());

            let gain_map_jpeg_bytes = &jpeg_bytes[mpf_info.mp_entries()[0].individual_image_size as usize..jpeg_bytes.len() - 1];
            let gain_map_jpeg = UhdrJpeg::new_from_bytes(gain_map_jpeg_bytes).unwrap();
            println!("Gain Map JPEG: {}x{}", gain_map_jpeg.jpeg_info.width, gain_map_jpeg.jpeg_info.height);

            let gain_map_jpeg_xmp_bytes = gain_map_jpeg.xmp_bytes.as_ref().unwrap();
            
            let gain_map_metadata = GainMapMetadata::new_from_xmp_bytes(&gain_map_jpeg_xmp_bytes);
            println!("Gain Map Metadata: {:?}", gain_map_metadata);

            let color_gamut = jpeg.content.icc_color_space.as_ref().map(|v| v.color_gamut).unwrap_or(ColorGamut::srgb());

            let gain_map_metadata = gain_map_metadata.as_ref().unwrap();

            let unclamped_weight_factor = (MAX_DISPLAY_BOOST.log2() - gain_map_metadata.hdr_capacity_min) / (gain_map_metadata.hdr_capacity_max - gain_map_metadata.hdr_capacity_min);
            let weight_factor = if !gain_map_metadata.base_rendition_is_hdr {
                unclamped_weight_factor.clamp(0.0, 1.0)
            }
            else {
                1.0 - unclamped_weight_factor.clamp(0.0, 1.0)
            };

            let output_file_name = file_path.file_stem().unwrap().to_str().unwrap();
            let output_file_name = format!("{}.avif", output_file_name);

            crate::outavif::write_rgb_image_to_avif(
                &output_file_name,
                jpeg_info.width as usize,
                jpeg_info.height as usize,
                &color_gamut,
                |x, y| {
                    let pixel_index = (y * jpeg_info.width as usize + x) * 3;

                    let pixels = &jpeg.content.pixels;
                    let r = pixels[pixel_index + 0] as f32 / 255.0;
                    let g = pixels[pixel_index + 1] as f32 / 255.0;
                    let b = pixels[pixel_index + 2] as f32 / 255.0;

                    // RGB value after EOTF.
                    let mut rgb = [r, g, b];
                    if let Some(icc_color_space) = &jpeg.content.icc_color_space {
                        rgb = icc_color_space.transfer_characteristics.evaluate(&rgb);
                    } else {
                        // Assume 2.2 gamma, which is the default for most JPEGs and is the best we can do without an ICC profile.
                        rgb[0] = rgb[0].powf(2.2);
                        rgb[1] = rgb[1].powf(2.2);
                        rgb[2] = rgb[2].powf(2.2);
                    }

                    let (u, v) ={
                        let texel_width = 1.0 / jpeg_info.width as f32;
                        let texel_height = 1.0 / jpeg_info.height as f32;

                        // Use texel center.
                        let u_offset = texel_width * 0.5;
                        let v_offset = texel_height * 0.5;
                        let u = texel_width * x as f32 + u_offset;
                        let v = texel_height * y as f32 + v_offset;

                        (u, v)
                    };
                    let gain_map_rgb = gain_map_jpeg.sample_bilinear(u, v).unwrap_or_else(|| panic!("Failed to sample gain map at ({}, {})", u, v));

                    let log_recovery_r = f32::powf(gain_map_rgb.0, 1.0 / gain_map_metadata.gamma[0]);
                    let log_recovery_g = f32::powf(gain_map_rgb.1, 1.0 / gain_map_metadata.gamma[1]);
                    let log_recovery_b = f32::powf(gain_map_rgb.2, 1.0 / gain_map_metadata.gamma[2]);

                    let log_boost_r = gain_map_metadata.gain_map_min[0] * (1.0 - log_recovery_r) + gain_map_metadata.gain_map_max[0] * log_recovery_r;
                    let log_boost_g = gain_map_metadata.gain_map_min[1] * (1.0 - log_recovery_g) + gain_map_metadata.gain_map_max[1] * log_recovery_g;
                    let log_boost_b = gain_map_metadata.gain_map_min[2] * (1.0 - log_recovery_b) + gain_map_metadata.gain_map_max[2] * log_recovery_b;

                    let boost_r = (log_boost_r * weight_factor).exp2();
                    let boost_g = (log_boost_g * weight_factor).exp2();
                    let boost_b = (log_boost_b * weight_factor).exp2();

                    let r = (rgb[0] + gain_map_metadata.offset_sdr[0]) * boost_r - gain_map_metadata.offset_hdr[0];
                    let g = (rgb[1] + gain_map_metadata.offset_sdr[1]) * boost_g - gain_map_metadata.offset_hdr[1];
                    let b = (rgb[2] + gain_map_metadata.offset_sdr[2]) * boost_b - gain_map_metadata.offset_hdr[2];

                    // Map 1 to `SDR_WHITE_LEVEL` nits.
                    let r = r * TARGET_SDR_WHITE_LEVEL;
                    let g = g * TARGET_SDR_WHITE_LEVEL;
                    let b = b * TARGET_SDR_WHITE_LEVEL;

                    (r, g, b)
                }
            ).unwrap();
        }
    }
}
