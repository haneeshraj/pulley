//! After Effects animation preset (.ffx) writer for gradient colors.
//!
//! ExtendScript cannot set a shape layer's Gradient Colors (verified in AE 25.5:
//! propertyValueType NO_VALUE). The workaround is a generated .ffx preset holding the colors,
//! applied with applyPreset. Until the writer is complete this returns an empty string and the
//! AE panel keeps the gradient's geometry with After Effects' default colors, plus a warning.

use crate::scene::GradientStop;

/// Base64 of a .ffx preset that sets a gradient's colors, or an empty string if unavailable.
pub fn gradient_preset_base64(_stops: &[GradientStop]) -> String {
    String::new()
}
