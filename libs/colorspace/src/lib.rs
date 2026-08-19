//! CIE L*a*b* → sRGB conversion (D65 white point) -- the "Lab-primary,
//! explicit white-point handling" colorspace conversion named in
//! `docs/roadmap/Workbenches.md`'s color-science tooling item. Pure and
//! core-side-free, same "hand-roll small testable math" habit as
//! `colormap.rs`/`ticks.rs` -- no colorspace crate exists in the
//! workspace today and this doesn't need one.
//!
//! standard textbook formulas (CIE Lab → CIE XYZ → linear sRGB → sRGB
//! gamma companding), D65/2° reference white -- the same reference point
//! every widely-cited Lab↔sRGB conversion table already uses, not a
//! project-specific choice.

/// D65/2° reference white in CIE XYZ, scaled to Y = 100 -- the standard
/// reference point Lab is defined relative to.
const WHITE_X: f64 = 95.047;
const WHITE_Y: f64 = 100.000;
const WHITE_Z: f64 = 108.883;

/// CIE's own breakpoint between the linear and cube-root segments of
/// the Lab forward/inverse transfer function.
const EPSILON: f64 = 0.008856;
const KAPPA: f64 = 903.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// inverse of Lab's `f(t)` -- `t.powi(3)` above the breakpoint, linear
/// below it. Applied to `fx`/`fy`/`fz` to recover CIE XYZ from Lab.
fn f_inv(t: f64) -> f64 {
    let cubed = t.powi(3);
    if cubed > EPSILON {
        cubed
    } else {
        (116.0 * t - 16.0) / KAPPA
    }
}

/// CIE L*a*b* (D65) → CIE XYZ (D65), `X`/`Y`/`Z` scaled to `[0, 100]`.
/// `Y` uses the same `f_inv(fy)` as `X`/`Z` not a separate
/// piecewise-in-`L` formula some references quote -- they're
/// algebraically identical (`f_inv`'s own linear branch reduces to
/// `L / KAPPA` exactly when `fy = (L+16)/116` is substituted in), so
/// one shared function covers all three without duplicating the
/// breakpoint logic.
fn lab_to_xyz(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    (WHITE_X * f_inv(fx), WHITE_Y * f_inv(fy), WHITE_Z * f_inv(fz))
}

/// sRGB's own OETF (gamma companding) -- linear light in `[0, 1]` to the
/// non-linear signal sRGB actually stores, same standard piecewise
/// curve for every sRGB-target conversion, not a project-specific
/// gamma approximation.
fn linear_to_srgb_companded(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// CIE XYZ (D65, `[0, 100]`-scaled) → linear sRGB, `[0, 1]`-scaled --
/// the standard D65 XYZ→linear-sRGB matrix (Bruce Lindbloom's
/// reference values, the same ones nearly every public Lab↔sRGB
/// conversion table cites).
fn xyz_to_linear_srgb(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let x = x / 100.0;
    let y = y / 100.0;
    let z = z / 100.0;
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let b = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    (r, g, b)
}

/// absorbs the rounding noise inherent to the *published* white-point/
/// matrix constants above (each independently rounded to 4-5
/// significant figures) -- real primaries sitting exactly on the gamut
/// boundary measure ~1e-4 off `[0, 1]` purely from that, not from an
/// actual out-of-gamut color. Two orders of magnitude below anything
/// this crate treats as a real violation (`impossible` in this crate's
/// own tests is off by 0.07-1.5, not 0.0001).
const GAMUT_TOLERANCE: f64 = 0.001;

/// `true` when a Lab color falls inside the sRGB gamut *before* any
/// clamping -- the "honesty ledger" this crate's own doc comment and
/// `docs/roadmap/Workbenches.md`'s color-science item both call for:
/// a caller (`example-doctor-color-swatch`) uses this to flag a swatch
/// whose real, unclamped conversion falls outside what sRGB can
/// actually display, not silently clamping and saying nothing.
pub fn lab_in_srgb_gamut(l: f64, a: f64, b: f64) -> bool {
    let (x, y, z) = lab_to_xyz(l, a, b);
    let (r, g, bl) = xyz_to_linear_srgb(x, y, z);
    let lo = -GAMUT_TOLERANCE;
    let hi = 1.0 + GAMUT_TOLERANCE;
    [r, g, bl].iter().all(|c| (lo..=hi).contains(c))
}

/// CIE L*a*b* (D65) → sRGB, clamped to `[0, 255]` per channel -- the
/// same clamp-and-document-it stance `lab_in_srgb_gamut` exists to make
/// non-silent for a caller that cares.
pub fn lab_to_srgb(l: f64, a: f64, b: f64) -> Rgb {
    let (x, y, z) = lab_to_xyz(l, a, b);
    let (r, g, b) = xyz_to_linear_srgb(x, y, z);
    let to_byte = |c: f64| -> u8 { (linear_to_srgb_companded(c.clamp(0.0, 1.0)) * 255.0).round().clamp(0.0, 255.0) as u8 };
    Rgb { r: to_byte(r), g: to_byte(g), b: to_byte(b) }
}

/// one named swatch from a `.color-swatches` file -- a project-declared
/// palette to check/render, same shape as `analog-waveform::Capture` or
/// `vcd_parser::VcdFile`: this crate owns both the format's math (`Lab
/// → sRGB` above) and its parsing, since `example-doctor-color-swatch`
/// and `example-view-color-swatch` both need the identical parse.
#[derive(Debug, Clone, PartialEq)]
pub struct Swatch {
    pub label: String,
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// `label,L,a,b` per line -- blank lines and `#`-prefixed comments
/// skipped, same conventions `invariant-report`'s pipe-delimited format
/// already established for a plain project-authored text format. Not a
/// real interchange standard (unlike VCD) since no such standard exists
/// for "a named list of Lab colors" — this is this project's own
/// minimal shape for it, per ADR-011 kept generic (a label and three
/// numbers), not tied to any specific downstream use.
pub fn parse_swatches(text: &str) -> Result<Vec<Swatch>, String> {
    let mut swatches = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let [label, l, a, b] = fields.as_slice() else {
            return Err(format!("line {}: expected \"label,L,a,b\", got {line:?}", line_no + 1));
        };
        let parse_f64 = |field: &str, name: &str| -> Result<f64, String> {
            field.parse::<f64>().map_err(|_| format!("line {}: invalid {name} {field:?}", line_no + 1))
        };
        swatches.push(Swatch {
            label: label.to_string(),
            l: parse_f64(l, "L")?,
            a: parse_f64(a, "a")?,
            b: parse_f64(b, "b")?,
        });
    }
    Ok(swatches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: u8, expected: u8, tolerance: i16) {
        let diff = (actual as i16 - expected as i16).abs();
        assert!(diff <= tolerance, "expected ~{expected}, got {actual} (tolerance {tolerance})");
    }

    #[test]
    fn white_point_round_trips_to_white() {
        let rgb = lab_to_srgb(100.0, 0.0, 0.0);
        assert_eq!(rgb, Rgb { r: 255, g: 255, b: 255 });
    }

    #[test]
    fn zero_lightness_is_black() {
        let rgb = lab_to_srgb(0.0, 0.0, 0.0);
        assert_eq!(rgb, Rgb { r: 0, g: 0, b: 0 });
    }

    /// real, widely-published D65 Lab values for pure sRGB primaries
    /// (the same reference values cited across easyrgb.com/Lindbloom-
    /// style conversion tables) -- proves this isn't just self-
    /// consistent math, it lands on colors an independent source
    /// already published.
    #[test]
    fn published_srgb_red_lab_converts_back_to_red() {
        let rgb = lab_to_srgb(53.24, 80.09, 67.20);
        assert_close(rgb.r, 255, 2);
        assert_close(rgb.g, 0, 2);
        assert_close(rgb.b, 0, 2);
    }

    #[test]
    fn published_srgb_green_lab_converts_back_to_green() {
        let rgb = lab_to_srgb(87.74, -86.18, 83.18);
        assert_close(rgb.r, 0, 2);
        assert_close(rgb.g, 255, 2);
        assert_close(rgb.b, 0, 2);
    }

    #[test]
    fn published_srgb_blue_lab_converts_back_to_blue() {
        let rgb = lab_to_srgb(32.30, 79.20, -107.86);
        assert_close(rgb.r, 0, 2);
        assert_close(rgb.g, 0, 2);
        assert_close(rgb.b, 255, 2);
    }

    #[test]
    fn mid_gray_lab_is_a_neutral_rgb() {
        // L*=53.59 is the standard D65 mid-gray reference point (sRGB
        // 128,128,128 round-trips to approximately this L*, a*=b*=0).
        let rgb = lab_to_srgb(53.59, 0.0, 0.0);
        assert_close(rgb.r, 128, 2);
        assert_close(rgb.g, 128, 2);
        assert_close(rgb.b, 128, 2);
    }

    #[test]
    fn in_gamut_colors_are_reported_in_gamut() {
        assert!(lab_in_srgb_gamut(53.24, 80.09, 67.20)); // real sRGB red
        assert!(lab_in_srgb_gamut(50.0, 0.0, 0.0)); // neutral gray
    }

    #[test]
    fn wildly_saturated_lab_is_reported_out_of_gamut() {
        // no real sRGB color has a* this extreme at this lightness --
        // clamped `lab_to_srgb` would still return *something*, which
        // is exactly why a separate gamut check matters.
        assert!(!lab_in_srgb_gamut(50.0, 200.0, 200.0));
    }

    #[test]
    fn parse_swatches_reads_labeled_lab_rows() {
        let text = "# a comment\nred,53.24,80.09,67.20\n\nblue,32.30,79.20,-107.86\n";
        let swatches = parse_swatches(text).unwrap();
        assert_eq!(swatches.len(), 2);
        assert_eq!(swatches[0], Swatch { label: "red".into(), l: 53.24, a: 80.09, b: 67.20 });
        assert_eq!(swatches[1].label, "blue");
    }

    #[test]
    fn parse_swatches_rejects_a_malformed_row() {
        let err = parse_swatches("only,two\n").unwrap_err();
        assert!(err.contains("line 1"), "error should name the bad line: {err}");
    }

    #[test]
    fn parse_swatches_rejects_a_non_numeric_field() {
        let err = parse_swatches("bad,notanumber,0,0\n").unwrap_err();
        assert!(err.contains("line 1"), "error should name the bad line: {err}");
    }

    /// real published Lab values for the sRGB primaries and white sit
    /// exactly *on* the gamut boundary, so the independently-rounded
    /// published white point/matrix constants (4-5 significant figures
    /// each) don't round-trip them to exactly `[0, 1]` -- measured
    /// ~1e-4 off (`xyz_to_linear_srgb(lab_to_xyz(100, 0, 0))` lands
    /// green at `1.000076`, not `1.0`). `GAMUT_TOLERANCE` exists so
    /// that expected noise doesn't read as a real gamut violation;
    /// `wildly_saturated_lab_is_reported_out_of_gamut` above proves it
    /// still catches an actual one, which is off by orders of
    /// magnitude more than this.
    #[test]
    fn boundary_primaries_are_not_spuriously_out_of_gamut() {
        assert!(lab_in_srgb_gamut(100.0, 0.0, 0.0), "white");
        assert!(lab_in_srgb_gamut(87.74, -86.18, 83.18), "green");
        assert!(lab_in_srgb_gamut(32.30, 79.20, -107.86), "blue");
    }
}
