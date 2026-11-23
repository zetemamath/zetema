//! Covering designs and their text form.
//!
//! A certificate is plain text. The first line names the format and the parameters, every later
//! line is one block:
//!
//! ```text
//! zetema/1 covering 7 3 2
//! 1 2 3
//! 1 4 5
//! ...
//! ```
//!
//! Points count from 1 in text. The canonical form lists the points of each block in increasing
//! order and the blocks in increasing order, with single spaces and a line feed after every line.
//! The digest is the SHA-256 of that canonical text, so two people who hold the same design get
//! the same digest no matter how their files were laid out.

use sha2::{Digest, Sha256};
use std::fmt;

/// The word that opens every certificate: format name and version.
pub const MAGIC: &str = "zetema/1";

/// The largest number of points a design may have (points are stored in a byte).
pub const MAX_POINTS: usize = 255;

/// A (v, k, t) covering design: blocks of `k` points out of `v`, meant to contain every set of
/// `t` points at least once.
///
/// Points are numbered from 0 here. Holding a `Covering` promises nothing about its blocks:
/// [`crate::check::check`] decides whether they are well formed and whether they really cover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Covering {
    /// Number of points.
    pub v: usize,
    /// Points per block.
    pub k: usize,
    /// Size of the sets to cover.
    pub t: usize,
    /// The blocks, each a list of points.
    pub blocks: Vec<Vec<u8>>,
}

/// Why a text is not a certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No header line.
    Empty,
    /// The first line is not `zetema/1 covering V K T`.
    Header(String),
    /// A block line holds something that is not a point number between 1 and 255.
    Point {
        /// Line number, from 1.
        line: usize,
        /// The text that is not a point.
        text: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty text, expected `{MAGIC} covering V K T`"),
            Self::Header(line) => {
                write!(f, "bad header `{line}`, expected `{MAGIC} covering V K T`")
            }
            Self::Point { line, text } => {
                write!(
                    f,
                    "line {line}: `{text}` is not a point number between 1 and {MAX_POINTS}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl Covering {
    /// Reads a certificate. Layout is free (any spacing, blank lines, CR LF); see [`Covering::canonical`].
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty());
        let (_, header) = lines.next().ok_or(ParseError::Empty)?;
        let bad = || ParseError::Header(header.trim().to_string());
        let mut words = header.split_whitespace();
        if words.next() != Some(MAGIC) || words.next() != Some("covering") {
            return Err(bad());
        }
        let mut number = || {
            words
                .next()
                .and_then(|w| w.parse::<usize>().ok())
                .ok_or_else(bad)
        };
        let (v, k, t) = (number()?, number()?, number()?);
        if words.next().is_some() {
            return Err(bad());
        }
        let blocks = lines
            .map(|(index, line)| parse_block(index + 1, line))
            .collect::<Result<_, _>>()?;
        Ok(Self { v, k, t, blocks })
    }

    /// Reads a bare list of blocks (one per line, points from 1) for the given parameters.
    pub fn from_block_list(v: usize, k: usize, t: usize, text: &str) -> Result<Self, ParseError> {
        let lines = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty());
        let blocks = lines
            .map(|(index, line)| parse_block(index + 1, line))
            .collect::<Result<_, _>>()?;
        Ok(Self { v, k, t, blocks })
    }

    /// The canonical text: points ascending inside each block, blocks ascending, one line feed per line.
    pub fn canonical(&self) -> String {
        let mut blocks = self.blocks.clone();
        for block in &mut blocks {
            block.sort_unstable();
        }
        blocks.sort_unstable();
        let mut out = format!("{MAGIC} covering {} {} {}\n", self.v, self.k, self.t);
        for block in &blocks {
            let points: Vec<String> = block
                .iter()
                .map(|&p| (usize::from(p) + 1).to_string())
                .collect();
            out.push_str(&points.join(" "));
            out.push('\n');
        }
        out
    }

    /// SHA-256 of the canonical text, in lowercase hex.
    pub fn digest(&self) -> String {
        Sha256::digest(self.canonical().as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// The usual name of the covering number these blocks bound from above, e.g. `C(83,17,2)`.
    pub fn name(&self) -> String {
        format!("C({},{},{})", self.v, self.k, self.t)
    }
}

impl fmt::Display for Covering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

fn parse_block(line: usize, text: &str) -> Result<Vec<u8>, ParseError> {
    text.split_whitespace()
        .map(|word| match word.parse::<usize>() {
            Ok(point) if (1..=MAX_POINTS).contains(&point) => Ok((point - 1) as u8),
            _ => Err(ParseError::Point {
                line,
                text: word.to_string(),
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FANO: &str = "zetema/1 covering 7 3 2\n1 2 3\n1 4 5\n1 6 7\n2 4 6\n2 5 7\n3 4 7\n3 5 6\n";

    #[test]
    fn canonical_text_round_trips() {
        let fano = Covering::parse(FANO).unwrap();
        assert_eq!((fano.v, fano.k, fano.t, fano.blocks.len()), (7, 3, 2, 7));
        assert_eq!(fano.canonical(), FANO);
        assert_eq!(fano.to_string(), FANO);
    }

    #[test]
    fn layout_does_not_change_the_digest() {
        let messy = "\r\n  zetema/1   covering 7 3 2 \r\n\r\n7 6 1\n3\t2 1\r\n5 4 1\n6 4 2\n7 5 2\n7 4 3\n6 5 3";
        let (tidy, messy) = (
            Covering::parse(FANO).unwrap(),
            Covering::parse(messy).unwrap(),
        );
        assert_eq!(messy.canonical(), FANO);
        assert_eq!(messy.digest(), tidy.digest());
        assert_eq!(tidy.digest().len(), 64);
    }

    #[test]
    fn a_changed_block_changes_the_digest() {
        let other = FANO.replace("3 5 6", "3 5 7");
        assert_ne!(
            Covering::parse(&other).unwrap().digest(),
            Covering::parse(FANO).unwrap().digest()
        );
    }

    #[test]
    fn bad_texts_are_refused() {
        assert_eq!(Covering::parse(" \n"), Err(ParseError::Empty));
        assert!(matches!(
            Covering::parse("covering 7 3 2\n1 2 3"),
            Err(ParseError::Header(_))
        ));
        assert!(matches!(
            Covering::parse("zetema/1 covering 7 3\n1 2 3"),
            Err(ParseError::Header(_))
        ));
        assert!(matches!(
            Covering::parse("zetema/1 covering 7 3 2 9\n"),
            Err(ParseError::Header(_))
        ));
        let zero = Covering::parse("zetema/1 covering 7 3 2\n0 1 2");
        assert_eq!(
            zero,
            Err(ParseError::Point {
                line: 2,
                text: "0".into()
            })
        );
        assert!(matches!(
            Covering::parse("zetema/1 covering 7 3 2\n1 x 2"),
            Err(ParseError::Point { .. })
        ));
        assert!(matches!(
            Covering::parse("zetema/1 covering 7 3 2\n1 2 256"),
            Err(ParseError::Point { .. })
        ));
    }

    #[test]
    fn bare_block_lists_are_read() {
        let bare = Covering::from_block_list(7, 3, 2, "3 2 1\n\n1 4 5\n").unwrap();
        assert_eq!(bare.blocks, vec![vec![2, 1, 0], vec![0, 3, 4]]);
        assert_eq!(bare.name(), "C(7,3,2)");
    }
}
