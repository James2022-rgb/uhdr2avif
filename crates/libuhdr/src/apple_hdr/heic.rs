
use std::io::{Cursor, Seek, SeekFrom};

use derive_more::Debug;
use log::debug;
use libheif_rs::{LibHeif, HeifContext, ColorSpace, RgbChroma};
use four_cc::FourCC;

use crate::colorspace::ColorGamut;
use crate::pixel::{FloatImageContent, FloatPixel};
use crate::tiff;

const APPLE_HDR_GAINMAP_URN: &str = "urn:com:apple:photo:2020:aux:hdrgainmap";

/// Represents an Apple HDR HEIC image, containing the SDR image, the gain map, and the HDR metadata (headroom).
#[derive(Debug)]
pub struct Heic {
    #[debug(skip)] sdr_image: FloatImageContent,
    hdr_metadata: AppleHdrMetadata,
    #[debug(skip)] gainmap: FloatImageContent,
    src_color_gamut: ColorGamut,
    exif_data_block: Option<Vec<u8>>,
}

#[derive(Debug)]
struct AppleHdrMetadata {
    headroom: f32,
}

impl Heic {
    pub fn new_from_bytes(heic_bytes: &[u8]) -> Result<Self, String> {
        let ctx = HeifContext::read_from_bytes(heic_bytes)
            .map_err(|e| format!("Failed to read HEIC from bytes: {}", e))?;

        let handles = ctx.top_level_image_handles();
        if handles.len() == 0 {
            return Err("No images found in HEIC".to_string());
        }

        let handle = &handles[0];

        // Detect source color gamut from NCLX or ICC color profile.
        let src_color_gamut = detect_color_gamut(handle)?;
        debug!("Detected source color gamut: {:?}", src_color_gamut);

        let auxiliary_image_handles = handle.auxiliary_images(None);
        if auxiliary_image_handles.len() == 0 {
            return Err("No auxiliary images found in HEIC".to_string());
        }

        let gainmap_handle = auxiliary_image_handles
            .into_iter()
            .find(|h| h.auxiliary_type().is_ok_and(|h| h == APPLE_HDR_GAINMAP_URN));

        let Some(gainmap_handle) = gainmap_handle else {
            return Err("No gain map auxiliary image found in HEIC".to_string());
        };

        // Check whether the image's metadata containers the `HDRGainMapVersion` key.
        {
            let gainmap_all_meta = gainmap_handle.all_metadata();

            let xmp_meta = gainmap_all_meta.iter()
                .find(|m| m.content_type == "application/rdf+xml")
                .ok_or_else(|| "No XMP metadata found".to_string())?;

            let xmp_str = std::str::from_utf8(&xmp_meta.raw_data)
                .map_err(|e| format!("Failed to parse XMP metadata as UTF-8 string: {}", e))?;

            let doc = roxmltree::Document::parse(xmp_str)
                .map_err(|e| format!("Failed to parse XMP metadata XML: {}", e))?;

            let node = doc.descendants()
                .find(|n| n.tag_name().name() == "HDRGainMapVersion")
                .ok_or_else(|| "No HDRGainMapVersion element found in XMP metadata".to_string())?;

            let version_str = node.text().ok_or_else(|| "HDRGainMapVersion element has no text content".to_string())?;

            debug!("Found HDRGainMapVersion in XMP metadata: {}", version_str);
        }

        let (hdr_metadata, exif_data_block) = {
            // Extract MakerNote from primary image's EXIF metadata.
            let all_meta = handle.all_metadata();

            let exif_meta = all_meta.iter()
                .find(|m| m.item_type == FourCC(*b"Exif"))
                .ok_or_else(|| "No EXIF metadata found on primary image".to_string())?;

            // Preserve the raw ExifDataBlock (4-byte prefix + TIFF data) for passthrough.
            let exif_data_block = exif_meta.raw_data.clone();

            let (maker33, maker48) = {
                let exif_bytes = &exif_meta.raw_data;

                // HEIF EXIF block has a 4-byte big-endian prefix: offset from byte 4 to the TIFF header.
                if exif_bytes.len() < 4 {
                    return Err("EXIF metadata too short".to_string());
                }
                let tiff_header_offset = u32::from_be_bytes(exif_bytes[0..4].try_into().unwrap()) as usize;
                let tiff_bytes = &exif_bytes[4 + tiff_header_offset..];

                let exif_tiff = tiff::Tiff::from_reader(&mut Cursor::new(tiff_bytes))
                    .map_err(|e| format!("Failed to parse EXIF TIFF: {}", e))?;

                // Find Exif Sub-IFD pointer (tag 0x8769) in IFD0.
                let ifd0 = exif_tiff.ifds.first()
                    .ok_or_else(|| "No IFD0 in EXIF".to_string())?;

                let exif_ifd_offset = ifd0.entry_with_tag(0x8769)
                    .and_then(|e| e.field_value_as_long())
                    .and_then(|v| v.first().copied())
                    .ok_or_else(|| "No Exif IFD pointer (tag 0x8769) in IFD0".to_string())?;

                // Parse the Exif Sub-IFD.
                let mut cursor = Cursor::new(tiff_bytes);
                cursor.seek(SeekFrom::Start(exif_ifd_offset as u64))
                    .map_err(|e| format!("Failed to seek to Exif Sub-IFD: {}", e))?;
                let exif_ifd = tiff::TiffIfd::new(&mut cursor, exif_tiff.header.endianness, exif_tiff.header.version)
                    .map_err(|e| format!("Failed to parse Exif Sub-IFD: {}", e))?;

                // Find MakerNote (tag 0x927C) in Exif Sub-IFD.
                let makernote_bytes = exif_ifd.entry_with_tag(0x927C)
                    .and_then(|e| e.field_value_as_undefined())
                    .ok_or_else(|| "No MakerNote (tag 0x927C) in Exif IFD".to_string())?;

                parse_apple_makernote(makernote_bytes, exif_tiff.header.endianness)?
            };
            
            let stops = if maker33 < 1.0 {
                if maker48 <= 0.01 {
                    -20.0 * maker48 + 1.8
                } else {
                    -0.101 * maker48 + 1.601
                }
            } else {
                if maker48 <= 0.01 {
                    -70.0 * maker48 + 3.0
                } else {
                    -0.303 * maker48 + 2.303
                }
            } as f32;

            let headroom = 2f32.powf(stops);

            println!(
                "Apple MakerNote: maker33 (HDRHeadroom) = {}, maker48 (HDRGain) = {} => stops = {} => headroom = {}",
                maker33, maker48, stops, headroom
            );

            let hdr_metadata = AppleHdrMetadata {
                headroom,
            };

            (hdr_metadata, Some(exif_data_block))
        };

        let lib_heif = LibHeif::new();

        let gainmap_image = lib_heif
            .decode(
                &gainmap_handle,
                ColorSpace::Monochrome,
                None
            )
            .map_err(|e| format!("Failed to decode gainmap: {}", e))?;

        let gainmap_plane = {
            let planes = gainmap_image.planes();
            let Some(mono_plane) = planes.y else {
                return Err("Decoded gainmap does not have a monochrome plane".to_string());
            };

            mono_plane
        };

        // Turn the gain map plane into a FloatImageContent.
        let gainmap = {
            // > The Apple HDR gain map is an 8-bit, single-channel luminance map that’s stored with an image.
            if gainmap_plane.bits_per_pixel != 8 {
                return Err(format!("Expected gain map to have 8 bits per pixel, but got {}", gainmap_plane.bits_per_pixel));
            }

            let mut float_image_content = FloatImageContent::with_extent(gainmap_plane.width as usize, gainmap_plane.height as usize);
            for y in 0..gainmap_plane.height {
                let row_start = y * gainmap_plane.stride as u32;
                for x in 0..gainmap_plane.width {
                    // > It’s encoded using the Rec.709 transfer function

                    let index = row_start + x;
                    let value_after_oetf = gainmap_plane.data[index as usize] as f32 / 255.0;

                    let value = if value_after_oetf <= 0.081 {
                        value_after_oetf / 4.5
                    } else {
                        ((value_after_oetf + 0.099) / 1.099).powf(1.0 / 0.45)
                    };

                    float_image_content.set_at(x as usize, y as usize, FloatPixel::new(value, value, value));
                }
            }

            float_image_content
        };



        let sdr_heif_image = {
            let image = lib_heif
                .decode(
                    handle,
                    ColorSpace::Rgb(RgbChroma::Rgba),
                    None
                )
                .map_err(|e| format!("Failed to decode primary image: {}", e))?;
            image
        };

        let sdr_plane = {
            let planes = sdr_heif_image.planes();
            let Some(rgba_plane) = planes.interleaved else {
                return Err("Decoded primary image does not have an interleaved RGBA plane".to_string());
            };
            rgba_plane
        };
        
        let sdr_image = {
            let mut float_image_content = FloatImageContent::with_extent(sdr_plane.width as usize, sdr_plane.height as usize);
            for y in 0..sdr_plane.height {
                let row_start = y * sdr_plane.stride as u32;
                for x in 0..sdr_plane.width {
                    let index = row_start + x * 4;
                    let r = srgb_eotf(sdr_plane.data[index as usize] as f32 / 255.0);
                    let g = srgb_eotf(sdr_plane.data[index as usize + 1] as f32 / 255.0);
                    let b = srgb_eotf(sdr_plane.data[index as usize + 2] as f32 / 255.0);

                    float_image_content.set_at(x as usize, y as usize, FloatPixel::new(r, g, b));
                }
            }
            float_image_content
        };

        Ok(Self {
            sdr_image,
            hdr_metadata,
            gainmap,
            src_color_gamut,
            exif_data_block,
        })
    }

    pub fn sdr_image(&self) -> &FloatImageContent {
        &self.sdr_image
    }

    pub fn gainmap(&self) -> &FloatImageContent {
        &self.gainmap
    }

    pub fn headroom(&self) -> f32 {
        self.hdr_metadata.headroom
    }

    pub fn src_color_gamut(&self) -> &ColorGamut {
        &self.src_color_gamut
    }

    /// Returns the raw EXIF ExifDataBlock bytes (4-byte offset prefix + TIFF data).
    pub fn exif_data_block(&self) -> Option<&[u8]> {
        self.exif_data_block.as_deref()
    }
}

/// sRGB electro-optical transfer function (EOTF), converting nonlinear sRGB values to linear light.
fn srgb_eotf(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Detect the source color gamut from the HEIC image handle.
/// Tries NCLX color profile first, then falls back to raw ICC profile.
fn detect_color_gamut(handle: &libheif_rs::ImageHandle) -> Result<ColorGamut, String> {
    // Try NCLX color profile first (xy primaries directly available).
    if let Some(nclx) = handle.color_profile_nclx() {
        let primaries = crate::colorspace::ColorGamut::from_xy_primaries(
            nclx.color_primary_red_x() as f64, nclx.color_primary_red_y() as f64,
            nclx.color_primary_green_x() as f64, nclx.color_primary_green_y() as f64,
            nclx.color_primary_blue_x() as f64, nclx.color_primary_blue_y() as f64,
            nclx.color_primary_white_x() as f64, nclx.color_primary_white_y() as f64,
        );
        return Ok(primaries);
    }

    // Fallback: raw ICC profile.
    if let Some(raw_profile) = handle.color_profile_raw() {
        let gamut = ColorGamut::from_icc_profile_bytes(&raw_profile.data)
            .ok_or_else(|| "Failed to extract color gamut from ICC profile".to_string())?;
        return Ok(gamut);
    }

    Err("No NCLX or ICC color profile found on primary image".to_string())
}

const APPLE_MAKERNOTE_MAGIC: &[u8; 10] = b"Apple iOS\0";

/// The offset where the IFD begins within the Apple MakerNote.
const APPLE_MAKERNOTE_IFD_OFFSET: u64 = 14;

/// Parse an Apple MakerNote and extract HDRHeadroom (tag 0x0021) and HDRGain (tag 0x0030).
///
/// The Apple MakerNote layout is:
///   - Bytes  0..10: magic `"Apple iOS\0"`
///   - Bytes 10..14: unknown (version / padding)
///   - Bytes 14+:    IFD entries (entry count followed by entries, no TIFF header)
///
/// The byte order is inherited from the parent EXIF TIFF header.
/// All data offsets within IFD entries are relative to byte 0 of the MakerNote value.
fn parse_apple_makernote(makernote_bytes: &[u8], endianness: tiff::Endianness) -> Result<(f64, f64), String> {
    if makernote_bytes.len() < APPLE_MAKERNOTE_IFD_OFFSET as usize + 2 {
        return Err("Apple MakerNote too short".to_string());
    }

    if &makernote_bytes[0..10] != APPLE_MAKERNOTE_MAGIC {
        return Err("Not an Apple MakerNote (missing 'Apple iOS\\0' header)".to_string());
    }

    let mut cursor = Cursor::new(makernote_bytes);
    cursor.set_position(APPLE_MAKERNOTE_IFD_OFFSET);

    let entry_count = endianness.read_u16(&mut cursor)
        .map_err(|e| format!("Failed to read MakerNote IFD entry count: {}", e))?;

    let mut maker33: Option<f64> = None; // HDRHeadroom  (tag 0x0021)
    let mut maker48: Option<f64> = None; // HDRGain      (tag 0x0030)

    const SRATIONAL: u16 = 10;

    for _ in 0..entry_count {
        let tag = endianness.read_u16(&mut cursor).map_err(|e| format!("MakerNote: {}", e))?;
        let field_type = endianness.read_u16(&mut cursor).map_err(|e| format!("MakerNote: {}", e))?;
        let count = endianness.read_u32(&mut cursor).map_err(|e| format!("MakerNote: {}", e))?;

        if (tag == 0x0021 || tag == 0x0030) && field_type == SRATIONAL && count == 1 {
            // SRATIONAL is 8 bytes (> 4-byte inline limit), so the value field is an offset.
            let data_offset = endianness.read_u32(&mut cursor)
                .map_err(|e| format!("MakerNote: {}", e))? as u64;

            let saved_pos = cursor.position();
            cursor.set_position(data_offset);

            let numerator = endianness.read_u32(&mut cursor)
                .map_err(|e| format!("MakerNote: {}", e))? as i32;
            let denominator = endianness.read_u32(&mut cursor)
                .map_err(|e| format!("MakerNote: {}", e))? as i32;

            cursor.set_position(saved_pos);

            let value = if denominator != 0 {
                numerator as f64 / denominator as f64
            } else {
                return Err(format!("MakerNote tag 0x{:04X}: zero denominator in SRATIONAL", tag));
            };

            match tag {
                0x0021 => maker33 = Some(value),
                0x0030 => maker48 = Some(value),
                _ => unreachable!(),
            }

            if maker33.is_some() && maker48.is_some() {
                break;
            }
        } else {
            // Skip the 4-byte value/offset field.
            cursor.seek(SeekFrom::Current(4))
                .map_err(|e| format!("MakerNote: {}", e))?;
        }
    }

    let maker33 = maker33.ok_or_else(|| "Apple MakerNote: tag 0x0021 (HDRHeadroom) not found".to_string())?;
    let maker48 = maker48.ok_or_else(|| "Apple MakerNote: tag 0x0030 (HDRGain) not found".to_string())?;

    Ok((maker33, maker48))
}

mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;

    use crate::pixel::{FloatImageContent, FloatPixel};

    #[test]
    fn test_read_heic() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let test_dir_path = Path::new(manifest_dir).join("..").join("..").join("test");

        let mut file = File::open(test_dir_path.join("greyhounds-looking-for-a-table.heic")).expect("Failed to open HEIC file");
        let mut heic_bytes = Vec::new();
        file.read_to_end(&mut heic_bytes).expect("Failed to read HEIC file");

        let heic = Heic::new_from_bytes(&heic_bytes).expect("Failed to create Heic from bytes");
    }
}
