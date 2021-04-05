#![cfg_attr(not(feature = "std"), no_std)]

//! A crate for the cryptographic sponge trait.
#![deny(
    const_err,
    future_incompatible,
    missing_docs,
    non_shorthand_field_patterns,
    renamed_and_removed_lints,
    rust_2018_idioms,
    stable_features,
    trivial_casts,
    trivial_numeric_casts,
    unused,
    variant_size_differences,
    warnings
)]
#![forbid(unsafe_code)]

use ark_ff::{FpParameters, PrimeField};
use ark_std::vec::Vec;
use ark_std::vec;

#[macro_use]
extern crate derivative;

/// Infrastructure for the constraints counterparts.
#[cfg(feature = "r1cs")]
pub mod constraints;

mod absorbable;
pub use absorbable::*;

/// The sponge for Poseidon
///
/// This implementation of Poseidon is entirely from Fractal's implementation in [COS20][cos]
/// with small syntax changes.
///
/// [cos]: https://eprint.iacr.org/2019/1076
pub mod poseidon;

/// A sponge that offers backwards compatibility for implementations that do not accept sponge
/// objects but require domain separation. Operates in the same way as fork.
pub mod domain_separated;

/// An enum for specifying the output field element size.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FieldElementSize {
    /// Sample field elements from the entire field.
    Full,

    /// Sample field elements from a subset of the field, specified by the maximum number of bits.
    Truncated(usize),
}

impl FieldElementSize {
    pub(crate) fn num_bits<F: PrimeField>(&self) -> usize {
        if let FieldElementSize::Truncated(num_bits) = self {
            *num_bits.min(&(F::Params::CAPACITY as usize))
        } else {
            F::Params::CAPACITY as usize
        }
    }
}

/// The interface for a cryptographic sponge.
/// A sponge can `absorb` or take in inputs and later `squeeze` or output bytes or field elements.
/// The outputs are dependent on previous `absorb` and `squeeze` calls.
pub trait CryptographicSponge<CF: PrimeField>: Clone {
    /// Initialize a new instance of the sponge.
    fn new() -> Self;

    /// Absorb an input into the sponge.
    fn absorb(&mut self, input: &impl Absorbable<CF>);

    /// Squeeze `num_bytes` bytes from the sponge.
    fn squeeze_bytes(&mut self, num_bytes: usize) -> Vec<u8>;

    /// Squeeze `num_bits` bits from the sponge.
    fn squeeze_bits(&mut self, num_bits: usize) -> Vec<bool>;

    /// Squeeze `num_elements` field elements from the sponge.
    fn squeeze_field_elements(&mut self, num_elements: usize) -> Vec<CF>;

    /// Squeeze `sizes.len()` field elements from the sponge, where the `i`-th element of
    /// the output has size `sizes[i]`.
    fn squeeze_field_elements_with_sizes(&mut self, sizes: &[FieldElementSize]) -> Vec<CF> {
        let mut all_full_sizes = true;
        for size in sizes {
            if *size != FieldElementSize::Full {
                all_full_sizes = false;
                break;
            }
        }

        if all_full_sizes {
            self.squeeze_field_elements(sizes.len())
        } else {
            self.squeeze_nonnative_field_elements_with_sizes::<CF>(sizes)
        }
    }

    /// Squeeze `sizes.len()` nonnative field elements from the sponge, where the `i`-th element of
    /// the output has size `sizes[i]`.
    fn squeeze_nonnative_field_elements_with_sizes<F: PrimeField>(
        &mut self,
        sizes: &[FieldElementSize],
    ) -> Vec<F> {
        if sizes.len() == 0 {
            return Vec::new();
        }

        let mut total_bits = 0usize;
        for size in sizes {
            total_bits += size.num_bits::<F>();
        }

        let bits = self.squeeze_bits(total_bits);
        let mut bits_window = bits.as_slice();

        let mut output = Vec::with_capacity(sizes.len());
        for size in sizes {
            let num_bits = size.num_bits::<F>();
            let nonnative_bits_le: Vec<bool> = bits_window[..num_bits].to_vec();
            bits_window = &bits_window[num_bits..];

            let nonnative_bytes = nonnative_bits_le
                .chunks(8)
                .map(|bits| {
                    let mut byte = 0u8;
                    for (i, &bit) in bits.into_iter().enumerate() {
                        if bit {
                            byte += 1 << i;
                        }
                    }
                    byte
                })
                .collect::<Vec<_>>();

            output.push(F::from_random_bytes(nonnative_bytes.as_slice()).unwrap());
        }

        output
    }

    /// Squeeze `num_elements` nonnative field elements from the sponge.
    fn squeeze_nonnative_field_elements<F: PrimeField>(&mut self, num_elements: usize) -> Vec<F> {
        self.squeeze_nonnative_field_elements_with_sizes::<F>(
            vec![FieldElementSize::Full; num_elements].as_slice(),
        )
    }

    /// Creates a new sponge with applied domain separation.
    fn fork(&self, domain: &[u8]) -> Self {
        let mut new_sponge = self.clone();

        let mut input = Absorbable::<CF>::to_sponge_bytes(&domain.len());
        input.extend_from_slice(domain);
        new_sponge.absorb(&input);

        new_sponge
    }
}

/// An extension for the inferface of a cryptographic sponge.
/// In addition to operations defined in `CryptographicSponge`, `SpongeExt` can convert itself to
/// a state, and instantiate itself from state.
pub trait SpongeExt: CryptographicSponge {
    type State: Clone;
    /// Returns a sponge that uses `state`.
    fn from_state(state: Self::State) -> Self;
    /// Consumes `self` and returns the state.
    fn into_state(self) -> Self::State;
}
