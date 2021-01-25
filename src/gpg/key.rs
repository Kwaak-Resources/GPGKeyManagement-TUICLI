use crate::gpg::handler;
use gpgme::{Key, Subkey, UserId, UserIdSignature};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Type of the key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyType {
	/// Public key.
	Public,
	/// Secret (private) key.
	Secret,
}

impl Display for KeyType {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		write!(
			f,
			"{}",
			match self {
				Self::Public => "pub",
				Self::Secret => "sec",
			}
		)
	}
}

/// Level of detail to show for key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyDetailLevel {
	/// Show only the primary key and user ID.
	Minimum = 0,
	/// Show all subkeys and user IDs.
	Standard = 1,
	/// Show signatures.
	Full = 2,
}

impl KeyDetailLevel {
	/// Increases the level of detail.
	pub fn increase(&mut self) {
		*self = match *self as i8 + 1 {
			1 => KeyDetailLevel::Standard,
			2 => KeyDetailLevel::Full,
			_ => KeyDetailLevel::Minimum,
		}
	}
}

/// Representation of a key.
#[derive(Clone, Debug)]
pub struct GpgKey {
	/// GPGME Key type.
	inner: Key,
	/// Level of detail to show about key information.
	pub detail: KeyDetailLevel,
}

impl From<Key> for GpgKey {
	fn from(key: Key) -> Self {
		Self {
			inner: key,
			detail: KeyDetailLevel::Minimum,
		}
	}
}

impl GpgKey {
	/// Returns the key ID with '0x' prefix.
	pub fn get_id(&self) -> String {
		self.inner
			.id()
			.map_or(String::from("[?]"), |v| format!("0x{}", v))
	}

	/// Returns information about the subkeys.
	pub fn get_subkey_info(&self, truncate: bool) -> Vec<String> {
		let mut key_info = Vec::new();
		let subkeys = self.inner.subkeys().collect::<Vec<Subkey>>();
		for (i, subkey) in subkeys.iter().enumerate() {
			key_info.push(format!(
				"[{}] {}/{}",
				handler::get_subkey_flags(*subkey),
				subkey
					.algorithm_name()
					.unwrap_or_else(|_| { String::from("[?]") }),
				if truncate {
					subkey.id()
				} else {
					subkey.fingerprint()
				}
				.unwrap_or("[?]"),
			));
			if self.detail == KeyDetailLevel::Minimum {
				break;
			}
			key_info.push(format!(
				"{}      └─{}",
				if i != subkeys.len() - 1 { "|" } else { " " },
				handler::get_subkey_time(
					*subkey,
					if truncate { "%Y" } else { "%F" }
				)
			));
		}
		key_info
	}

	/// Returns information about the users of the key.
	pub fn get_user_info(&self, truncate: bool) -> Vec<String> {
		let mut user_info = Vec::new();
		let user_ids = self.inner.user_ids().collect::<Vec<UserId>>();
		for (i, user) in user_ids.iter().enumerate() {
			user_info.push(format!(
				"{}[{}] {}",
				if i == 0 {
					""
				} else if i == user_ids.len() - 1 {
					" └─"
				} else {
					" ├─"
				},
				user.validity(),
				if truncate { user.email() } else { user.id() }
					.unwrap_or("[?]")
			));
			if self.detail == KeyDetailLevel::Minimum {
				break;
			}
			if self.detail == KeyDetailLevel::Full {
				user_info.extend(self.get_user_signatures(
					user,
					user_ids.len(),
					i,
					truncate,
				));
			}
		}
		user_info
	}

	/// Returns the signature information of an user.
	fn get_user_signatures(
		&self,
		user: &UserId,
		user_count: usize,
		user_index: usize,
		truncate: bool,
	) -> Vec<String> {
		let signatures = user.signatures().collect::<Vec<UserIdSignature>>();
		signatures
			.iter()
			.enumerate()
			.map(|(i, sig)| {
				format!(
					" {}  {}[{:x}] {} {}",
					if user_count == 1 {
						" "
					} else if user_index == user_count - 1 {
						"    "
					} else if user_index == 0 {
						"│"
					} else {
						"│   "
					},
					if i == signatures.len() - 1 {
						"└─"
					} else {
						"├─"
					},
					sig.cert_class(),
					if sig.signer_key_id() == self.inner.id() {
						String::from("selfsig")
					} else if truncate {
						sig.signer_key_id().unwrap_or("[?]").to_string()
					} else {
						format!(
							"{} {}",
							sig.signer_key_id().unwrap_or("[?]"),
							sig.signer_user_id().unwrap_or("[?]")
						)
					},
					handler::get_signature_time(
						*sig,
						if truncate { "%Y" } else { "%F" }
					)
				)
			})
			.collect()
	}
}
