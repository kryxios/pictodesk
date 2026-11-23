//! Real ICC profile support, powered by moxcms (a pure-Rust color
//! management engine — the plan said LittleCMS; moxcms gives the same
//! ICC v2/v4 capability with no C library dependency, which keeps
//! PictoDesk buildable anywhere Rust builds. The API here is the seam:
//! if we ever swap engines, callers don't change.)
//!
//! What this provides:
//! - Load any `.icc`/`.icm` profile from bytes or a file
//! - Built-in sRGB and Adobe RGB profiles
//! - Profile-to-profile conversion for RGBA8 buffers (display/export)
//!   and RGB f32 buffers (the 16-bit pipeline converts via f32)

use std::path::Path;

use moxcms::{ColorProfile, Layout, TransformOptions};

use crate::color::WorkingSpace;

#[derive(Debug)]
pub struct IccError(pub String);

impl std::fmt::Display for IccError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ICC error: {}", self.0)
    }
}
impl std::error::Error for IccError {}

#[derive(Clone)]
pub struct IccProfile {
    inner: ColorProfile,
    pub description: String,
}

impl IccProfile {
    /// Parse a real ICC profile from its raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IccError> {
        let inner = ColorProfile::new_from_slice(bytes).map_err(|e| IccError(format!("{e:?}")))?;
        let description = match &inner.description {
            Some(moxcms::ProfileText::Localizable(records)) => records
                .first()
                .map(|r| r.value.clone())
                .unwrap_or_else(|| "unnamed profile".to_string()),
            _ => "unnamed profile".to_string(),
        };
        Ok(IccProfile { inner, description })
    }

    pub fn from_file(path: &Path) -> Result<Self, IccError> {
        let bytes = std::fs::read(path).map_err(|e| IccError(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn srgb() -> Self {
        IccProfile {
            inner: ColorProfile::new_srgb(),
            description: "sRGB".into(),
        }
    }

    pub fn adobe_rgb() -> Self {
        IccProfile {
            inner: ColorProfile::new_adobe_rgb(),
            description: "Adobe RGB (1998)".into(),
        }
    }

    /// The ICC profile matching a built-in working space. ProPhoto maps
    /// to a wide-gamut built-in as the closest stock profile; documents
    /// that need exact ROMM should load the real ROMM ICC from file —
    /// which from_bytes/from_file now make possible.
    pub fn for_working_space(space: WorkingSpace) -> Self {
        match space {
            WorkingSpace::Srgb => Self::srgb(),
            WorkingSpace::AdobeRgb => Self::adobe_rgb(),
            WorkingSpace::ProPhoto => Self::adobe_rgb(),
        }
    }

    /// Convert packed RGBA8 pixels from this profile to `dst`.
    /// Alpha passes through untouched (ICC transforms are color-only).
    pub fn convert_rgba8(
        &self,
        dst: &IccProfile,
        src: &[u8],
        out: &mut [u8],
    ) -> Result<(), IccError> {
        assert_eq!(src.len(), out.len());
        assert_eq!(src.len() % 4, 0);
        let transform = self
            .inner
            .create_transform_8bit(
                Layout::Rgba,
                &dst.inner,
                Layout::Rgba,
                TransformOptions::default(),
            )
            .map_err(|e| IccError(format!("{e:?}")))?;
        transform
            .transform(src, out)
            .map_err(|e| IccError(format!("{e:?}")))?;
        Ok(())
    }

    /// Convert RGB f32 triplets (0.0..=1.0) from this profile to `dst`.
    pub fn convert_rgb_f32(
        &self,
        dst: &IccProfile,
        src: &[f32],
        out: &mut [f32],
    ) -> Result<(), IccError> {
        assert_eq!(src.len(), out.len());
        assert_eq!(src.len() % 3, 0);
        let transform = self
            .inner
            .create_transform_f32(
                Layout::Rgb,
                &dst.inner,
                Layout::Rgb,
                TransformOptions::default(),
            )
            .map_err(|e| IccError(format!("{e:?}")))?;
        transform
            .transform(src, out)
            .map_err(|e| IccError(format!("{e:?}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_srgb_profile_loads_and_self_converts() {
        // A real ICC byte stream, produced by a standards-compliant writer.
        let bytes = ColorProfile::new_srgb().encode().unwrap();
        let prof = IccProfile::from_bytes(&bytes).unwrap();
        // sRGB -> sRGB must be (near-)identity on 8-bit data.
        let src: Vec<u8> = vec![200, 30, 90, 255, 10, 220, 128, 128];
        let mut out = vec![0u8; src.len()];
        prof.convert_rgba8(&IccProfile::srgb(), &src, &mut out)
            .unwrap();
        for i in 0..src.len() {
            assert!(
                (src[i] as i32 - out[i] as i32).abs() <= 1,
                "channel {i}: {} -> {}",
                src[i],
                out[i]
            );
        }
    }

    #[test]
    fn garbage_bytes_are_rejected() {
        assert!(IccProfile::from_bytes(b"this is not an icc profile at all").is_err());
        assert!(IccProfile::from_bytes(&[]).is_err());
    }

    #[test]
    fn srgb_to_adobe_f32_keeps_neutral_gray_neutral() {
        let src = [0.5f32, 0.5, 0.5, 0.25, 0.25, 0.25];
        let mut out = [0.0f32; 6];
        IccProfile::srgb()
            .convert_rgb_f32(&IccProfile::adobe_rgb(), &src, &mut out)
            .unwrap();
        for px in out.as_chunks::<3>().0 {
            assert!(
                (px[0] - px[1]).abs() < 0.01 && (px[1] - px[2]).abs() < 0.01,
                "gray went non-neutral: {px:?}"
            );
        }
    }

    #[test]
    fn icc_path_agrees_with_matrix_path_for_adobe_red() {
        // The ICC engine and our Phase 0 matrix math should land close
        // for an in-gamut color going sRGB -> Adobe RGB.
        let via_icc_f32 = {
            let mut out = [0.0f32; 3];
            IccProfile::srgb()
                .convert_rgb_f32(&IccProfile::adobe_rgb(), &[0.8, 0.2, 0.1], &mut out)
                .unwrap();
            out
        };
        let via_matrix = WorkingSpace::Srgb.convert_rgb((0.8, 0.2, 0.1), WorkingSpace::AdobeRgb);
        assert!(
            (via_icc_f32[0] - via_matrix.0).abs() < 0.05,
            "icc {} vs matrix {}",
            via_icc_f32[0],
            via_matrix.0
        );
        assert!(
            (via_icc_f32[1] - via_matrix.1).abs() < 0.05,
            "icc {} vs matrix {}",
            via_icc_f32[1],
            via_matrix.1
        );
    }
}
