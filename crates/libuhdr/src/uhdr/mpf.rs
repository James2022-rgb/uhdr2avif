
use crate::tiff;

/// Represents the Multi-Page File (MPF) information extracted from TIFF bytes,
/// which can be contained in a JPEG file.
#[derive(Debug, Clone)]
pub struct MpfInfo {
    mp_entries: Vec<MpfMpEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct MpfMpEntry {
    pub individual_image_attribute: [u8; 4],
    pub individual_image_size: u32,
    pub individual_image_data_offset: u32,
    pub dependent_image_1_entry_number: u16,
    pub dependent_image_2_entry_number: u16,
}

impl MpfInfo {
    pub fn mp_entries(&self) -> &[MpfMpEntry] {
        &self.mp_entries
    }

    pub fn new_from_bytes(mpf_bytes: &[u8]) -> std::io::Result<Self> {
        // https://web.archive.org/web/20160405200235/http://cipa.jp/std/documents/e/DC-007_E.pdf

        let mpf_tiff = tiff::Tiff::from_reader(&mut std::io::Cursor::new(mpf_bytes))?;

        let mp_index_ifd = mpf_tiff.ifds.first().unwrap();

        let version_entry = mp_index_ifd.entry_with_tag(0xB000).unwrap();
        let version_bytes = version_entry.field_value_as_undefined().unwrap();
        assert_eq!(version_bytes, &[48, 49, 48, 48], "Version bytes must be '0', '1', '0', '0'");

        let number_of_images = {
            let number_of_images_entry = mp_index_ifd.entry_with_tag(0xB001).unwrap();
            *number_of_images_entry.field_value_as_long().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Failed to read number of images",
                )
            })?.first().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "No value found for number of images",
                )
            })?
        };

        let mut mp_entries: Vec<MpfMpEntry> = Vec::new();
        {
            let mp_entry_entry = mp_index_ifd.entry_with_tag(0xB002).unwrap();
            let mp_entry_bytes = mp_entry_entry.field_value_as_undefined().unwrap();
            assert!(mp_entry_bytes.len() == 16 * number_of_images as usize);

            for i in 0..number_of_images {
                let mp_entry_bytes = &mp_entry_bytes[i as usize * 16..(i + 1) as usize * 16];
                
                let individual_image_attribute = &mp_entry_bytes[0..4];
                let individual_image_size = mpf_tiff.header.endianness.read_u32(&mut &mp_entry_bytes[4..8])?;
                let individual_image_data_offset = mpf_tiff.header.endianness.read_u32(&mut &mp_entry_bytes[8..12])?;
                let dependent_image_1_entry_number = mpf_tiff.header.endianness.read_u16(&mut &mp_entry_bytes[12..14])?;
                let dependent_image_2_entry_number = mpf_tiff.header.endianness.read_u16(&mut &mp_entry_bytes[14..16])?;

                mp_entries.push(MpfMpEntry {
                    individual_image_attribute: individual_image_attribute.try_into().unwrap(),
                    individual_image_size,
                    individual_image_data_offset,
                    dependent_image_1_entry_number,
                    dependent_image_2_entry_number,
                });
            }                
        }

        Ok(Self {
            mp_entries,
        })
    }
}
