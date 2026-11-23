//! Lower bounds on the number of blocks.

/// The Schoenheim bound: no (v, k, t) covering has fewer blocks than this.
///
/// `L(v, k, 1) = ceil(v / k)` and `L(v, k, t) = ceil(v * L(v - 1, k - 1, t - 1) / k)`.
/// Needs `1 <= t <= k <= v`. Saturates instead of overflowing.
pub fn schoenheim(v: u64, k: u64, t: u64) -> u64 {
    assert!(t >= 1 && t <= k && k <= v, "need 1 <= t <= k <= v");
    if t == 1 {
        return v.div_ceil(k);
    }
    let inner = u128::from(schoenheim(v - 1, k - 1, t - 1));
    let blocks = (u128::from(v) * inner).div_ceil(u128::from(k));
    u64::try_from(blocks).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::schoenheim;

    #[test]
    fn known_values() {
        assert_eq!(schoenheim(7, 3, 2), 7); // the Fano plane meets it
        assert_eq!(schoenheim(9, 3, 2), 12); // the affine plane of order 3
        assert_eq!(schoenheim(13, 4, 2), 13); // the projective plane of order 3
        assert_eq!(schoenheim(83, 17, 2), 30);
        assert_eq!(schoenheim(10, 5, 1), 2);
        assert_eq!(schoenheim(8, 4, 3), 14); // the Steiner system S(3, 4, 8)
    }
}
