
pub use uhdr::UhdrConverter;
#[cfg(feature = "heif")]
pub use apple_hdr::AppleHdrHeicConverter;

pub mod colorspace;
pub mod tiff;
pub mod uhdr;
#[cfg(feature = "heif")]
pub mod apple_hdr;

#[cfg(feature = "avif")]
pub mod outavif;

#[cfg(feature = "exr")]
pub mod outexr;
#[cfg(feature = "heif")]
mod outheif;
mod pixel;

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
            
            uhdr_converter.convert_to_avif(&mut out_file, WINDOWS_SDR_WHITE_LEVEL, true)
                .expect("Failed to convert UHDR JPEG to AVIF");
        }
    }
}
