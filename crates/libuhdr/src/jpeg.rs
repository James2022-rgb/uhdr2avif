
use log::{trace, warn, error};
use zune_jpeg::ImageInfo as JpegImageInfo;
use zune_jpeg::zune_core::colorspace::ColorSpace as JpegColorSpace;

use crate::colorspace::{IccColorSpace, ColorGamut};
use crate::mpf::MpfInfo;

/// Represents a JPEG image, potentially with Ultra HDR metadata and gain map information.
#[derive(Clone)]
pub struct UhdrJpeg {
    jpeg_info: JpegImageInfo,
    xmp_bytes: Option<Vec<u8>>,
    content: JpegImageContent,
}

#[derive(Clone)]
struct JpegImageContent {
    icc_color_space: Option<IccColorSpace>,
    jpeg_color_space: JpegColorSpace,
    pixels: Vec<u8>,
}

impl UhdrJpeg {
    /// Creates a new `UhdrJpeg` instance from the provided JPEG bytes.
    /// This function decodes the JPEG image, extracts the XMP metadata, ICC profile, and pixel data.
    /// Despite the struct's name, the JPEG does not need to be in an Ultra HDR JPEG format for this function to succeed.
    pub fn new_from_bytes(jpeg_bytes: &[u8]) -> Result<Self, String> {
        use zune_jpeg::JpegDecoder;
        use zune_jpeg::zune_core::bytestream::ZCursor;

        let mut jpeg_decoder = JpegDecoder::new(ZCursor::new(jpeg_bytes));
        jpeg_decoder.decode_headers()
            .map_err(|e| format!("Failed to decode JPEG headers: {}", e))
            ?;

        let jpeg_info = jpeg_decoder.info().unwrap();

        let xmp_bytes = jpeg_decoder.xmp().cloned();

        let jpeg_output_color_space = jpeg_decoder.output_colorspace()
            .ok_or_else(|| "Failed to get JPEG output ColorSpace")
            ?;
        trace!("Output color space: {:?}", jpeg_output_color_space);

        let pixels = jpeg_decoder.decode()
            .map_err(|e| format!("Failed to decode JPEG image: {}", e))
            ?;
        trace!("Decoded JPEG: {}x{} with {} bytes", jpeg_info.width, jpeg_info.height, pixels.len());

        let icc_profile_bytes = jpeg_decoder.icc_profile();
        let icc_profile = if let Some(icc_profile_bytes) = &icc_profile_bytes {
            let icc_profile = lcms2::Profile::new_icc(&icc_profile_bytes)
                .map_err(|e| format!("Failed to parse ICC profile: {}", e))
                ?;
            Some(icc_profile)
        } else {
            None
        };
        
        let icc_color_space = icc_profile
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

    pub fn extent(&self) -> (usize, usize) {
        (self.jpeg_info.width as usize, self.jpeg_info.height as usize)
    }

    pub fn xmp_bytes(&self) -> Option<&[u8]> {
        self.xmp_bytes.as_deref()
    }

    pub fn icc_color_space(&self) -> Option<&IccColorSpace> {
        self.content.icc_color_space.as_ref()
    }

    pub fn color_gamut(&self) -> Option<ColorGamut> {
        self.icc_color_space()
            .map(|icc| icc.color_gamut)
    }

    /// Returns the MPF (Multi-Picture Format) information bytes if available,
    /// which can be parsed according to _CIPA DC- x007 - Translation- 2009_.
    pub fn mpf_bytes(&self) -> Option<&[u8]> {
        self.jpeg_info.multi_picture_information.as_deref()
    }

    /// Extracts the gain map JPEG from the original JPEG bytes if available, using the MPF information.
    /// Returns `None` if the JPEG does not contain MPF information or if the gain map JPEG cannot be extracted.
    pub fn extract_gain_map_jpeg(&self, original_bytes: &[u8]) -> Option<Self> {
        let mpf_info = {
            let mpf_bytes = self.mpf_bytes()?;

            MpfInfo::new_from_bytes(mpf_bytes)
                .ok()
                ?
        };

        if mpf_info.mp_entries().len() < 2 {
            warn!("Probably not an Ultra HDR JPEG: MPF information does not contain enough entries (found {}), expected at least 2.", mpf_info.mp_entries().len());
            return None;
        }

        let first_mp_entry = &mpf_info.mp_entries()[0];
        let offset = first_mp_entry.individual_image_size;

        let gain_map_jpeg_bytes = &original_bytes[offset as usize..original_bytes.len() - 1];
        let gain_map_jpeg = UhdrJpeg::new_from_bytes(gain_map_jpeg_bytes)
            .map_err(|e| {
                error!("Failed to extract gain map JPEG: {}", e);
                e
            })
            .ok()?;
        Some(gain_map_jpeg)
    }

    /// Fetches a pixel at the given coordinates (x, y), which is typically in a non-linear color space (i.e. after OETF).
    pub fn fetch_pixel(
        &self,
        x: usize,
        y: usize,
    ) -> [f32; 3] {
        let pixel_index = (y * self.jpeg_info.width as usize + x) * 3;

        let r = self.content.pixels[pixel_index + 0] as f32 / 255.0;
        let g = self.content.pixels[pixel_index + 1] as f32 / 255.0;
        let b = self.content.pixels[pixel_index + 2] as f32 / 255.0;

        [r, g, b]
    }

    /// Fetches a pixel at the given coordinates (x, y) and applies the EOTF according the `IccColorSpace` if available.
    /// If no `IccColorSpace` is available, the EOTF is assumed to be gamma of `2.2`.
    pub fn fetch_pixel_linear(
        &self,
        x: usize,
        y: usize,
    ) -> [f32; 3] {
        let rgb = self.fetch_pixel(x, y);
        self.to_linear(rgb)
    }

    /// Samples a pixel coordinate using bilinear filtering and clamp addressing.
    /// The U and V coordinates are in the range [0, 1].
    /// The function returns the RGB values in the range [0, 1].
    /// If the coordinates are out of bounds, it returns None.
    pub fn sample_bilinear(
        &self,
        u: f32,
        v: f32,
    ) -> Option<[f32; 3]> {
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

        let p00 = self.get_pixel_as_rgb888_unorm_linear(base_x, base_y);
        let p01 = self.get_pixel_as_rgb888_unorm_linear(base_x, base_y + 1);
        let p10 = self.get_pixel_as_rgb888_unorm_linear(base_x + 1, base_y);
        let p11 = self.get_pixel_as_rgb888_unorm_linear(base_x + 1, base_y + 1);

        let p00 = p00.unwrap_or([0.0, 0.0, 0.0]);
        let p01 = p01.unwrap_or([0.0, 0.0, 0.0]);
        let p10 = p10.unwrap_or([0.0, 0.0, 0.0]);
        let p11 = p11.unwrap_or([0.0, 0.0, 0.0]);

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

        let r = bilinear(p00[0], p10[0], p01[0], p11[0], s, t);
        let g = bilinear(p00[1], p10[1], p01[1], p11[1], s, t);
        let b = bilinear(p00[2], p10[2], p01[2], p11[2], s, t);
        Some([r, g, b])
    }
}

impl UhdrJpeg {
    fn get_pixel_as_rgb888_unorm_linear(&self, x: usize, y: usize) -> Option<[f32; 3]> {
        let [r, g, b] = self.get_pixel_as_rgb888(x, y)?;
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;
        Some(self.to_linear([r, g, b]))
    }

    fn get_pixel_as_rgb888(&self, x: usize, y: usize) -> Option<[u8; 3]> {
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
            Some([r, g, b])
        } else {
            None
        }
    }

    /// Applies the EOTF according the `IccColorSpace` if available.
    /// If no `IccColorSpace` is available, the EOTF is assumed to be gamma of `2.2`.
    fn to_linear(&self, mut rgb: [f32; 3]) -> [f32; 3] {
        if let Some(icc_color_space) = &self.content.icc_color_space {
            rgb = icc_color_space.transfer_characteristics.evaluate(&rgb);
        } else {
            // Assume 2.2 gamma, which is the default for most JPEGs and is the best we can do without an ICC profile.
            rgb[0] = rgb[0].powf(2.2);
            rgb[1] = rgb[1].powf(2.2);
            rgb[2] = rgb[2].powf(2.2);
        }
        rgb
    }
}
