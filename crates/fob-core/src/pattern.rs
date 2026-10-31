//! An address's pattern: dots on a 5 × 5 grid in one of eight colours,
//! drawn on Receive and on the payer's confirm screen so two people can
//! see at a glance they have the same address. Derived from a hash of the
//! whole address, so changing any character changes it.
//!
//! The derivation (docs/decisions.md has it too):
//!
//! 1. `h` = Keccak-256 (Monero's hash, not SHA3-256) of the address's
//!    UTF-8 bytes, exactly as given.
//! 2. `cells` = `(h[0] | h[1] << 8) & 0x7FFF`: bit `3 × row + column` is
//!    the cell at `row` 0–4 (top down) and `column` 0–2 (0 the outer left).
//! 3. `colour` = `h[2] & 7`.
//! 4. If fewer than 3 or more than 12 of the 15 bits are on, `h` =
//!    Keccak-256 of the 32 bytes of `h` and back to 2.
//!
//! Drawn, column `c` of 0–4 shows source column `min(c, 4 − c)`, so the
//! right two columns mirror the left two.

use sha3::{Digest, Keccak256};

/// The fewest and most of the 15 cells on.
const FEWEST: u32 = 3;
const MOST: u32 = 12;

/// An address's pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// 15 bits, 5 rows of 3 columns; bit `3 × row + column`.
    pub cells: u16,
    /// 0–7: an index into the apps' fixed palette.
    pub colour: u8,
}

impl Pattern {
    /// The 5 × 5 grid as drawn, row by row, the right two columns mirroring
    /// the left two: `true` for a dot.
    pub fn dots(&self) -> Vec<bool> {
        (0..5).flat_map(|row| (0..5).map(move |column: usize| self.cells >> (3 * row + column.min(4 - column)) & 1 == 1)).collect()
    }
}

/// The pattern for `address`.
pub fn address_pattern(address: &str) -> Pattern {
    let mut hash: [u8; 32] = Keccak256::digest(address.as_bytes()).into();
    loop {
        let cells = u16::from_le_bytes([hash[0], hash[1]]) & 0x7FFF;
        if (FEWEST..=MOST).contains(&cells.count_ones()) {
            return Pattern { cells, colour: hash[2] & 7 };
        }
        hash = Keccak256::digest(hash).into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "5AQmdbMiSRLJQqGGeVA2aMPoyRqR64sTb9BHKWdi8nBpTJTrRBsgffvPBsdm5JeSG6MRvG5SBPuNZtJjqDkJXapj5S6QWVc";

    #[test]
    fn the_hash_is_keccak_not_sha3() {
        let empty: [u8; 32] = Keccak256::digest(b"").into();
        assert_eq!(hex::encode(empty), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    }

    #[test]
    fn fixed_vectors() {
        let first = address_pattern(ADDRESS);
        assert_eq!(first, Pattern { cells: VECTOR.0, colour: VECTOR.1 });
        assert_eq!(address_pattern(""), Pattern { cells: EMPTY.0, colour: EMPTY.1 });
        // One character's difference is a different pattern.
        assert_ne!(address_pattern(&ADDRESS.replace("Vc", "Vd")), first);
    }

    /// An input whose first hash has too few or too many cells on, so the
    /// pattern comes from hashing the hash.
    #[test]
    fn out_of_range_counts_hash_again() {
        let h: [u8; 32] = Keccak256::digest(REHASHED.0.as_bytes()).into();
        let count = (u16::from_le_bytes([h[0], h[1]]) & 0x7FFF).count_ones();
        assert!(!(3..=12).contains(&count), "{count}");
        let again: [u8; 32] = Keccak256::digest(h).into();
        let expected = Pattern { cells: u16::from_le_bytes([again[0], again[1]]) & 0x7FFF, colour: again[2] & 7 };
        assert_eq!(address_pattern(REHASHED.0), expected);
        assert_eq!(expected, Pattern { cells: REHASHED.1, colour: REHASHED.2 });
    }

    #[test]
    fn always_three_to_twelve_cells() {
        for i in 0..20_000 {
            let n = address_pattern(&format!("{ADDRESS}{i}")).cells.count_ones();
            assert!((3..=12).contains(&n));
        }
    }

    #[test]
    fn drawn_mirrored() {
        // Cells 0 (row 0, outer left), 2 (row 0, centre) and 13 (row 4, middle left).
        let p = Pattern { cells: 1 | 1 << 2 | 1 << 13, colour: 0 };
        let rows: Vec<String> = p.dots().chunks(5).map(|r| r.iter().map(|&d| if d { '●' } else { '·' }).collect()).collect();
        assert_eq!(rows, ["●·●·●", "·····", "·····", "·····", "·●·●·"]);
    }

    const VECTOR: (u16, u8) = (27_718, 2);
    /// From Keccak-256("") = c5d246…: 0xd2c5 & 0x7fff, 0x46 & 7.
    const EMPTY: (u16, u8) = (0x52c5, 6);
    const REHASHED: (&str, u16, u8) = ("fob-17", 28_262, 2);
}
