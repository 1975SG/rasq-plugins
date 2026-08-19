//! amplitude-modulated (AM) clustered-dot halftone screening -- the
//! "Halftone/screening threshold and dot-pattern visualization" item
//! from `docs/roadmap/Workbenches.md`'s color-science tooling
//! category. Pure and Core-side-free, same "hand-roll small testable
//! math" habit as `colorspace`/`colormap.rs` -- this is the standard
//! print/prepress technique (a rotated, periodic threshold surface
//! compared against a gray level), not a project-specific invention.

/// rotates `(x, y)` by `angle_deg` degrees -- every screen frequency is
/// evaluated in this rotated coordinate space, which is what gives a
/// halftone screen its characteristic angle.
fn rotate(x: f64, y: f64, angle_deg: f64) -> (f64, f64) {
    let theta = angle_deg.to_radians();
    let (sin, cos) = theta.sin_cos();
    (x * cos + y * sin, -x * sin + y * cos)
}

/// classic clustered-dot spot function -- a product of two rotated
/// cosines, normalized to `[0, 1]`. Symmetric around `0.5` by
/// construction (`cos * cos` is as often positive as negative over a
/// full period), which is exactly what makes `average_coverage` land
/// on the input `gray` level at the midpoint -- verified numerically in
/// this module's own tests, not just asserted.
fn spot_threshold(x: f64, y: f64, angle_deg: f64, frequency: f64) -> f64 {
    let (rx, ry) = rotate(x, y, angle_deg);
    0.5 * (1.0 + (2.0 * std::f64::consts::PI * frequency * rx).cos() * (2.0 * std::f64::consts::PI * frequency * ry).cos())
}

/// `true` when the halftone dot at grid position `(x, y)` (any real
/// coordinates -- the screen is periodic, not bounded to a single
/// cell) is "on" (ink) for a given `gray` input level (`0` = no ink,
/// `1` = full coverage) at the given screen `angle_deg`/`frequency`.
pub fn dot_is_on(gray: f64, x: f64, y: f64, angle_deg: f64, frequency: f64) -> bool {
    gray >= spot_threshold(x, y, angle_deg, frequency)
}

/// numerically integrates `dot_is_on` over an `n * n` sample grid
/// spanning `[0, 1) x [0, 1)` -- the fraction of samples that land "on"
/// approximates real dot-area coverage. Used both as this module's own
/// self-consistency check (coverage should track the input `gray`
/// level) and by `example-view-halftone` to show a measured coverage
/// next to each screen's declared `gray`, same cross-check instinct
/// `colorspace::lab_in_srgb_gamut` already established.
pub fn average_coverage(gray: f64, angle_deg: f64, frequency: f64, samples: usize) -> f64 {
    // undefined, not zero, same convention `distribution-analysis`
    // already uses for its own degenerate cases (empty samples, zero
    // x-variance) -- `0.0` here would silently read as "a real
    // measurement of zero coverage," not "no measurement was taken."
    if samples == 0 {
        return f64::NAN;
    }
    let mut on = 0usize;
    for i in 0..samples {
        for j in 0..samples {
            let x = i as f64 / samples as f64;
            let y = j as f64 / samples as f64;
            if dot_is_on(gray, x, y, angle_deg, frequency) {
                on += 1;
            }
        }
    }
    on as f64 / (samples * samples) as f64
}

/// absolute angular separation between two screen angles, folded into
/// `[0, 90]` -- halftone screens are periodic every 180° and mirror-
/// symmetric, so e.g. 5° and 175° are the same screen angle, and the
/// meaningful separation between any two angles never exceeds 90°.
fn angular_separation(a_deg: f64, b_deg: f64) -> f64 {
    let diff = (a_deg - b_deg).rem_euclid(180.0);
    diff.min(180.0 - diff).min(90.0)
}

/// classic prepress practice: process-color screens need enough
/// angular separation from each other or their dot grids beat against
/// each other, producing a visible moiré interference pattern -- the
/// reason real 4-color printing uses fixed, deliberately-separated
/// angles (commonly cited: cyan 15°, black 45°, magenta 75°, yellow
/// 0°/90° -- yellow closest to its neighbors since its low visual
/// contrast tolerates it). `threshold_deg` names how close is too
/// close; this function doesn't hardcode a "correct" angle set itself
/// (per ADR-011, no fixed palette baked into a crate) — a caller
/// supplies whatever angles a project actually declared.
pub fn moire_risk(angle_a_deg: f64, angle_b_deg: f64, threshold_deg: f64) -> bool {
    angular_separation(angle_a_deg, angle_b_deg) < threshold_deg
}

/// one named screen from a `.halftone-screens` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub label: String,
    pub angle_deg: f64,
    pub frequency: f64,
    pub gray: f64,
}

/// `label,angle_deg,frequency,gray` per line -- same minimal "label +
/// three numbers" shape `colorspace::parse_swatches` already
/// established for its own project-authored format, blank lines and
/// `#` comments skipped.
pub fn parse_screens(text: &str) -> Result<Vec<Screen>, String> {
    let mut screens = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let [label, angle, frequency, gray] = fields.as_slice() else {
            return Err(format!(
                "line {}: expected \"label,angle_deg,frequency,gray\", got {line:?}",
                line_no + 1
            ));
        };
        let parse_f64 = |field: &str, name: &str| -> Result<f64, String> {
            field.parse::<f64>().map_err(|_| format!("line {}: invalid {name} {field:?}", line_no + 1))
        };
        screens.push(Screen {
            label: label.to_string(),
            angle_deg: parse_f64(angle, "angle_deg")?,
            frequency: parse_f64(frequency, "frequency")?,
            gray: parse_f64(gray, "gray")?,
        });
    }
    Ok(screens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected ~{expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn zero_gray_has_almost_no_coverage() {
        assert_close(average_coverage(0.0, 15.0, 6.0, 200), 0.0, 0.01);
    }

    #[test]
    fn zero_samples_is_nan_not_a_real_looking_zero() {
        assert!(
            average_coverage(0.5, 15.0, 6.0, 0).is_nan(),
            "zero samples is undefined, not a real coverage measurement of 0.0"
        );
    }

    #[test]
    fn full_gray_is_fully_covered() {
        assert_close(average_coverage(1.0, 15.0, 6.0, 200), 1.0, 0.001);
    }

    #[test]
    fn mid_gray_covers_about_half_the_cell_regardless_of_angle() {
        for angle in [0.0, 15.0, 45.0, 75.0, 90.0] {
            assert_close(average_coverage(0.5, angle, 6.0, 200), 0.5, 0.02);
        }
    }

    #[test]
    fn coverage_increases_monotonically_with_gray() {
        let coverages: Vec<f64> =
            [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|&g| average_coverage(g, 20.0, 5.0, 150)).collect();
        for pair in coverages.windows(2) {
            assert!(pair[1] >= pair[0] - 1e-9, "coverage should never decrease as gray rises: {coverages:?}");
        }
    }

    #[test]
    fn same_angle_screens_have_zero_separation() {
        assert!(moire_risk(15.0, 15.0, 10.0));
    }

    /// real, widely-cited classic 4-color process screening angles
    /// (cyan 15°, black 45°, magenta 75°) — every pairwise separation
    /// among this trio is exactly 30°, the traditional minimum
    /// separation used specifically to avoid moiré, so none should be
    /// flagged at a 25° threshold.
    #[test]
    fn classic_cmyk_trio_angles_are_not_flagged_as_moire_risk() {
        let angles = [("cyan", 15.0), ("black", 45.0), ("magenta", 75.0)];
        for i in 0..angles.len() {
            for j in (i + 1)..angles.len() {
                assert!(
                    !moire_risk(angles[i].1, angles[j].1, 25.0),
                    "{} and {} should not be flagged",
                    angles[i].0,
                    angles[j].0
                );
            }
        }
    }

    #[test]
    fn angles_close_together_are_flagged_as_moire_risk() {
        assert!(moire_risk(15.0, 20.0, 10.0));
    }

    /// screens are periodic every 180° (and mirror-symmetric within
    /// that), so 5° and 175° are effectively the same screen angle --
    /// should read as maximally *at* risk, not accidentally treated as
    /// far apart by a naive `(a - b).abs()`.
    #[test]
    fn angles_near_the_180_degree_wrap_are_recognized_as_close() {
        assert!(moire_risk(5.0, 175.0, 15.0));
    }

    #[test]
    fn parse_screens_reads_labeled_rows() {
        let text = "# comment\ncyan,15,6,0.4\n\nmagenta,75,6,0.3\n";
        let screens = parse_screens(text).unwrap();
        assert_eq!(screens.len(), 2);
        assert_eq!(screens[0], Screen { label: "cyan".into(), angle_deg: 15.0, frequency: 6.0, gray: 0.4 });
        assert_eq!(screens[1].label, "magenta");
    }

    #[test]
    fn parse_screens_rejects_a_malformed_row() {
        let err = parse_screens("only,three,fields\n").unwrap_err();
        assert!(err.contains("line 1"), "error should name the bad line: {err}");
    }
}
