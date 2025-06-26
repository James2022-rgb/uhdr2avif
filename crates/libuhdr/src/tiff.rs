
// https://www.itu.int/itudoc/itu-t/com16/tiff-fx/docs/tiff6.pdf

use std::io::{Read, Seek};

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Endianness {
    /// `0x4949` (little-endian)
    LittleEndian,
    /// `0x4D4D` (big-endian)
    BigEndian,
}

#[derive(Debug, Clone)]
pub struct Tiff {
    pub header: TiffHeader,
    pub ifds: Vec<TiffIfd>,
}

#[derive(Debug, Clone)]
pub struct TiffHeader {
    pub endianness: Endianness,
    pub version: u16,
    pub first_ifd_offset: u32,
}

/// Image File Directory (IFD) structure
#[derive(Debug, Clone)]
pub struct TiffIfd {
    pub entries: Vec<TiffIfdEntry>,

    next_ifd_offset: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct TiffIfdEntry {
    pub tag: u16,
    pub field_type: TiffFieldType,
    pub count: u32,
    pub field_value: TiffFieldValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
#[repr(u16)]
pub enum TiffFieldType {
    BYTE = 1,
    ASCII = 2,
    SHORT = 3,
    LONG = 4,
    RATIONAL = 5,
    SBYTE = 6,
    UNDEFINED = 7,
    SSHORT = 8,
    SLONG = 9,
    SRATIONAL = 10,
    FLOAT = 11,
    DOUBLE = 12,
    LONG8 = 16,
    SLONG8 = 17,
}

#[derive(Debug, Clone)]
pub enum TiffFieldValue {
    BYTE(Vec<u8>),
    ASCII(String),
    SHORT(Vec<u16>),
    LONG(Vec<u32>),
    RATIONAL(Vec<(u32, u32)>),
    SBYTE(Vec<i8>),
    UNDEFINED(Vec<u8>),
    SSHORT(Vec<i16>),
    SLONG(Vec<i32>),
    SRATIONAL(Vec<(i32, i32)>),
    FLOAT(Vec<f32>),
    DOUBLE(Vec<f64>),
    LONG8(Vec<u64>),
    SLONG8(Vec<i64>),
}

impl Endianness {
    pub fn read_u16<R: Read>(self, reader: &mut R) -> std::io::Result<u16> {
        let mut buffer = [0; 2];
        reader.read_exact(&mut buffer)?;
        match self {
            Endianness::LittleEndian => Ok(u16::from_le_bytes(buffer)),
            Endianness::BigEndian => Ok(u16::from_be_bytes(buffer)),
        }
    }

    pub fn read_u32<R: Read>(self, reader: &mut R) -> std::io::Result<u32> {
        let mut buffer = [0; 4];
        reader.read_exact(&mut buffer)?;
        match self {
            Endianness::LittleEndian => Ok(u32::from_le_bytes(buffer)),
            Endianness::BigEndian => Ok(u32::from_be_bytes(buffer)),
        }
    }
}

impl Tiff {
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> std::io::Result<Self> {
        let header = TiffHeader::new(reader)?;

        let mut ifds: Vec<TiffIfd> = Vec::new();

        let mut ifd_offset = Some(header.first_ifd_offset);
        while let Some(offset) = ifd_offset {
            reader.seek(std::io::SeekFrom::Start(offset as u64))?;
            let ifd = TiffIfd::new(reader, header.endianness, header.version)?;

            ifd_offset = ifd.next_ifd_offset;
            ifds.push(ifd);
        }

        Ok(Tiff { header, ifds })
    }
}

impl TiffIfd {
    pub fn entry_with_tag(&self, tag: u16) -> Option<&TiffIfdEntry> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }
}

impl TiffIfdEntry {
    pub fn field_value_size(&self) -> usize {
        self.field_value.size()
    }

    pub fn field_value_as_long(&self) -> Option<&[u32]> {
        if let TiffFieldValue::LONG(ref data) = self.field_value {
            Some(data)
        } else {
            None
        }
    }

    pub fn field_value_as_undefined(&self) -> Option<&[u8]> {
        if let TiffFieldValue::UNDEFINED(ref data) = self.field_value {
            Some(data)
        } else {
            None
        }
    }
}

impl TiffHeader {
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let byte_order = read_u16(reader, Endianness::LittleEndian)?;

        if byte_order != 0x4949 && byte_order != 0x4D4D {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid TIFF byte order",
            ));
        }

        let endianness = match byte_order {
            0x4949 => Endianness::LittleEndian,
            0x4D4D => Endianness::BigEndian,
            _ => unreachable!(), // This case is already handled above
        };
        
        let version = read_u16(reader, endianness)?;
        if version < 42 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid TIFF magic number",
            ));
        }

        let first_ifd_offset = read_u32(reader, endianness)?;

        Ok(TiffHeader {
            endianness,
            version,
            first_ifd_offset,
        })
    }
}

impl TiffIfd {
    /// * `reader` - The `Read` from which to read the IFD. Must be positioned at the start of the IFD.
    fn new<R: Read + Seek>(reader: &mut R, endianness: Endianness, version: u16) -> std::io::Result<Self> {
        let value_offset_size = match version {
            42 => 4usize, // 32-bit offset
            43 => 8usize, // 64-bit offset
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported TIFF version")),
        };

        let entry_count = read_u16(reader, endianness)?;
        if entry_count < 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid IFD entry count"));
        }

        let mut entries: Vec<TiffIfdEntry> = Vec::with_capacity(entry_count as usize);

        for _ in 0..entry_count {
            let tag = read_u16(reader, endianness)?;
            let field_type = read_u16(reader, endianness)?;
            let count = read_u32(reader, endianness)?;

            let field_type = TiffFieldType::from_u16(field_type)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid field type"))?;

            let size = field_type.size() * count as usize;

            let field_value = if size <= value_offset_size {
                // The field value is stored directly in the IFD entry
                TiffFieldValue::from_reader(reader, endianness, field_type, count)?
            } else {
                // The field value is stored in a separate location.
                // We need to seek to that location and read the value from there.

                let value_offset: u64 = match value_offset_size {
                    4 => read_u32(reader, endianness)? as u64,
                    8 => read_u64(reader, endianness)?,
                    _ => unreachable!(),
                };

                let old_position = reader.stream_position()?;
                reader.seek(std::io::SeekFrom::Start(value_offset))?;

                let field_value = TiffFieldValue::from_reader(reader, endianness, field_type, count)?;
                reader.seek(std::io::SeekFrom::Start(old_position))?;
                field_value
            };

            entries.push(TiffIfdEntry {
                tag,
                field_type,
                count,
                field_value,
            });
        }

        let next_ifd_offset = read_u32(reader, endianness)?;
        let next_ifd_offset = if next_ifd_offset == 0 {
            None
        } else {
            Some(next_ifd_offset)
        };

        Ok(TiffIfd {
            entries,

            next_ifd_offset,
        })
    }
}

impl TiffFieldType {
    fn size(&self) -> usize {
        match self {
            TiffFieldType::BYTE => 1,
            TiffFieldType::ASCII => 1,
            TiffFieldType::SHORT => 2,
            TiffFieldType::LONG => 4,
            TiffFieldType::RATIONAL => 8,
            TiffFieldType::SBYTE => 1,
            TiffFieldType::UNDEFINED => 1,
            TiffFieldType::SSHORT => 2,
            TiffFieldType::SLONG => 4,
            TiffFieldType::SRATIONAL => 8,
            TiffFieldType::FLOAT => 4,
            TiffFieldType::DOUBLE => 8,
            _ => unimplemented!(),
        }
    }
}

impl TiffFieldValue {
    fn from_reader<R: Read>(reader: &mut R, endianness: Endianness, field_type: TiffFieldType, count: u32) -> std::io::Result<Self> {
        match field_type {
            TiffFieldType::BYTE => {
                let mut buffer = vec![0; count as usize];
                reader.read_exact(&mut buffer)?;
                Ok(TiffFieldValue::BYTE(buffer))
            }
            TiffFieldType::ASCII => {
                let mut buffer = vec![0; count as usize];
                reader.read_exact(&mut buffer)?;
                let string = String::from_utf8(buffer).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid ASCII string"))?;
                Ok(TiffFieldValue::ASCII(string))
            }
            TiffFieldType::SHORT => {
                let mut values = vec![0; count as usize];
                for i in 0..count {
                    values[i as usize] = read_u16(reader, endianness)?;
                }
                Ok(TiffFieldValue::SHORT(values))
            },
            TiffFieldType::LONG => {
                let mut values = vec![0; count as usize];
                for i in 0..count {
                    values[i as usize] = read_u32(reader, endianness)?;
                }
                Ok(TiffFieldValue::LONG(values))
            },
            TiffFieldType::RATIONAL => {
                let mut values = vec![(0, 0); count as usize];
                for i in 0..count {
                    let numerator = read_u32(reader, endianness)?;
                    let denominator = read_u32(reader, endianness)?;
                    values[i as usize] = (numerator, denominator);
                }
                Ok(TiffFieldValue::RATIONAL(values))
            },
            TiffFieldType::SBYTE => {
                let mut buffer = vec![0; count as usize];
                reader.read_exact(&mut buffer)?;
                let values: Vec<i8> = buffer.iter().map(|&b| b as i8).collect();
                Ok(TiffFieldValue::SBYTE(values))
            },
            TiffFieldType::UNDEFINED => {
                let mut buffer = vec![0; count as usize];
                reader.read_exact(&mut buffer)?;
                Ok(TiffFieldValue::UNDEFINED(buffer))
            },
            TiffFieldType::SSHORT => {
                let mut values = vec![0; count as usize];
                for i in 0..count {
                    values[i as usize] = read_u16(reader, endianness)? as i16;
                }
                Ok(TiffFieldValue::SSHORT(values))
            },
            TiffFieldType::SLONG => {
                let mut values = vec![0; count as usize];
                for i in 0..count {
                    values[i as usize] = read_u32(reader, endianness)? as i32;
                }
                Ok(TiffFieldValue::SLONG(values))
            },
            TiffFieldType::SRATIONAL => {
                let mut values = vec![(0, 0); count as usize];
                for i in 0..count {
                    let numerator = read_u32(reader, endianness)? as i32;
                    let denominator = read_u32(reader, endianness)? as i32;
                    values[i as usize] = (numerator, denominator);
                }
                Ok(TiffFieldValue::SRATIONAL(values))
            },
            TiffFieldType::FLOAT => {
                let mut values = vec![0.0; count as usize];
                for i in 0..count {
                    let value = read_f32(reader, endianness)?;
                    values[i as usize] = value;
                }
                Ok(TiffFieldValue::FLOAT(values))
            },
            TiffFieldType::DOUBLE => {
                let mut values = vec![0.0; count as usize];
                for i in 0..count {
                    let value = read_f64(reader, endianness)?;
                    values[i as usize] = value;
                }
                Ok(TiffFieldValue::DOUBLE(values))
            },
            // Handle other field types similarly
            _ => unimplemented!(),
        }
    }

    fn size(&self) -> usize {
        match self {
            TiffFieldValue::BYTE(values) => values.len(),
            TiffFieldValue::ASCII(string) => string.len(),
            TiffFieldValue::SHORT(values) => values.len() * std::mem::size_of::<u16>(),
            TiffFieldValue::LONG(values) => values.len() * std::mem::size_of::<u32>(),
            TiffFieldValue::RATIONAL(values) => values.len() * (std::mem::size_of::<u32>() * 2),
            TiffFieldValue::SBYTE(values) => values.len(),
            TiffFieldValue::UNDEFINED(values) => values.len(),
            TiffFieldValue::SSHORT(values) => values.len() * std::mem::size_of::<i16>(),
            TiffFieldValue::SLONG(values) => values.len() * std::mem::size_of::<i32>(),
            TiffFieldValue::SRATIONAL(values) => values.len() * (std::mem::size_of::<i32>() * 2),
            TiffFieldValue::FLOAT(values) => values.len() * std::mem::size_of::<f32>(),
            TiffFieldValue::DOUBLE(values) => values.len() * std::mem::size_of::<f64>(),
            _ => unimplemented!(),
        }
    }
}

fn read_u16<R: Read>(reader: &mut R, endianness: Endianness) -> std::io::Result<u16> {
    let mut buffer = [0; 2];
    reader.read_exact(&mut buffer)?;
    match endianness {
        Endianness::LittleEndian => Ok(u16::from_le_bytes(buffer)),
        Endianness::BigEndian => Ok(u16::from_be_bytes(buffer)),
    }
}

fn read_u32<R: Read>(reader: &mut R, endianness: Endianness) -> std::io::Result<u32> {
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)?;
    match endianness {
        Endianness::LittleEndian => Ok(u32::from_le_bytes(buffer)),
        Endianness::BigEndian => Ok(u32::from_be_bytes(buffer)),
    }
}

fn read_u64<R: Read>(reader: &mut R, endianness: Endianness) -> std::io::Result<u64> {
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    match endianness {
        Endianness::LittleEndian => Ok(u64::from_le_bytes(buffer)),
        Endianness::BigEndian => Ok(u64::from_be_bytes(buffer)),
    }
}

fn read_f32<R: Read>(reader: &mut R, endianness: Endianness) -> std::io::Result<f32> {
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)?;
    match endianness {
        Endianness::LittleEndian => Ok(f32::from_le_bytes(buffer)),
        Endianness::BigEndian => Ok(f32::from_be_bytes(buffer)),
    }
}

fn read_f64<R: Read>(reader: &mut R, endianness: Endianness) -> std::io::Result<f64> {
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    match endianness {
        Endianness::LittleEndian => Ok(f64::from_le_bytes(buffer)),
        Endianness::BigEndian => Ok(f64::from_be_bytes(buffer)),
    }
}
