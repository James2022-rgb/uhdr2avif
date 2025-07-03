
use crate::gainmap::GainMapMetadata;
use crate::pixel::FloatPixel;

#[derive(Debug, Clone, Copy)]
pub struct UhdrBoostComputer {
    inv_gamma: FloatPixel,
    gain_map_min: FloatPixel,
    gain_map_max: FloatPixel,
    offset_sdr: FloatPixel,
    offset_hdr: FloatPixel,
    weight_factor: f32,
}

impl UhdrBoostComputer {
    pub fn new(
        gain_map_metadata: &GainMapMetadata,
        log2_max_display_boost: f32,
    ) -> Self {
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

    pub fn compute_boosted(
        &self,
        sdr: FloatPixel,
        recovery: FloatPixel,
    ) -> FloatPixel {
        let log_recovery = FloatPixel::powf(&recovery, &self.inv_gamma);

        let log_boost = self.gain_map_min * (FloatPixel::one() - log_recovery) + self.gain_map_max * log_recovery;
        let boost = (log_boost * self.weight_factor).exp2();

        let boosted = (sdr + self.offset_sdr) * boost - self.offset_hdr;
        boosted
    }
}
