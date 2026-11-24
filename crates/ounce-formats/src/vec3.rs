//! Sums on points and directions in the world, `[x, y, z]`.

pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

pub fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|x| x * s)
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn length(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1]).hypot(v[2])
}

/// `a` at length 1, the same way.
pub fn normalized(a: [f32; 3]) -> [f32; 3] {
    scale(a, 1.0 / dot(a, a).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    const X: [f32; 3] = [1.0, 0.0, 0.0];
    const Y: [f32; 3] = [0.0, 1.0, 0.0];
    const Z: [f32; 3] = [0.0, 0.0, 1.0];

    #[test]
    fn sums_work_per_axis() {
        assert_eq!(add([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), [5.0, 7.0, 9.0]);
        assert_eq!(sub([4.0, 5.0, 6.0], [1.0, 2.0, 3.0]), [3.0; 3]);
        assert_eq!(scale([1.0, -2.0, 3.0], 2.0), [2.0, -4.0, 6.0]);
        assert_eq!(dot([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), 32.0);
    }

    #[test]
    fn cross_follows_the_right_hand() {
        assert_eq!(cross(X, Y), Z);
        assert_eq!(cross(Y, Z), X);
        assert_eq!(cross(Z, X), Y);
        assert_eq!(cross(Y, X), [0.0, 0.0, -1.0]);
    }

    #[test]
    fn length_and_normalized_agree() {
        assert_eq!(length([3.0, 4.0, 12.0]), 13.0);
        let n = normalized([3.0, 4.0, 12.0]);
        assert!((length(n) - 1.0).abs() < 1e-6);
        assert!((n[0] - 3.0 / 13.0).abs() < 1e-6);
    }
}
