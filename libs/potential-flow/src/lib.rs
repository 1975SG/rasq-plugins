//! 2D potential flow past a cylinder -- the "Flow/CFD-style visualizer"
//! item from `docs/roadmap/Workbenches.md`'s stochastic/flow-simulation
//! tooling category. Engineering-sandbox framing, not a physics claim
//! (same stance `cellular-automaton`'s own doc comment already takes
//! for its category): inviscid, incompressible, irrotational flow —
//! the textbook closed-form solution, not a numerical CFD solver.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flow {
    pub free_stream_speed: f64,
    pub radius: f64,
}

impl Flow {
    /// radial velocity component in polar coordinates centered on the
    /// cylinder -- zero exactly on the surface (`r == radius`), the
    /// no-penetration boundary condition a solid cylinder must satisfy.
    ///
    /// `r == 0.0` is undefined (`self.radius / r` -- division by the
    /// query radius itself, not `self.radius`, so this isn't specific
    /// to a zero-radius cylinder) and returns `NaN` explicitly rather
    /// than whatever IEEE division happens to produce: `0.0/0.0` is
    /// already `NaN`, but a nonzero `self.radius` over `r == 0.0` is
    /// `Inf`, an inconsistent signal for the same "evaluated at the
    /// coordinate origin" mistake depending on an unrelated field.
    pub fn radial_velocity(&self, r: f64, theta: f64) -> f64 {
        if r == 0.0 {
            return f64::NAN;
        }
        self.free_stream_speed * (1.0 - (self.radius / r).powi(2)) * theta.cos()
    }

    /// tangential velocity component -- on the surface (`r == radius`)
    /// reduces to the classic `-2U sin(theta)`. Same `r == 0.0` guard
    /// and reasoning as `radial_velocity`.
    pub fn tangential_velocity(&self, r: f64, theta: f64) -> f64 {
        if r == 0.0 {
            return f64::NAN;
        }
        -self.free_stream_speed * (1.0 + (self.radius / r).powi(2)) * theta.sin()
    }

    pub fn speed(&self, r: f64, theta: f64) -> f64 {
        let vr = self.radial_velocity(r, theta);
        let vt = self.tangential_velocity(r, theta);
        (vr * vr + vt * vt).sqrt()
    }

    /// velocity at a Cartesian point, `None` inside the solid cylinder
    /// -- there's no flow there, not a placeholder for missing data.
    pub fn velocity_xy(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let r = (x * x + y * y).sqrt();
        if r < self.radius {
            return None;
        }
        let theta = y.atan2(x);
        let vr = self.radial_velocity(r, theta);
        let vt = self.tangential_velocity(r, theta);
        let vx = vr * theta.cos() - vt * theta.sin();
        let vy = vr * theta.sin() + vt * theta.cos();
        Some((vx, vy))
    }

    /// speed magnitude at a Cartesian point -- `0.0` inside the solid
    /// cylinder, the physically correct value (no fluid there), not a
    /// sentinel standing in for "no data."
    pub fn speed_xy(&self, x: f64, y: f64) -> f64 {
        self.velocity_xy(x, y).map(|(vx, vy)| (vx * vx + vy * vy).sqrt()).unwrap_or(0.0)
    }

    /// maximum speed found on the cylinder surface, scanned at
    /// `samples` evenly-spaced angles -- the classic textbook result is
    /// exactly `2 * free_stream_speed`, reached at the top/bottom of
    /// the cylinder (`theta = ±π/2`).
    pub fn max_surface_speed(&self, samples: usize) -> f64 {
        // `fold(0.0, f64::max)` over an empty range silently returns
        // the fold seed, `0.0` -- a real-looking "zero speed
        // everywhere" answer for "no samples were taken at all."
        // undefined, not zero, same convention as every other
        // degenerate case in this workbench-crate family.
        if samples == 0 {
            return f64::NAN;
        }
        // zero-or-negative radius hits `radial_velocity`/
        // `tangential_velocity`'s own `r == 0.0` guard at every sampled
        // angle (`speed(self.radius, theta)` evaluates at `r ==
        // self.radius`) -- but `f64::max` treats `NaN` as "not
        // comparable, keep the other value" not propagating
        // it, per its own documented semantics, so a `fold(0.0,
        // f64::max)` over an all-`NaN` sequence silently returns the
        // `0.0` seed instead. Checked explicitly here not
        // relying on fold to surface it, since it provably doesn't.
        if self.radius <= 0.0 {
            return f64::NAN;
        }
        (0..samples)
            .map(|i| {
                let theta = 2.0 * std::f64::consts::PI * i as f64 / samples as f64;
                self.speed(self.radius, theta)
            })
            .fold(0.0, f64::max)
    }

    /// angles on the surface that are local minima of speed *and*
    /// below `tolerance` -- the flow's stagnation points, theoretically
    /// at exactly `theta = 0` and `theta = π` (front and back of the
    /// cylinder). Local-minimum, not just "below tolerance": speed
    /// decreases smoothly toward each true zero and back up again, so
    /// thresholding alone returns every nearby sample clustered around
    /// each zero, not one point per stagnation region.
    pub fn stagnation_points(&self, samples: usize, tolerance: f64) -> Vec<f64> {
        let speeds: Vec<f64> = (0..samples)
            .map(|i| self.speed(self.radius, 2.0 * std::f64::consts::PI * i as f64 / samples as f64))
            .collect();
        (0..samples)
            .filter(|&i| {
                let prev = speeds[(i + samples - 1) % samples];
                let next = speeds[(i + 1) % samples];
                speeds[i] < tolerance && speeds[i] <= prev && speeds[i] <= next
            })
            .map(|i| 2.0 * std::f64::consts::PI * i as f64 / samples as f64)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    pub flow: Flow,
    pub x_min: f64,
    pub x_max: f64,
    pub x_steps: usize,
    pub y_min: f64,
    pub y_max: f64,
    pub y_steps: usize,
}

fn lerp(min: f64, max: f64, i: usize, steps: usize) -> f64 {
    if steps <= 1 {
        return min;
    }
    min + (max - min) * (i as f64) / ((steps - 1) as f64)
}

/// row-major speed-magnitude grid, matching `heatmap-view`'s own
/// documented row-major convention.
pub fn generate_speed_field(spec: &FieldSpec) -> Vec<f64> {
    let mut values = Vec::with_capacity(spec.x_steps * spec.y_steps);
    for j in 0..spec.y_steps {
        let y = lerp(spec.y_min, spec.y_max, j, spec.y_steps);
        for i in 0..spec.x_steps {
            let x = lerp(spec.x_min, spec.x_max, i, spec.x_steps);
            values.push(spec.flow.speed_xy(x, y));
        }
    }
    values
}

/// `key,value` per line -- `#` comments/blank lines skipped, same
/// plain-text convention every prior project-authored format in this
/// codebase already uses.
pub fn parse_field_spec(text: &str) -> Result<FieldSpec, String> {
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

    Ok(FieldSpec {
        flow: Flow { free_stream_speed: get_f64("free_stream_speed")?, radius: get_f64("radius")? },
        x_min: get_f64("x_min")?,
        x_max: get_f64("x_max")?,
        x_steps: get_usize("x_steps")?,
        y_min: get_f64("y_min")?,
        y_max: get_f64("y_max")?,
        y_steps: get_usize("y_steps")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected ~{expected}, got {actual}");
    }

    fn sample_flow() -> Flow {
        Flow { free_stream_speed: 10.0, radius: 1.0 }
    }

    #[test]
    fn radial_velocity_is_zero_on_the_surface_no_penetration() {
        let flow = sample_flow();
        for theta_deg in [0.0, 30.0, 90.0, 145.0, 200.0, 300.0] {
            let theta = theta_deg_f64(theta_deg);
            assert_close(flow.radial_velocity(flow.radius, theta), 0.0, 1e-9);
        }
    }

    fn theta_deg_f64(deg: f64) -> f64 {
        deg.to_radians()
    }

    #[test]
    fn velocity_at_r_zero_is_nan_not_a_silent_inf() {
        let flow = sample_flow();
        assert!(flow.radial_velocity(0.0, 0.0).is_nan());
        assert!(flow.tangential_velocity(0.0, 0.0).is_nan());
        // zero-radius cylinder evaluated on its own surface hits the
        // same r == 0.0 case via max_surface_speed's `self.speed(self.
        // radius, theta)` -- the same guard closes both routes, not two
        // separate checks.
        let degenerate = Flow { free_stream_speed: 10.0, radius: 0.0 };
        assert!(degenerate.max_surface_speed(16).is_nan());
    }

    #[test]
    fn max_surface_speed_of_zero_samples_is_nan_not_a_real_looking_zero() {
        let flow = sample_flow();
        assert!(
            flow.max_surface_speed(0).is_nan(),
            "zero samples is undefined, not a real 'zero speed everywhere' measurement"
        );
    }

    #[test]
    fn tangential_velocity_on_the_surface_matches_the_classic_formula() {
        let flow = sample_flow();
        for theta_deg in [0.0, 30.0, 90.0, 145.0, 200.0, 300.0] {
            let theta = theta_deg_f64(theta_deg);
            let expected = -2.0 * flow.free_stream_speed * theta.sin();
            assert_close(flow.tangential_velocity(flow.radius, theta), expected, 1e-9);
        }
    }

    #[test]
    fn max_surface_speed_is_twice_the_free_stream_speed() {
        let flow = sample_flow();
        assert_close(flow.max_surface_speed(3600), 2.0 * flow.free_stream_speed, 1e-3);
    }

    #[test]
    fn stagnation_points_are_found_at_the_front_and_back() {
        let flow = sample_flow();
        let points = flow.stagnation_points(3600, 0.05);
        assert_eq!(points.len(), 2, "expected exactly 2 stagnation points, got {points:?}");
        assert_close(points[0], 0.0, 0.01);
        assert_close(points[1], std::f64::consts::PI, 0.01);
    }

    #[test]
    fn far_from_the_cylinder_velocity_approaches_uniform_flow() {
        let flow = sample_flow();
        let (vx, vy) = flow.velocity_xy(1000.0, 0.0).unwrap();
        assert_close(vx, flow.free_stream_speed, 1e-3);
        assert_close(vy, 0.0, 1e-3);
    }

    #[test]
    fn inside_the_cylinder_there_is_no_flow() {
        let flow = sample_flow();
        assert_eq!(flow.velocity_xy(0.1, 0.1), None);
        assert_eq!(flow.speed_xy(0.1, 0.1), 0.0);
    }

    #[test]
    fn generate_speed_field_has_the_declared_dimensions_and_matches_direct_calls() {
        let spec = FieldSpec {
            flow: sample_flow(),
            x_min: -3.0,
            x_max: 3.0,
            x_steps: 5,
            y_min: -2.0,
            y_max: 2.0,
            y_steps: 4,
        };
        let values = generate_speed_field(&spec);
        assert_eq!(values.len(), 5 * 4);
        assert_close(values[0], spec.flow.speed_xy(-3.0, -2.0), 1e-9);
        assert_close(values[values.len() - 1], spec.flow.speed_xy(3.0, 2.0), 1e-9);
    }

    #[test]
    fn parse_field_spec_reads_a_real_declaration() {
        let text = "\
free_stream_speed,10
radius,1
x_min,-3
x_max,3
x_steps,40
y_min,-3
y_max,3
y_steps,40
";
        let spec = parse_field_spec(text).unwrap();
        assert_eq!(
            spec,
            FieldSpec {
                flow: Flow { free_stream_speed: 10.0, radius: 1.0 },
                x_min: -3.0,
                x_max: 3.0,
                x_steps: 40,
                y_min: -3.0,
                y_max: 3.0,
                y_steps: 40,
            }
        );
    }

    #[test]
    fn parse_field_spec_rejects_a_missing_field() {
        let err = parse_field_spec("free_stream_speed,10\n").unwrap_err();
        assert!(err.contains("radius"), "error should name the missing field: {err}");
    }
}
