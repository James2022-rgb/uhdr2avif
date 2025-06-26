
use derive_more::Debug;
use lcms2::{Profile, TagSignature, Tag, CIEXYZ, CIExyY, ToneCurve};

#[derive(Debug, Clone)]
pub struct IccColorSpace {
    pub description: Option<String>,
    pub copyright: Option<String>,
    pub color_gamut: ColorGamut,
    #[debug(skip)]
    pub transfer_characteristics: TransferCharacteristics,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorGamut {
    primaries: ColorPrimaries,
    white_point: CIExyY,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorPrimaries {
    red: CIExyY,
    green: CIExyY,
    blue: CIExyY,
}

#[derive(Clone)]
pub struct TransferCharacteristics {
    red: Option<ToneCurve>,
    green: Option<ToneCurve>,
    blue: Option<ToneCurve>,
}
unsafe impl Send for TransferCharacteristics {}
unsafe impl Sync for TransferCharacteristics {}

impl IccColorSpace {
    pub fn from_icc_profile_bytes(icc_profile_bytes: &[u8]) -> Option<Self> {
        let icc_profile = Profile::new_icc(icc_profile_bytes).ok()?;
        Self::from_icc_profile(&icc_profile)
    }

    pub fn from_icc_profile(icc_profile: &Profile) -> Option<Self> {
        let description = read_mlu_tag(icc_profile, TagSignature::ProfileDescriptionTag);
        let copyright = read_mlu_tag(icc_profile, TagSignature::CopyrightTag);

        let color_gamut = ColorGamut::from_icc_profile(icc_profile)?;

        let transfer_characteristics = TransferCharacteristics::from_icc_profile(icc_profile)?;

        Some(Self {
            description,
            copyright,
            color_gamut,
            transfer_characteristics,
        })
    }
}

impl ColorGamut {
    const WHITE_POINT_D50: CIExyY = CIExyY { x: 0.3457, y: 0.3585, Y: 1.0000 };
    const WHITE_POINT_D65: CIExyY = CIExyY { x: 0.3127, y: 0.3290, Y: 1.0000 };

    /// [sRGB](https://en.wikipedia.org/wiki/SRGB) color gamut, same color primaries and white point as the ITU-R Recommendation BT.709 or [Rec.709](https://en.wikipedia.org/wiki/Rec._709) standard.
    pub const fn srgb() -> Self {
        Self {
            primaries: ColorPrimaries::srgb(),
            white_point: Self::WHITE_POINT_D65,
        }
    }

    /// Color gamut defined by the ITU-R Recommendation BT.2020 or [Rec.2020](https://en.wikipedia.org/wiki/Rec._2020) standard.
    pub const fn bt2020() -> Self {
        Self {
            primaries: ColorPrimaries {
                red: CIExyY { x: 0.7080, y: 0.2920, Y: 0.2627 },
                green: CIExyY { x: 0.1700, y: 0.7970, Y: 0.6780 },
                blue: CIExyY { x: 0.1310, y: 0.0460, Y: 0.0593 },
            },
            white_point: Self::WHITE_POINT_D65,
        }
    }

    /// Color gamut used by the [ProPhoto RGB color space](https://en.wikipedia.org/wiki/ProPhoto_RGB_color_space) developed by Kodak.
    pub const fn prophoto_rgb() -> Self {
        Self {
            primaries: ColorPrimaries::prophoto_rgb(),
            white_point: Self::WHITE_POINT_D50,
        }
    }

    pub fn from_icc_profile_bytes(icc_profile_bytes: &[u8]) -> Option<Self> {
        let icc_profile = Profile::new_icc(icc_profile_bytes).ok()?;
        Self::from_icc_profile(&icc_profile)
    }

    pub fn from_icc_profile(icc_profile: &Profile) -> Option<Self> {
        let from_d50 = {
            if let Some(tag) = read_tag(icc_profile, TagSignature::ChromaticAdaptationTag) {
                match tag {
                    Tag::CIExyYTRIPLE(rows) => {
                        // Row-major 3x3 matrix to right-multiply to the row vector CIEXYZ.
                        let to_d50 = [
                            [rows.Red.x, rows.Green.x, rows.Blue.x],
                            [rows.Red.y, rows.Green.y, rows.Blue.y],
                            [rows.Red.Y, rows.Green.Y, rows.Blue.Y],
                        ];

                        Some(invert_matrix(to_d50)?)
                    },
                    _ => {
                        eprintln!("Expected CIExyYTRIPLE tag for Chromatic Adaptation, but got {:?}", tag);
                        return None;
                    },
                }
            } else {
                None
            }
        };

        let white_point = read_CIEXYZ_tag(icc_profile, TagSignature::MediaWhitePointTag)?;

        // Convert the white point from D50.
        let white_point = if let Some(from_d50) = &from_d50 {
            // Some non-D50 white point, in many cases D65.
            let result = transform_right(&[white_point.X, white_point.Y, white_point.Z], from_d50);
            CIEXYZ { X: result[0], Y: result[1], Z: result[2] }
        } else {
            // D50.
            white_point
        };
        
        let white_point = lcms2::XYZ2xyY(&white_point);

        // Single Chromaticity tag present ?
        if let Some(tag) = read_tag(icc_profile, TagSignature::ChromaticityTag) {
            match tag {
                Tag::CIExyYTRIPLE(primaries) => {
                    return Some(Self {
                        primaries: ColorPrimaries {
                            red: primaries.Red,
                            green: primaries.Green,
                            blue: primaries.Blue,
                        },
                        white_point,
                    });
                },
                _ => panic!("Expected CIExyYTRIPLE tag for ChromaticityTag but got {:?}", tag),
            }
        }

        // Otherwise, read the three primary colorant tags.

        let red_primary = read_CIEXYZ_tag_as_CIExyY(icc_profile, TagSignature::RedColorantTag)?;
        let green_primary = read_CIEXYZ_tag_as_CIExyY(icc_profile, TagSignature::GreenColorantTag)?;
        let blue_primary = read_CIEXYZ_tag_as_CIExyY(icc_profile, TagSignature::BlueColorantTag)?;

        Some(Self {
            primaries: ColorPrimaries {
                red: red_primary,
                green: green_primary,
                blue: blue_primary,
            },
            white_point,
        })
    }

    pub const fn primaries(&self) -> &ColorPrimaries {
        &self.primaries
    }

    /// The white point in CIExyY format.
    pub const fn white_point(&self) -> [f64; 3] {
        [self.white_point.x, self.white_point.y, self.white_point.Y]
    }
    /// The white point in CIExyY format but without Y.
    pub const fn white_point_xy(&self) -> [f64; 2] {
        [self.white_point.x, self.white_point.y]
    }

    pub fn convert(value: &[f32; 3], src: &Self, dst: &Self) -> [f32; 3] {
        // https://physics.stackexchange.com/questions/487763/how-are-the-matrices-for-the-rgb-to-from-cie-xyz-conversions-generated

        // FIXME: Much of this stuff could be precomputed and cached.

        #![allow(non_snake_case)]

        let src_p = &src.primaries;

        // This matrix converts RGB values to a relative XYZ space, but not yet scaled to match the white point.
        let src_rgb_to_XYZ = {
            // CIEXYZ coordinates of each RGB primary.
            let r_X = src_p.red.x * src_p.red.Y / src_p.red.y;
            let r_Y = src_p.red.Y;
            let r_Z = (1.0 - src_p.red.x - src_p.red.y) * r_Y / src_p.red.y;
            let g_X = src_p.green.x * src_p.green.Y / src_p.green.y;
            let g_Y = src_p.green.Y;
            let g_Z = (1.0 - src_p.green.x - src_p.green.y) * g_Y / src_p.green.y;
            let b_X = src_p.blue.x * src_p.blue.Y / src_p.blue.y;
            let b_Y = src_p.blue.Y;
            let b_Z = (1.0 - src_p.blue.x - src_p.blue.y) * b_Y / src_p.blue.y;

            [
                [r_X, r_Y, r_Z],
                [g_X, g_Y, g_Z],
                [b_X, b_Y, b_Z],
            ]
        };

        // UnscaledXYZ is not correctly scaled to the destitnation gamut white point:
        // ```
        // UnscaledXYZ = RGB * src_rgb_to_XYZ
        // ```
        //
        // In order to scale it to the destination white point, we need to scale it by a factor [a, b, c]:
        // ```
        // WhitePointXYZ = [a, b, c] * [1, 1, 1] * src_rgb_to_XYZ
        // [a, b, c] = WhitePointXYZ * src_rgb_to_XYZ^-1
        let chromatic_adaptation = {
            let dst_white_point_XYZ = {
                let w_X = dst.white_point.x * dst.white_point.Y / dst.white_point.y;
                let w_Y = dst.white_point.Y;
                let w_Z = (1.0 - dst.white_point.x - dst.white_point.y) * w_Y / dst.white_point.y;
    
                [w_X, w_Y, w_Z]
            };

            transform_right(&dst_white_point_XYZ, &invert_matrix(src_rgb_to_XYZ).unwrap())
        };

        let XYZ_to_dst_rgb= {
            let dst_p = &dst.primaries;

            let dst_rgb_to_XYZ = {
                let r_X = dst_p.red.x * dst_p.red.Y / dst_p.red.y;
                let r_Y = dst_p.red.Y;
                let r_Z = (1.0 - dst_p.red.x - dst_p.red.y) * r_Y / dst_p.red.y;
                let g_X = dst_p.green.x * dst_p.green.Y / dst_p.green.y;
                let g_Y = dst_p.green.Y;
                let g_Z = (1.0 - dst_p.green.x - dst_p.green.y) * g_Y / dst_p.green.y;
                let b_X = dst_p.blue.x * dst_p.blue.Y / dst_p.blue.y;
                let b_Y = dst_p.blue.Y;
                let b_Z = (1.0 - dst_p.blue.x - dst_p.blue.y) * b_Y / dst_p.blue.y;
    
                [
                    [r_X, r_Y, r_Z],
                    [g_X, g_Y, g_Z],
                    [b_X, b_Y, b_Z],
                ]
            };

            invert_matrix(dst_rgb_to_XYZ).unwrap()
        };

        let value_XYZ = transform_right(&[value[0] as f64, value[1] as f64, value[2] as f64], &src_rgb_to_XYZ);        
        let value_XYZ = [
            value_XYZ[0] * chromatic_adaptation[0],
            value_XYZ[1] * chromatic_adaptation[1],
            value_XYZ[2] * chromatic_adaptation[2],
        ];

        let result_rgb = transform_right(&value_XYZ, &XYZ_to_dst_rgb);

        [
            result_rgb[0] as f32,
            result_rgb[1] as f32,
            result_rgb[2] as f32,
        ]
    }
}

impl ColorPrimaries {
    pub const fn srgb() -> Self {
        Self {
            red: CIExyY { x: 0.6400, y: 0.3300, Y: 0.2126 },
            green: CIExyY { x: 0.3000, y: 0.6000, Y: 0.7152 },
            blue: CIExyY { x: 0.1500, y: 0.0600, Y: 0.0722 },
        }
    }

    pub const fn prophoto_rgb() -> Self {
        Self {
            red: CIExyY { x: 0.7347, y: 0.2653, Y: 0.28804  },
            green: CIExyY { x: 0.1596, y: 0.8404, Y: 0.71188 },
            blue: CIExyY { x: 0.0366, y: 0.0001, Y: 0.00009 },
        }
    }

    /// The red primary in CIExyY format.
    pub fn red(&self) -> [f64; 3] {
        [self.red.x, self.red.y, self.red.Y]
    }
    /// The green primary in CIExyY format.
    pub fn green(&self) -> [f64; 3] {
        [self.green.x, self.green.y, self.green.Y]
    }
    /// The blue primary in CIExyY format.
    pub fn blue(&self) -> [f64; 3] {
        [self.blue.x, self.blue.y, self.blue.Y]
    }

    /// The red primary in CIExyY format but without Y.
    pub fn red_xy(&self) -> [f64; 2] {
        [self.red.x, self.red.y]
    }
    /// The green primary in CIExyY format but without Y.
    pub fn green_xy(&self) -> [f64; 2] {
        [self.green.x, self.green.y]
    }
    /// The blue primary in CIExyY format but without Y.
    pub fn blue_xy(&self) -> [f64; 2] {
        [self.blue.x, self.blue.y]
    }
}

impl TransferCharacteristics {
    /// Evaluates this EOTF transfer function for the given RGB value.
    pub fn evaluate(&self, rgb: &[f32; 3]) -> [f32; 3] {
        let mut result = *rgb;
        if let Some(red) = &self.red {
            result[0] = red.eval(rgb[0]);
        }
        if let Some(green) = &self.green {
            result[1] = green.eval(rgb[1]);
        }
        if let Some(blue) = &self.blue {
            result[2] = blue.eval(rgb[2]);
        }
        result
    }

    fn from_icc_profile(icc_profile: &Profile) -> Option<Self> {
        let red = read_tag(icc_profile, TagSignature::RedTRCTag).and_then(|tag| {
            if let Tag::ToneCurve(curve) = tag {
                Some(curve.to_owned())
            } else {
                None
            }
        });

        let green = read_tag(icc_profile, TagSignature::GreenTRCTag).and_then(|tag| {
            if let Tag::ToneCurve(curve) = tag {
                Some(curve.to_owned())
            } else {
                None
            }
        });

        let blue = read_tag(icc_profile, TagSignature::BlueTRCTag).and_then(|tag| {
            if let Tag::ToneCurve(curve) = tag {
                Some(curve.to_owned())
            } else {
                None
            }
        });

        Some(Self { red, green, blue })
    }
}

fn read_mlu_tag(icc_profile: &Profile, sig: TagSignature) -> Option<String> {
    let tag = read_tag(icc_profile, sig)?;
    match tag {
        Tag::MLU(mlu) => {
            assert!(!mlu.tanslations().is_empty());

            let locale = mlu.tanslations()[0];

            return Some(mlu.text(locale).unwrap())
        },
        _ => panic!("Expected MLU tag"),
    }
}

#[allow(non_snake_case)]
fn read_CIEXYZ_tag_as_CIExyY(icc_profile: &Profile, sig: TagSignature) -> Option<CIExyY> {
    let ciexyz = read_CIEXYZ_tag(icc_profile, sig)?;
    Some(lcms2::XYZ2xyY(&ciexyz))
}

#[allow(non_snake_case)]
fn read_CIEXYZ_tag(icc_profile: &Profile, sig: TagSignature) -> Option<CIEXYZ> {
    let tag = read_tag(icc_profile, sig)?;
    match tag {
        Tag::CIEXYZ(xyz) => {
            return Some(*xyz)
        },
        _ => panic!("Expected CIEXYZ tag"),
    }
}

fn read_tag(icc_profile: &Profile, sig: TagSignature) -> Option<Tag<'_>> {
    if icc_profile.has_tag(sig) {
        return Some(icc_profile.read_tag(sig));
    }
    None
}

/// Transform a row vector by right-multiplying a row-major 3x3 matrix.
fn transform_right(row_vector: &[f64; 3], matrix: &[[f64; 3]; 3]) -> [f64; 3] {
    let mut result = [0.0; 3];
    for i in 0..3 {
        result[i] = row_vector[0] * matrix[0][i] +
                    row_vector[1] * matrix[1][i] +
                    row_vector[2] * matrix[2][i];
    }
    result
}

/// Multiply a 3x3 matrix by another from the right.
fn multiply(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3]{
    let mut result = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            result[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    result
}

fn invert_matrix(matrix: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det =
          matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);

    if det.abs() < 1e-10 {
        // Matrix is not invertible
        return None;
    }

    let inv_det = 1.0 / det;

    let inverse = [
        [
            (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * inv_det,
            (matrix[0][2] * matrix[2][1] - matrix[0][1] * matrix[2][2]) * inv_det,
            (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * inv_det,
        ],
        [
            (matrix[1][2] * matrix[2][0] - matrix[1][0] * matrix[2][2]) * inv_det,
            (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * inv_det,
            (matrix[0][2] * matrix[1][0] - matrix[0][0] * matrix[1][2]) * inv_det,
        ],
        [
            (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * inv_det,
            (matrix[0][1] * matrix[2][0] - matrix[0][0] * matrix[2][1]) * inv_det,
            (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * inv_det,
        ],
    ];

    Some(inverse)
}
