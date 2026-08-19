//! black-Scholes option pricing -- the "Price/ROI-style surface
//! plotter" item from `docs/roadmap/Workbenches.md`'s stochastic/
//! flow-simulation tooling category. Standard closed-form option
//! pricing, verified against a widely-cited textbook reference case,
//! not a project-specific derivation.

/// standard normal CDF via the Abramowitz & Stegun rational
/// approximation (formula 26.2.17) — accurate to ~7.5e-8, a real,
/// independently-published approximation, same "real published
/// reference" standard this project's other math crates already hold
/// to, not an internally-invented one.
pub fn normal_cdf(x: f64) -> f64 {
    if x < 0.0 {
        return 1.0 - normal_cdf(-x);
    }
    const B1: f64 = 0.319381530;
    const B2: f64 = -0.356563782;
    const B3: f64 = 1.781477937;
    const B4: f64 = -1.821255978;
    const B5: f64 = 1.330274429;
    const P: f64 = 0.2316419;
    const INV_SQRT_2PI: f64 = 0.398942280401;

    let t = 1.0 / (1.0 + P * x);
    let poly = t * (B1 + t * (B2 + t * (B3 + t * (B4 + t * B5))));
    1.0 - INV_SQRT_2PI * (-x * x / 2.0).exp() * poly
}

fn d1_d2(spot: f64, strike: f64, rate: f64, volatility: f64, time_to_expiry: f64) -> (f64, f64) {
    let d1 = ((spot / strike).ln() + (rate + volatility * volatility / 2.0) * time_to_expiry)
        / (volatility * time_to_expiry.sqrt());
    let d2 = d1 - volatility * time_to_expiry.sqrt();
    (d1, d2)
}

/// european call option price, standard Black-Scholes closed form.
pub fn call_price(spot: f64, strike: f64, rate: f64, volatility: f64, time_to_expiry: f64) -> f64 {
    let (d1, d2) = d1_d2(spot, strike, rate, volatility, time_to_expiry);
    spot * normal_cdf(d1) - strike * (-rate * time_to_expiry).exp() * normal_cdf(d2)
}

/// european put option price, standard Black-Scholes closed form.
pub fn put_price(spot: f64, strike: f64, rate: f64, volatility: f64, time_to_expiry: f64) -> f64 {
    let (d1, d2) = d1_d2(spot, strike, rate, volatility, time_to_expiry);
    strike * (-rate * time_to_expiry).exp() * normal_cdf(-d2) - spot * normal_cdf(-d1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Call,
    Put,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceSpec {
    pub spot: f64,
    pub rate: f64,
    pub volatility: f64,
    pub option: OptionKind,
    pub strike_min: f64,
    pub strike_max: f64,
    pub strike_steps: usize,
    pub expiry_min: f64,
    pub expiry_max: f64,
    pub expiry_steps: usize,
}

impl SurfaceSpec {
    pub fn price_at(&self, strike: f64, time_to_expiry: f64) -> f64 {
        match self.option {
            OptionKind::Call => call_price(self.spot, strike, self.rate, self.volatility, time_to_expiry),
            OptionKind::Put => put_price(self.spot, strike, self.rate, self.volatility, time_to_expiry),
        }
    }
}

fn lerp(min: f64, max: f64, i: usize, steps: usize) -> f64 {
    if steps <= 1 {
        return min;
    }
    min + (max - min) * (i as f64) / ((steps - 1) as f64)
}

/// row-major, one row per expiry step (matching `heatmap-view`'s own
/// documented row-major convention) -- strike varies along a row,
/// expiry varies down the grid, so a column reads as "this strike's
/// price decaying/growing as expiry moves."
pub fn generate_surface(spec: &SurfaceSpec) -> Vec<f64> {
    let mut values = Vec::with_capacity(spec.strike_steps * spec.expiry_steps);
    for e in 0..spec.expiry_steps {
        let expiry = lerp(spec.expiry_min, spec.expiry_max, e, spec.expiry_steps);
        for s in 0..spec.strike_steps {
            let strike = lerp(spec.strike_min, spec.strike_max, s, spec.strike_steps);
            values.push(spec.price_at(strike, expiry));
        }
    }
    values
}

/// `key,value` per line -- `#` comments/blank lines skipped, same
/// plain-text convention every prior project-authored format in this
/// codebase already uses.
pub fn parse_surface_spec(text: &str) -> Result<SurfaceSpec, String> {
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(',') else {
            return Err(format!("line {}: expected \"key,value\", got {line:?}", line_no + 1));
        };
        fields.insert(key.trim().to_string(), value.trim().to_string());
    }

    let get_f64 = |name: &str| -> Result<f64, String> {
        let raw = fields.get(name).ok_or_else(|| format!("missing {name:?}"))?;
        raw.parse().map_err(|_| format!("invalid {name} {raw:?}"))
    };
    let get_usize = |name: &str| -> Result<usize, String> {
        let raw = fields.get(name).ok_or_else(|| format!("missing {name:?}"))?;
        raw.parse().map_err(|_| format!("invalid {name} {raw:?}"))
    };
    let option = match fields.get("option").map(String::as_str) {
        Some("call") => OptionKind::Call,
        Some("put") => OptionKind::Put,
        Some(other) => return Err(format!("unknown option kind {other:?} (expected \"call\" or \"put\")")),
        None => return Err("missing \"option\"".to_string()),
    };

    Ok(SurfaceSpec {
        spot: get_f64("spot")?,
        rate: get_f64("rate")?,
        volatility: get_f64("volatility")?,
        option,
        strike_min: get_f64("strike_min")?,
        strike_max: get_f64("strike_max")?,
        strike_steps: get_usize("strike_steps")?,
        expiry_min: get_f64("expiry_min")?,
        expiry_max: get_f64("expiry_max")?,
        expiry_steps: get_usize("expiry_steps")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected ~{expected}, got {actual}");
    }

    #[test]
    fn normal_cdf_matches_published_standard_normal_table_values() {
        assert_close(normal_cdf(0.0), 0.5, 1e-6);
        assert_close(normal_cdf(1.0), 0.8413, 1e-4);
        assert_close(normal_cdf(1.96), 0.9750, 1e-3);
        assert_close(normal_cdf(-1.0), 0.1587, 1e-4);
    }

    /// widely-cited Black-Scholes textbook example: spot=strike=
    /// 100, r=5%, sigma=20%, T=1 year. Call ~10.45, put ~5.57 are the
    /// commonly-reproduced reference values for exactly this parameter
    /// set (the same numbers any standard options calculator returns
    /// for this input).
    #[test]
    fn call_price_matches_the_published_textbook_reference_case() {
        let price = call_price(100.0, 100.0, 0.05, 0.2, 1.0);
        assert_close(price, 10.45, 0.01);
    }

    #[test]
    fn put_price_matches_the_published_textbook_reference_case() {
        let price = put_price(100.0, 100.0, 0.05, 0.2, 1.0);
        assert_close(price, 5.57, 0.01);
    }

    /// put-call parity (`C - P = S - K*e^(-rT)`) is a real, model-
    /// independent no-arbitrage identity -- holds regardless of the
    /// specific parameters, not just the one textbook case above.
    #[test]
    fn put_call_parity_holds_across_several_parameter_sets() {
        for (spot, strike, rate, vol, expiry) in
            [(100.0, 100.0, 0.05, 0.2, 1.0), (50.0, 60.0, 0.03, 0.35, 0.5), (200.0, 180.0, 0.01, 0.15, 2.0)]
        {
            let call = call_price(spot, strike, rate, vol, expiry);
            let put = put_price(spot, strike, rate, vol, expiry);
            let expected_diff = spot - strike * (-rate * expiry).exp();
            assert_close(call - put, expected_diff, 1e-6);
        }
    }

    fn sample_spec() -> SurfaceSpec {
        SurfaceSpec {
            spot: 100.0,
            rate: 0.05,
            volatility: 0.2,
            option: OptionKind::Call,
            strike_min: 80.0,
            strike_max: 120.0,
            strike_steps: 5,
            expiry_min: 0.5,
            expiry_max: 1.5,
            expiry_steps: 3,
        }
    }

    #[test]
    fn generate_surface_has_the_declared_dimensions_and_matches_direct_price_calls() {
        let spec = sample_spec();
        let values = generate_surface(&spec);
        assert_eq!(values.len(), spec.strike_steps * spec.expiry_steps);
        // row 0 (expiry_min), column 0 (strike_min).
        assert_close(values[0], spec.price_at(80.0, 0.5), 1e-9);
        // last row (expiry_max), last column (strike_max).
        assert_close(values[values.len() - 1], spec.price_at(120.0, 1.5), 1e-9);
    }

    #[test]
    fn parse_surface_spec_reads_a_real_declaration() {
        let text = "\
spot,100
rate,0.05
volatility,0.2
option,call
strike_min,80
strike_max,120
strike_steps,20
expiry_min,0.1
expiry_max,2.0
expiry_steps,20
";
        let spec = parse_surface_spec(text).unwrap();
        assert_eq!(spec, SurfaceSpec {
            spot: 100.0,
            rate: 0.05,
            volatility: 0.2,
            option: OptionKind::Call,
            strike_min: 80.0,
            strike_max: 120.0,
            strike_steps: 20,
            expiry_min: 0.1,
            expiry_max: 2.0,
            expiry_steps: 20,
        });
    }

    #[test]
    fn parse_surface_spec_rejects_a_missing_field() {
        let err = parse_surface_spec("spot,100\noption,call\n").unwrap_err();
        assert!(err.contains("rate"), "error should name the missing field: {err}");
    }

    #[test]
    fn parse_surface_spec_rejects_an_unknown_option_kind() {
        let text = "spot,100\nrate,0.05\nvolatility,0.2\noption,straddle\nstrike_min,1\nstrike_max,2\nstrike_steps,2\nexpiry_min,1\nexpiry_max,2\nexpiry_steps,2\n";
        let err = parse_surface_spec(text).unwrap_err();
        assert!(err.contains("straddle"), "error should name the bad value: {err}");
    }
}
