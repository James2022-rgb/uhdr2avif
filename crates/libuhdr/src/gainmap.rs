
/// See: https://developer.android.com/media/platform/hdr-image-format
#[derive(Debug, Clone, Copy)]
pub struct GainMapMetadata {
    /// Indicates the dynamic range of the primary image. It is required to be set to `false`: https://developer.android.com/media/platform/hdr-image-format#HDR_gain_map_metadata
    /// 
    /// `false` indicates the primary image is SDR and the gain map can be combined with it to produce an HDR rendition.
    /// `true` indicates the primary image is HDR and the gain map can be combined with it to produce the SDR rendition.
    pub base_rendition_is_hdr: bool,
    /// `map_min_log2`. `log2` of min content boost, which is the minimum allowed ratio of the linear luminance for the target HDR rendition relative to that of the SDR image, at a given pixel.
    pub gain_map_min: [f32; 3],
    /// `map_max_log2`. `log2` of max content boost, which is the maximum allowed ratio of the linear luminance for the target HDR rendition relative to that of the SDR image, at a given pixel.
    pub gain_map_max: [f32; 3],
    /// `map_gamma`. The gamma to apply to the stored map values.
    pub gamma: [f32; 3],
    /// `offset_sdr`. The offset to apply to the SDR pixel values during gain map generation and application.
    pub offset_sdr: [f32; 3],
    /// `offset_hdr`. The offset to apply to the HDR pixel values during gain map generation and application.
    pub offset_hdr: [f32; 3],
    /// `hdr_capacity_min`. `log2` of the minimum display boost value for which the map is applied at all.
    pub hdr_capacity_min: f32,
    /// `hdr_capacity_max`. `log2` of the maximum display boost value for which the map is applied completely.
    pub hdr_capacity_max: f32,
}

impl GainMapMetadata {
    pub fn new_from_xmp_bytes(xmp_bytes: &[u8]) -> Option<Self> {
        let doc = roxmltree::Document::parse(std::str::from_utf8(xmp_bytes).unwrap()).unwrap();
        let description_element_node = doc.descendants().find(|node| node.tag_name().name() == "Description").unwrap();

        let base_rendition_is_hdr = Self::read_single_bool_value(&description_element_node, "BaseRenditionIsHDR").unwrap_or(false);
        let gain_map_min = Self::read_rgb_f32_value(&description_element_node, "GainMapMin").unwrap_or([0.0; 3]);
        let gain_map_max = Self::read_rgb_f32_value(&description_element_node, "GainMapMax").unwrap_or([0.0; 3]);
        let gamma = Self::read_rgb_f32_value(&description_element_node, "Gamma").unwrap_or([1.0; 3]);
        let offset_sdr = Self::read_rgb_f32_value(&description_element_node, "OffsetSDR").unwrap_or([0.015625; 3]);
        let offset_hdr = Self::read_rgb_f32_value(&description_element_node, "OffsetHDR").unwrap_or([0.015625; 3]);
        let hdr_capacity_min = Self::read_single_f32_value(&description_element_node, "HDRCapacityMin").unwrap_or(0.0);
        let hdr_capacity_max = Self::read_single_f32_value(&description_element_node, "HDRCapacityMax")?;

        Some(Self {
            base_rendition_is_hdr,
            gain_map_min,
            gain_map_max,
            gamma,
            offset_sdr,
            offset_hdr,
            hdr_capacity_min,
            hdr_capacity_max,
        })
    }

    pub fn compute_weight_factor(&self, log2_max_display_boost: f32) -> f32 {
        let unclamped_weight_factor = (log2_max_display_boost - self.hdr_capacity_min) / (self.hdr_capacity_max - self.hdr_capacity_min);
        if !self.base_rendition_is_hdr {
            unclamped_weight_factor.clamp(0.0, 1.0)
        }
        else {
            1.0 - unclamped_weight_factor.clamp(0.0, 1.0)
        }
    }
}

impl GainMapMetadata{
    fn read_single_bool_value(description_node: &roxmltree::Node<'_, '_>, name: &str) -> Option<bool> {
        let attr = description_node.attributes()
            .find(|attr| attr.name() == name);
        if let Some(attr) = attr {
            return attr.value().parse::<bool>().ok();
        }

        let value_element_node = description_node.children().find(|node| node.tag_name().name() == name)?;
        let text = value_element_node.text()?;
        text.parse::<bool>().ok()
    }

    fn read_single_f32_value(description_node: &roxmltree::Node<'_, '_>, name: &str) -> Option<f32> {
        let attr = description_node.attributes()
            .find(|attr| attr.name() == name);
        if let Some(attr) = attr {
            return attr.value().parse::<f32>().ok();
        }

        let value_element_node = description_node.children().find(|node| node.tag_name().name() == name)?;
        let text = value_element_node.text()?;
        text.parse::<f32>().ok()
    }

    fn read_rgb_f32_value(description_node: &roxmltree::Node<'_, '_>, name: &str) -> Option<[f32; 3]> {
        let attr = description_node.attributes()
            .find(|attr| attr.name() == name);
        if let Some(attr) = attr {
            let value = attr.value().parse::<f32>().ok()?;
            return Some([value, value, value]);
        }

        let value_element_node = description_node.children().find(|node| node.tag_name().name() == name)?;

        Self::read_seq_rgb_value(&value_element_node)
    }

    fn read_seq_rgb_value(value_element_node: &roxmltree::Node<'_, '_>) -> Option<[f32; 3]> {
        let seq_element_node = value_element_node.children().find(|node| node.tag_name().name() == "Seq")?;

        let mut values = [0.0; 3];
        let mut index = 0;

        for li_node in seq_element_node.children().filter(|node| node.tag_name().name() == "li") {
            if index >= 3 {
                break; // Ensure we only read up to 3 values
            }

            if let Some(text) = li_node.text() {
                if let Ok(parsed_value) = text.parse::<f32>() {
                    values[index] = parsed_value;
                    index += 1;
                }
            }
        }

        if index == 3 {
            Some(values)
        } else {
            None // Return None if we couldn't parse exactly 3 values
        }
    }
}
