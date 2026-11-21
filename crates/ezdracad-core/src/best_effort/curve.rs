//! Evaluation of cubic spline polylines from their control polygon.

/// Uniform cubic B-spline through the control polygon `points`.
///
/// Every span between two consecutive points is divided into `divisions`
/// pieces. An open curve gets one mirrored control point before the first and
/// after the last point, so that it starts and ends on the polygon. A closed
/// curve wraps around; a repeated closing point is not counted twice.
pub(crate) fn cubic_b_spline(points: &[[f64; 2]], divisions: usize, closed: bool) -> Vec<[f64; 2]> {
    if points.len() < 2 || divisions == 0 {
        return points.to_vec();
    }
    let basis = |p: [[f64; 2]; 4], t: f64| -> [f64; 2] {
        let weights = [
            (1.0 - t).powi(3) / 6.0,
            (3.0 * t.powi(3) - 6.0 * t.powi(2) + 4.0) / 6.0,
            (-3.0 * t.powi(3) + 3.0 * t.powi(2) + 3.0 * t + 1.0) / 6.0,
            t.powi(3) / 6.0,
        ];
        let mut value = [0.0; 2];
        for axis in 0..2 {
            value[axis] = (0..4).map(|index| weights[index] * p[index][axis]).sum();
        }
        value
    };
    let mut result = Vec::new();
    if closed {
        let ring = if points.first() == points.last() && points.len() > 2 {
            &points[..points.len() - 1]
        } else {
            points
        };
        let count = ring.len();
        for span in 0..count {
            let control = [
                ring[(span + count - 1) % count],
                ring[span],
                ring[(span + 1) % count],
                ring[(span + 2) % count],
            ];
            for step in 0..divisions {
                result.push(basis(control, step as f64 / divisions as f64));
            }
        }
        if let Some(first) = result.first().copied() {
            result.push(first);
        }
        return result;
    }
    let mirror = |near: [f64; 2], far: [f64; 2]| [2.0 * near[0] - far[0], 2.0 * near[1] - far[1]];
    let mut control = Vec::with_capacity(points.len() + 2);
    control.push(mirror(points[0], points[1]));
    control.extend_from_slice(points);
    control.push(mirror(points[points.len() - 1], points[points.len() - 2]));
    for span in 0..points.len() - 1 {
        let window = [
            control[span],
            control[span + 1],
            control[span + 2],
            control[span + 3],
        ];
        for step in 0..divisions {
            result.push(basis(window, step as f64 / divisions as f64));
        }
        if span + 2 == points.len() {
            result.push(basis(window, 1.0));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12
    }

    #[test]
    fn open_curve_starts_and_ends_on_the_polygon() {
        let points = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let curve = cubic_b_spline(&points, 10, false);
        assert_eq!(curve.len(), 3 * 10 + 1);
        assert!(near(curve[0], points[0]));
        assert!(near(curve[30], points[3]));
        // Interior control points are approached, not interpolated.
        assert!(!near(curve[10], points[1]));
    }

    #[test]
    fn collinear_equidistant_points_stay_on_the_line() {
        let points = [[0.0, 0.0], [1.0, 2.0], [2.0, 4.0], [3.0, 6.0]];
        for (index, point) in cubic_b_spline(&points, 4, false).iter().enumerate() {
            assert!(near(*point, [index as f64 / 4.0, index as f64 / 2.0]));
        }
    }

    #[test]
    fn closed_curve_wraps_and_ignores_a_repeated_closing_point() {
        let ring = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let mut repeated = ring.to_vec();
        repeated.push(ring[0]);
        let a = cubic_b_spline(&ring, 5, true);
        let b = cubic_b_spline(&repeated, 5, true);
        assert_eq!(a.len(), 4 * 5 + 1);
        assert_eq!(a, b);
        assert!(near(a[0], a[20]));
    }

    #[test]
    fn degenerate_input_is_returned_unchanged() {
        assert_eq!(cubic_b_spline(&[[1.0, 2.0]], 10, false), vec![[1.0, 2.0]]);
        let points = [[0.0, 0.0], [1.0, 1.0]];
        assert_eq!(cubic_b_spline(&points, 0, false), points.to_vec());
    }
}
