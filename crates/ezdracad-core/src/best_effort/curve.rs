//! Evaluation of curve polylines from their stored points.
//!
//! Every function divides each span into `divisions` pieces and returns the
//! points of the resulting open or closed run. Degenerate input is returned
//! unchanged.

type Point = [f64; 2];

fn ring(points: &[Point]) -> &[Point] {
    // A closed curve may repeat its first point at the end; it counts once.
    if points.len() > 2 && points.first() == points.last() {
        &points[..points.len() - 1]
    } else {
        points
    }
}

fn mid(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn combine<const N: usize>(weights: [f64; N], points: [Point; N]) -> Point {
    let mut value = [0.0; 2];
    for (weight, point) in weights.iter().zip(points) {
        value[0] += weight * point[0];
        value[1] += weight * point[1];
    }
    value
}

fn cubic_basis(points: [Point; 4], t: f64) -> Point {
    combine(
        [
            (1.0 - t).powi(3) / 6.0,
            (3.0 * t.powi(3) - 6.0 * t.powi(2) + 4.0) / 6.0,
            (-3.0 * t.powi(3) + 3.0 * t.powi(2) + 3.0 * t + 1.0) / 6.0,
            t.powi(3) / 6.0,
        ],
        points,
    )
}

fn steps(divisions: usize) -> impl Iterator<Item = f64> {
    (0..divisions).map(move |step| step as f64 / divisions as f64)
}

/// Number of spans the evaluation of `points` produces, for budget checks.
pub(crate) fn spans(kind: CurveKind, points: usize, closed: bool, repeated_end: bool) -> usize {
    let ring = if closed && repeated_end && points > 2 {
        points - 1
    } else {
        points
    };
    match kind {
        CurveKind::Quadratic if closed => ring,
        CurveKind::Quadratic => points.saturating_sub(2),
        CurveKind::Cubic | CurveKind::CubicThrough if closed => ring,
        CurveKind::Cubic | CurveKind::CubicThrough => points.saturating_sub(1),
        CurveKind::Bezier => points.saturating_sub(1) / 2,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurveKind {
    Quadratic,
    Cubic,
    CubicThrough,
    Bezier,
}

pub(crate) fn evaluate(
    kind: CurveKind,
    points: &[Point],
    divisions: usize,
    closed: bool,
) -> Vec<Point> {
    if divisions == 0 {
        return points.to_vec();
    }
    match kind {
        CurveKind::Quadratic => quadratic(points, divisions, closed),
        CurveKind::Cubic => cubic(points, divisions, closed),
        CurveKind::CubicThrough => cubic_through(points, divisions, closed),
        CurveKind::Bezier => bezier(points, divisions),
    }
}

/// Uniform quadratic spline: each interior point is the control point of one
/// span that runs between the midpoints of its two edges. An open curve
/// starts and ends on its first and last point.
fn quadratic(points: &[Point], divisions: usize, closed: bool) -> Vec<Point> {
    let span = |from: Point, control: Point, to: Point, result: &mut Vec<Point>| {
        for t in steps(divisions) {
            result.push(combine(
                [(1.0 - t).powi(2), 2.0 * t * (1.0 - t), t * t],
                [from, control, to],
            ));
        }
    };
    let mut result = Vec::new();
    if closed {
        let ring = ring(points);
        let count = ring.len();
        if count < 3 {
            return points.to_vec();
        }
        for index in 0..count {
            let control = ring[(index + 1) % count];
            span(
                mid(ring[index], control),
                control,
                mid(control, ring[(index + 2) % count]),
                &mut result,
            );
        }
        result.push(result[0]);
        return result;
    }
    let count = points.len();
    if count < 3 {
        return points.to_vec();
    }
    for index in 1..count - 1 {
        let from = if index == 1 {
            points[0]
        } else {
            mid(points[index - 1], points[index])
        };
        let to = if index == count - 2 {
            points[count - 1]
        } else {
            mid(points[index], points[index + 1])
        };
        span(from, points[index], to, &mut result);
    }
    result.push(points[count - 1]);
    result
}

/// Uniform cubic B-spline with `control` as its control points. An open curve
/// gets one mirrored control point before the first and after the last point,
/// so that it starts and ends on them.
fn cubic(control: &[Point], divisions: usize, closed: bool) -> Vec<Point> {
    let mut result = Vec::new();
    if closed {
        let ring = ring(control);
        let count = ring.len();
        if count < 2 {
            return control.to_vec();
        }
        for span in 0..count {
            let window = [
                ring[(span + count - 1) % count],
                ring[span],
                ring[(span + 1) % count],
                ring[(span + 2) % count],
            ];
            for t in steps(divisions) {
                result.push(cubic_basis(window, t));
            }
        }
        result.push(result[0]);
        return result;
    }
    if control.len() < 2 {
        return control.to_vec();
    }
    let mirror = |near: Point, far: Point| [2.0 * near[0] - far[0], 2.0 * near[1] - far[1]];
    let mut padded = Vec::with_capacity(control.len() + 2);
    padded.push(mirror(control[0], control[1]));
    padded.extend_from_slice(control);
    padded.push(mirror(
        control[control.len() - 1],
        control[control.len() - 2],
    ));
    for span in 0..control.len() - 1 {
        let window = [
            padded[span],
            padded[span + 1],
            padded[span + 2],
            padded[span + 3],
        ];
        for t in steps(divisions) {
            result.push(cubic_basis(window, t));
        }
        if span + 2 == control.len() {
            result.push(cubic_basis(window, 1.0));
        }
    }
    result
}

/// Cubic B-spline that passes through `points`: its control points solve
/// `(c[i-1] + 4 c[i] + c[i+1]) / 6 = p[i]`.
fn cubic_through(points: &[Point], divisions: usize, closed: bool) -> Vec<Point> {
    if closed {
        let ring = ring(points);
        if ring.len() < 3 {
            return points.to_vec();
        }
        let control: Vec<Point> = solve_cyclic(ring);
        return cubic(&control, divisions, true);
    }
    if points.len() < 2 {
        return points.to_vec();
    }
    cubic(&solve_open(points), divisions, false)
}

/// Open case: the mirrored end control points turn the first and last rows
/// into `6 c = 6 p`, leaving a tridiagonal system.
fn solve_open(points: &[Point]) -> Vec<Point> {
    let count = points.len();
    let mut control = vec![[0.0; 2]; count];
    for axis in 0..2 {
        let mut upper = vec![0.0; count];
        let mut right = vec![0.0; count];
        // Rows: 6 c0 = 6 p0; c[i-1] + 4 c[i] + c[i+1] = 6 p[i]; 6 c[n-1] = 6 p[n-1].
        right[0] = points[0][axis];
        for index in 1..count {
            let (lower, diagonal, above) = if index == count - 1 {
                (0.0, 6.0, 0.0)
            } else {
                (1.0, 4.0, 1.0)
            };
            let pivot = diagonal - lower * upper[index - 1];
            upper[index] = above / pivot;
            right[index] = (6.0 * points[index][axis] - lower * right[index - 1]) / pivot;
        }
        control[count - 1][axis] = right[count - 1];
        for index in (0..count - 1).rev() {
            control[index][axis] = right[index] - upper[index] * control[index + 1][axis];
        }
    }
    control
}

/// Closed case: the same rows with indices taken around the ring.
fn solve_cyclic(points: &[Point]) -> Vec<Point> {
    let count = points.len();
    let mut control = vec![[0.0; 2]; count];
    // Sherman-Morrison on the cyclic tridiagonal matrix with rows (1, 4, 1).
    let gamma = -4.0;
    let tridiagonal = |right: &[f64]| -> Vec<f64> {
        let mut upper = vec![0.0; count];
        let mut value = vec![0.0; count];
        let diagonal = |index: usize| {
            if index == 0 {
                4.0 - gamma
            } else if index == count - 1 {
                4.0 - 1.0 / gamma
            } else {
                4.0
            }
        };
        upper[0] = 1.0 / diagonal(0);
        value[0] = right[0] / diagonal(0);
        for index in 1..count {
            let pivot = diagonal(index) - upper[index - 1];
            upper[index] = 1.0 / pivot;
            value[index] = (right[index] - value[index - 1]) / pivot;
        }
        for index in (0..count - 1).rev() {
            value[index] -= upper[index] * value[index + 1];
        }
        value
    };
    let mut correction = vec![0.0; count];
    correction[0] = gamma;
    correction[count - 1] = 1.0;
    let z = tridiagonal(&correction);
    for axis in 0..2 {
        let right: Vec<f64> = points.iter().map(|point| 6.0 * point[axis]).collect();
        let y = tridiagonal(&right);
        let factor = (y[0] + y[count - 1] / gamma) / (1.0 + z[0] + z[count - 1] / gamma);
        for index in 0..count {
            control[index][axis] = y[index] - factor * z[index];
        }
    }
    control
}

/// Points alternate between anchors and handles. Each span is a cubic Bezier
/// from one anchor to the next with the handle between them as both of its
/// control points. A trailing handle without an anchor is ignored.
fn bezier(points: &[Point], divisions: usize) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut result = Vec::new();
    let mut last = points[0];
    for window in points.windows(3).step_by(2) {
        let [from, handle, to] = [window[0], window[1], window[2]];
        for t in steps(divisions) {
            let s = 1.0 - t;
            result.push(combine(
                [s * s * s, 3.0 * t * s * s + 3.0 * t * t * s, t * t * t],
                [from, handle, to],
            ));
        }
        last = to;
    }
    result.push(last);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTS: [Point; 5] = [
        [0.0, 0.0],
        [10.0, 20.0],
        [30.0, 25.0],
        [50.0, 5.0],
        [70.0, 30.0],
    ];

    fn near(a: Point, b: Point) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    fn closed_points() -> Vec<Point> {
        let mut points = POINTS[..4].to_vec();
        points.push(POINTS[0]);
        points
    }

    #[test]
    fn open_cubic_starts_and_ends_on_the_polygon() {
        let curve = evaluate(CurveKind::Cubic, &POINTS, 10, false);
        assert_eq!(curve.len(), 4 * 10 + 1);
        assert!(near(curve[0], POINTS[0]) && near(curve[40], POINTS[4]));
        // Interior control points are approached, not interpolated.
        assert!(!near(curve[10], POINTS[1]));
        assert_eq!(spans(CurveKind::Cubic, 5, false, false), 4);
    }

    #[test]
    fn collinear_equidistant_cubic_stays_on_the_line() {
        let points = [[0.0, 0.0], [1.0, 2.0], [2.0, 4.0], [3.0, 6.0]];
        for (index, point) in evaluate(CurveKind::Cubic, &points, 4, false)
            .iter()
            .enumerate()
        {
            assert!(near(*point, [index as f64 / 4.0, index as f64 / 2.0]));
        }
    }

    #[test]
    fn closed_cubic_wraps_and_ignores_a_repeated_closing_point() {
        let a = evaluate(CurveKind::Cubic, &POINTS[..4], 5, true);
        let b = evaluate(CurveKind::Cubic, &closed_points(), 5, true);
        assert_eq!(a.len(), 4 * 5 + 1);
        assert_eq!(a, b);
        assert!(near(a[0], a[20]));
        assert_eq!(spans(CurveKind::Cubic, 5, true, true), 4);
        assert_eq!(spans(CurveKind::Cubic, 4, true, false), 4);
    }

    #[test]
    fn quadratic_runs_between_edge_midpoints() {
        let curve = evaluate(CurveKind::Quadratic, &POINTS, 10, false);
        assert_eq!(curve.len(), 3 * 10 + 1);
        assert!(near(curve[0], POINTS[0]) && near(curve[30], POINTS[4]));
        // First span: from the first point, around the second, to a midpoint.
        assert!(near(curve[1], [2.0, 3.825]));
        assert!(near(curve[10], mid(POINTS[1], POINTS[2])));
        assert!(near(curve[20], mid(POINTS[2], POINTS[3])));
        assert_eq!(spans(CurveKind::Quadratic, 5, false, false), 3);

        let closed = evaluate(CurveKind::Quadratic, &closed_points(), 10, true);
        assert_eq!(closed.len(), 4 * 10 + 1);
        assert!(near(closed[0], mid(POINTS[0], POINTS[1])));
        assert!(near(closed[0], closed[40]));
    }

    #[test]
    fn through_curve_passes_its_points() {
        let curve = evaluate(CurveKind::CubicThrough, &POINTS, 10, false);
        assert_eq!(curve.len(), 4 * 10 + 1);
        for (index, point) in POINTS.iter().enumerate() {
            assert!(near(curve[index * 10], *point), "{index}");
        }
        let closed = evaluate(CurveKind::CubicThrough, &closed_points(), 10, true);
        assert_eq!(closed.len(), 4 * 10 + 1);
        for (index, point) in POINTS[..4].iter().enumerate() {
            assert!(near(closed[index * 10], *point), "{index}");
        }
        assert!(near(closed[40], POINTS[0]));
        // Three points take the small closed system.
        let triangle = evaluate(CurveKind::CubicThrough, &POINTS[..3], 4, true);
        assert_eq!(triangle.len(), 3 * 4 + 1);
        assert!(near(triangle[4], POINTS[1]) && near(triangle[8], POINTS[2]));
    }

    #[test]
    fn bezier_alternates_anchors_and_handles() {
        let curve = evaluate(CurveKind::Bezier, &POINTS, 10, false);
        assert_eq!(curve.len(), 2 * 10 + 1);
        assert!(near(curve[0], POINTS[0]));
        assert!(near(curve[1], [2.73, 5.425]));
        assert!(near(curve[10], POINTS[2]) && near(curve[20], POINTS[4]));
        // A trailing handle has no anchor to run to.
        let short = evaluate(CurveKind::Bezier, &POINTS[..4], 10, false);
        assert_eq!(short.len(), 10 + 1);
        assert!(near(short[10], POINTS[2]));
        assert_eq!(spans(CurveKind::Bezier, 4, false, false), 1);
        assert_eq!(spans(CurveKind::Bezier, 7, false, false), 3);
    }

    #[test]
    fn degenerate_input_is_returned_unchanged() {
        for kind in [
            CurveKind::Quadratic,
            CurveKind::Cubic,
            CurveKind::CubicThrough,
            CurveKind::Bezier,
        ] {
            assert_eq!(evaluate(kind, &POINTS[..1], 10, false), POINTS[..1]);
            assert_eq!(evaluate(kind, &POINTS, 0, false), POINTS);
            assert_eq!(evaluate(kind, &[], 10, true), Vec::<Point>::new());
        }
        assert_eq!(
            evaluate(CurveKind::Quadratic, &POINTS[..2], 10, false),
            POINTS[..2]
        );
        assert_eq!(
            evaluate(CurveKind::Bezier, &POINTS[..2], 10, false),
            POINTS[..2]
        );
    }
}
