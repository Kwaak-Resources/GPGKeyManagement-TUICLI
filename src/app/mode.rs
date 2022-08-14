use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

/// Application mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
	/// Normal mode.
	Normal,
	/// Visual mode.
	/// (Disables the mouse capture)
	Visual,
	/// Copy mode.
	/// (Makes it easier to copy values)
	Copy,
}

impl Display for Mode {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		write!(f, "-- {} --", format!("{:?}", self).to_lowercase())
	}
}

impl FromStr for Mode {
	type Err = ();
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().as_str() {
			"normal" | "n" => Ok(Self::Normal),
			"visual" | "v" => Ok(Self::Visual),
			"copy" | "c" => Ok(Self::Copy),
			_ => Err(()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;
	#[test]
	fn test_app_mode() {
		let mode = Mode::from_str("normal").unwrap();
		assert_eq!(Mode::Normal, mode);
		assert_eq!(String::from("-- normal --"), mode.to_string());
		let mode = Mode::from_str("visual").unwrap();
		assert_eq!(Mode::Visual, mode);
		assert_eq!(String::from("-- visual --"), mode.to_string());
		let mode = Mode::from_str("copy").unwrap();
		assert_eq!(Mode::Copy, mode);
		assert_eq!(String::from("-- copy --"), mode.to_string());
	}
}
