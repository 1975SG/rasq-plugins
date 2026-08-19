//! generic residual/confidence-band/outlier analysis over a predicted-
//! vs-measured pair, shared by every domain workbench not
//! rebuilt per-domain (see `docs/roadmap/Workbenches.md`'s Shared Bench
//! infrastructure list). Pure computation, no I/O, no WASM/WIT
//! dependency of its own -- plugin crates depend on this directly and
//! compile it into their own `wasm32` component, same as any other
//! pure-compute crate (`serde`, `toml`) already does elsewhere in this
//! project.

/// One (x, predicted, measured) triple — index-aligned with
/// `DeviationResult`'s own vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub predicted: f64,
    pub measured: f64,
}

#[derive(Debug, Clone)]
pub struct DeviationResult {
    /// `measured - predicted` per point, same order/length as input.
    pub residuals: Vec<f64>,
    /// pearson correlation coefficient between predicted and measured --
    /// `NaN` if either series has zero variance (undefined, not zero).
    pub correlation: f64,
    /// `predicted[i] ± CONFIDENCE_SIGMA * stddev(residuals)` per point --
    /// a heuristic band, not a rigorous parametric confidence interval
    /// (no distributional assumption is tested). Documented as a
    /// heuristic not overclaiming statistical rigor.
    pub band_upper: Vec<f64>,
    pub band_lower: Vec<f64>,
    /// indices where the measured value falls outside its own
    /// confidence band -- same threshold that defines the band, not a
    /// second independent one.
    pub outliers: Vec<usize>,
}

/// ~95% under a normal-ish assumption (1.96σ), rounded to a plain 2.0 --
/// already a heuristic per `DeviationResult::band_upper`'s own doc, so
/// there's no rigor lost by rounding a normal-approximation constant.
const CONFIDENCE_SIGMA: f64 = 2.0;

pub fn analyze(points: &[Point]) -> DeviationResult {
    let residuals: Vec<f64> = points.iter().map(|p| p.measured - p.predicted).collect();
    let predicted: Vec<f64> = points.iter().map(|p| p.predicted).collect();
    let measured: Vec<f64> = points.iter().map(|p| p.measured).collect();
    let correlation = pearson_correlation(&predicted, &measured);
    let sigma = stddev(&residuals);
    let band_upper: Vec<f64> = predicted.iter().map(|p| p + CONFIDENCE_SIGMA * sigma).collect();
    let band_lower: Vec<f64> = predicted.iter().map(|p| p - CONFIDENCE_SIGMA * sigma).collect();
    let outliers = residuals
        .iter()
        .enumerate()
        .filter(|(_, r)| r.abs() > CONFIDENCE_SIGMA * sigma)
        .map(|(i, _)| i)
        .collect();

    DeviationResult { residuals, correlation, band_upper, band_lower, outliers }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

fn pearson_correlation(a: &[f64], b: &[f64]) -> f64 {
    let ma = mean(a);
    let mb = mean(b);
    let cov: f64 = a.iter().zip(b.iter()).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let sa: f64 = a.iter().map(|x| (x - ma).powi(2)).sum::<f64>().sqrt();
    let sb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum::<f64>().sqrt();
    if sa == 0.0 || sb == 0.0 { f64::NAN } else { cov / (sa * sb) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(predicted: &[f64], measured: &[f64]) -> Vec<Point> {
        predicted
            .iter()
            .zip(measured.iter())
            .enumerate()
            .map(|(i, (&p, &m))| Point { x: i as f64, predicted: p, measured: m })
            .collect()
    }

    #[test]
    fn residuals_are_measured_minus_predicted() {
        let result = analyze(&pts(&[1.0, 2.0, 3.0], &[1.5, 1.5, 4.0]));
        assert_eq!(result.residuals, vec![0.5, -0.5, 1.0]);
    }

    #[test]
    fn perfectly_matched_series_correlate_at_one() {
        let result = analyze(&pts(&[1.0, 2.0, 3.0, 4.0], &[2.0, 4.0, 6.0, 8.0]));
        assert!((result.correlation - 1.0).abs() < 1e-9);
    }

    #[test]
    fn inversely_matched_series_correlate_at_negative_one() {
        let result = analyze(&pts(&[1.0, 2.0, 3.0, 4.0], &[8.0, 6.0, 4.0, 2.0]));
        assert!((result.correlation - -1.0).abs() < 1e-9);
    }

    #[test]
    fn zero_variance_predicted_series_gives_undefined_not_zero_correlation() {
        let result = analyze(&pts(&[5.0, 5.0, 5.0], &[1.0, 2.0, 3.0]));
        assert!(result.correlation.is_nan());
    }

    #[test]
    fn band_is_symmetric_around_predicted() {
        let result = analyze(&pts(&[10.0, 20.0, 30.0], &[11.0, 19.0, 31.0]));
        for i in 0..3 {
            let predicted = [10.0, 20.0, 30.0][i];
            let half_width = result.band_upper[i] - predicted;
            assert!((predicted - result.band_lower[i] - half_width).abs() < 1e-9);
            assert!(half_width > 0.0);
        }
    }

    #[test]
    fn a_residual_far_outside_the_noise_is_flagged_as_an_outlier() {
        // small, consistent noise except index 2, which is wildly off.
        let predicted = [10.0, 10.0, 10.0, 10.0, 10.0, 10.0];
        let measured = [10.1, 9.9, 50.0, 10.1, 9.9, 10.0];
        let result = analyze(&pts(&predicted, &measured));
        assert_eq!(result.outliers, vec![2]);
    }

    #[test]
    fn identical_predicted_and_measured_has_no_outliers_and_zero_width_band() {
        let result = analyze(&pts(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]));
        assert!(result.outliers.is_empty());
        for i in 0..3 {
            assert_eq!(result.band_upper[i], result.band_lower[i]);
        }
    }
}
