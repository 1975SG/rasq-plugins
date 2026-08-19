//! generic distribution/regression analysis over raw sample data --
//! monte Carlo output → histogram (a PDF estimate), an empirical CDF,
//! and a least-squares linear fit -- shared by every domain workbench
//! not rebuilt per-domain (see `docs/roadmap/Workbenches.md`'s
//! shared Bench Infrastructure list). Pure computation, no I/O, no
//! WASM/WIT dependency of its own -- plugin crates depend on this
//! directly and compile it into their own `wasm32` component, same as
//! `deviation-analyzer` already does.
//!
//! `linear_regression`/`predict` exist specifically to feed
//! `deviation-analyzer::analyze` -- a fit's `predicted` series is
//! exactly the `Point::predicted` that crate already expects, so a
//! "regression/fit overlay" (this crate's own roadmap entry) is a
//! `linear_regression` call followed by a `deviation_analyzer::analyze`
//! call, not a second confidence-band implementation here.

/// one bin of a histogram -- `[x_start, x_end)`, except the last bin,
/// which is `[x_start, x_end]` (closed on both ends) so the sample
/// equal to `samples`'s own max value lands somewhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistogramBin {
    pub x_start: f64,
    pub x_end: f64,
    pub count: usize,
    /// `count / (samples.len() * bin_width)` -- integrates to ~1 across
    /// all bins, a real (if binned) probability density estimate, not
    /// just a raw count. `0.0` when `bin_width` is `0.0` (all samples
    /// identical -- see `histogram`'s own doc), since density is
    /// undefined at a single point, not infinite in any useful sense.
    pub density: f64,
}

/// `true` if any sample is `NaN`/`±Inf` -- a single non-finite value
/// corrupts every computation downstream (a `NaN` sample lands in bin
/// 0 via `as usize`'s own NaN-to-zero cast, silently, with no
/// indication anything was wrong; an `Inf` sample stretches `bin_width`
/// to `Inf` and collapses every other real sample into one bin).
/// shared by every function in this module that walks raw `samples`,
/// same "undefined, not a misleading real-looking answer" convention
/// this crate already uses for empty input.
fn has_non_finite(samples: &[f64]) -> bool {
    samples.iter().any(|s| !s.is_finite())
}

/// monte Carlo output → a histogram, i.e. a binned PDF estimate.
/// `bins` is clamped to `1` (an empty or zero-`bins` request would
/// otherwise silently produce nothing to plot). All-identical samples
/// (zero range) collapse to a single bin spanning `[v, v]` — width `0`,
/// so `density` is `0.0` per `HistogramBin`'s own doc not
/// dividing by zero. A `NaN`/`Inf` sample anywhere in the input returns
/// empty not a silently-corrupted histogram -- see
/// `has_non_finite`.
pub fn histogram(samples: &[f64], bins: usize) -> Vec<HistogramBin> {
    if samples.is_empty() || has_non_finite(samples) {
        return Vec::new();
    }
    let bins = bins.max(1);
    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    if max <= min {
        return vec![HistogramBin { x_start: min, x_end: max, count: samples.len(), density: 0.0 }];
    }

    let bin_width = (max - min) / bins as f64;
    let mut counts = vec![0usize; bins];
    for &s in samples {
        let idx = (((s - min) / bin_width) as usize).min(bins - 1);
        counts[idx] += 1;
    }

    counts
        .into_iter()
        .enumerate()
        .map(|(i, count)| HistogramBin {
            x_start: min + i as f64 * bin_width,
            x_end: min + (i + 1) as f64 * bin_width,
            count,
            density: count as f64 / (samples.len() as f64 * bin_width),
        })
        .collect()
}

/// one point of an empirical CDF -- `cumulative_fraction` is the
/// fraction of all samples `<= x` at this point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdfPoint {
    pub x: f64,
    pub cumulative_fraction: f64,
}

/// linear-interpolation percentile -- `p` in `[0, 1]` (e.g. `0.1` for
/// P10), same method as numpy's default `percentile`. `p` is clamped
/// to `[0, 1]` not treated as an error: a caller building a
/// P10/P50/P90 band from a fixed constant has no realistic way to pass
/// an out-of-range value by accident, only by a typo worth surfacing as
/// a clamped-but-plausible result, not a panic. `NaN` on empty
/// `samples` (undefined, not zero -- same convention every other
/// undefined case in this crate/`deviation-analyzer` already uses).
pub fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    if sorted.len() == 1 {
        return sorted[0];
    }
    let p = p.clamp(0.0, 1.0);
    let rank = p * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let frac = rank - lower as f64;
        sorted[lower] + frac * (sorted[upper] - sorted[lower])
    }
}

/// sorted samples paired with their running `(i+1)/n` cumulative
/// fraction -- one point per sample, not deduplicated at equal `x`
/// values, so repeated values still produce the correct vertical jump
/// when the result is plotted as a connected line. A `NaN`/`Inf`
/// sample anywhere returns empty not a real-looking curve with
/// a garbage point spliced in -- `total_cmp` sorts a `NaN` into a
/// defined position without panicking, which would otherwise let it
/// through silently. See `has_non_finite`.
pub fn empirical_cdf(samples: &[f64]) -> Vec<CdfPoint> {
    let n = samples.len();
    if n == 0 || has_non_finite(samples) {
        return Vec::new();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted
        .into_iter()
        .enumerate()
        .map(|(i, x)| CdfPoint { x, cumulative_fraction: (i + 1) as f64 / n as f64 })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct LinearFit {
    pub slope: f64,
    pub intercept: f64,
    /// coefficient of determination -- for ordinary least squares with
    /// an intercept, equal to the Pearson correlation squared. `NaN`
    /// when `x` has zero variance (undefined, not zero -- same
    /// convention `deviation-analyzer::analyze`'s own `correlation`
    /// field already uses).
    pub r_squared: f64,
}

/// ordinary least-squares fit of `y = slope * x + intercept` over
/// `(x, y)` pairs. `slope`/`intercept`/`r_squared` are all `NaN` when
/// `points` has fewer than 2 entries or zero `x`-variance -- nothing
/// meaningful to fit through a single point or a vertical spread.
pub fn linear_regression(points: &[(f64, f64)]) -> LinearFit {
    if points.len() < 2 {
        return LinearFit { slope: f64::NAN, intercept: f64::NAN, r_squared: f64::NAN };
    }
    let xs: Vec<f64> = points.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = points.iter().map(|(_, y)| *y).collect();
    let mx = mean(&xs);
    let my = mean(&ys);
    let cov: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| (x - mx) * (y - my)).sum();
    let var_x: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    if var_x == 0.0 {
        return LinearFit { slope: f64::NAN, intercept: f64::NAN, r_squared: f64::NAN };
    }
    let slope = cov / var_x;
    let intercept = my - slope * mx;
    let r = pearson_correlation(&xs, &ys);
    LinearFit { slope, intercept, r_squared: r * r }
}

/// `fit.slope * x + fit.intercept` -- a fitted line's predicted value at
/// `x`, meant to feed `deviation_analyzer::Point::predicted` directly
/// (see this crate's own module doc).
pub fn predict(fit: &LinearFit, x: f64) -> f64 {
    fit.slope * x + fit.intercept
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
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

    #[test]
    fn histogram_counts_every_sample_exactly_once() {
        let samples = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let bins = histogram(&samples, 5);
        let total: usize = bins.iter().map(|b| b.count).sum();
        assert_eq!(total, samples.len());
    }

    #[test]
    fn histogram_density_integrates_to_approximately_one() {
        let samples = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let bins = histogram(&samples, 5);
        let bin_width = bins[0].x_end - bins[0].x_start;
        let integral: f64 = bins.iter().map(|b| b.density * bin_width).sum();
        assert!((integral - 1.0).abs() < 1e-9);
    }

    #[test]
    fn histogram_max_value_lands_in_the_last_bin_not_dropped() {
        let samples = [0.0, 10.0];
        let bins = histogram(&samples, 2);
        assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 2);
        assert_eq!(bins.last().unwrap().count, 1);
    }

    #[test]
    fn histogram_of_identical_samples_is_one_zero_width_bin_with_zero_density() {
        let bins = histogram(&[5.0, 5.0, 5.0], 4);
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].count, 3);
        assert_eq!(bins[0].density, 0.0);
    }

    #[test]
    fn histogram_of_empty_samples_is_empty() {
        assert!(histogram(&[], 10).is_empty());
    }

    #[test]
    fn histogram_with_a_nan_or_inf_sample_is_empty_not_silently_wrong() {
        assert!(
            histogram(&[1.0, 2.0, f64::NAN, 3.0], 10).is_empty(),
            "a NaN sample would otherwise land in bin 0 via the as-usize cast, silently"
        );
        assert!(
            histogram(&[1.0, 2.0, f64::INFINITY, 3.0], 10).is_empty(),
            "an Inf sample would otherwise stretch bin_width to Inf, collapsing every other bin"
        );
    }

    #[test]
    fn empirical_cdf_with_a_nan_or_inf_sample_is_empty_not_silently_wrong() {
        assert!(empirical_cdf(&[1.0, 2.0, f64::NAN]).is_empty());
        assert!(empirical_cdf(&[1.0, 2.0, f64::NEG_INFINITY]).is_empty());
    }

    #[test]
    fn percentile_at_p50_of_an_odd_count_is_the_exact_middle_value() {
        assert_eq!(percentile(&[5.0, 1.0, 3.0, 2.0, 4.0], 0.5), 3.0);
    }

    #[test]
    fn percentile_interpolates_between_ranks_when_needed() {
        // rank = 0.5 * 3 = 1.5, halfway between sorted[1]=2.0 and sorted[2]=3.0.
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
    }

    #[test]
    fn percentile_p0_and_p1_are_min_and_max() {
        let samples = [4.0, 1.0, 7.0, 3.0];
        assert_eq!(percentile(&samples, 0.0), 1.0);
        assert_eq!(percentile(&samples, 1.0), 7.0);
    }

    #[test]
    fn percentile_clamps_out_of_range_p_instead_of_extrapolating() {
        let samples = [1.0, 2.0, 3.0];
        assert_eq!(percentile(&samples, 1.5), 3.0);
        assert_eq!(percentile(&samples, -0.5), 1.0);
    }

    #[test]
    fn percentile_of_a_single_sample_is_that_sample_at_any_p() {
        assert_eq!(percentile(&[42.0], 0.1), 42.0);
        assert_eq!(percentile(&[42.0], 0.9), 42.0);
    }

    #[test]
    fn percentile_of_empty_samples_is_undefined_not_zero() {
        assert!(percentile(&[], 0.5).is_nan());
    }

    #[test]
    fn empirical_cdf_reaches_exactly_one_at_the_last_point() {
        let cdf = empirical_cdf(&[3.0, 1.0, 2.0]);
        assert_eq!(cdf.last().unwrap().cumulative_fraction, 1.0);
    }

    #[test]
    fn empirical_cdf_is_sorted_ascending() {
        let cdf = empirical_cdf(&[5.0, 1.0, 3.0, 2.0, 4.0]);
        let xs: Vec<f64> = cdf.iter().map(|p| p.x).collect();
        assert_eq!(xs, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn linear_regression_recovers_an_exact_line() {
        let points: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 2.0 * i as f64 + 3.0)).collect();
        let fit = linear_regression(&points);
        assert!((fit.slope - 2.0).abs() < 1e-9);
        assert!((fit.intercept - 3.0).abs() < 1e-9);
        assert!((fit.r_squared - 1.0).abs() < 1e-9);
    }

    #[test]
    fn linear_regression_on_zero_variance_x_is_undefined_not_zero() {
        let fit = linear_regression(&[(5.0, 1.0), (5.0, 2.0), (5.0, 3.0)]);
        assert!(fit.slope.is_nan());
        assert!(fit.intercept.is_nan());
        assert!(fit.r_squared.is_nan());
    }

    #[test]
    fn linear_regression_on_fewer_than_two_points_is_undefined() {
        let fit = linear_regression(&[(1.0, 1.0)]);
        assert!(fit.slope.is_nan());
    }

    #[test]
    fn predict_matches_the_fitted_line() {
        let fit = LinearFit { slope: 2.0, intercept: 1.0, r_squared: 1.0 };
        assert_eq!(predict(&fit, 5.0), 11.0);
    }
}
