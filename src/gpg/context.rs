use crate::gpg::config::GpgConfig;
use crate::gpg::key::{GpgKey, KeyDetail, KeyType};
use anyhow::{anyhow, Result};
use gpgme::context::Keys;
use gpgme::{
	Context, Data, ExportMode, Key, KeyListMode, PinentryMode, Protocol,
};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

/// Context to use for rendering the output template.
#[derive(Serialize)]
struct ExportContext<'a> {
	/// Key type.
	#[serde(rename = "type")]
	pub type_: &'a str,
	/// Export pattern.
	pub query: &'a str,
	/// File extension.
	pub ext: &'a str,
}

/// A context for cryptographic operations.
#[derive(Debug)]
pub struct GpgContext {
	/// GPGME context type.
	inner: Context,
	/// GPGME configuration manager.
	pub config: GpgConfig,
}

impl GpgContext {
	/// Constructs a new instance of `GpgContext`.
	pub fn new(config: GpgConfig) -> Result<Self> {
		let mut context = Context::from_protocol(Protocol::OpenPgp)?;
		context.set_key_list_mode(
			KeyListMode::LOCAL | KeyListMode::SIGS | KeyListMode::SIG_NOTATIONS,
		)?;
		context.set_armor(config.armor);
		context.set_offline(false);
		context.set_pinentry_mode(PinentryMode::Ask)?;
		Ok(Self {
			inner: context,
			config,
		})
	}

	/// Applies the current configuration values to the context.
	pub fn apply_config(&mut self) {
		self.inner.set_armor(self.config.armor);
	}

	/// Returns the configured file path.
	///
	/// [`output_dir`] is used for output directory.
	///
	/// [`output_dir`]: GpgConfig::output_dir
	pub fn get_output_file(
		&self,
		key_type: KeyType,
		patterns: Vec<String>,
	) -> Result<PathBuf> {
		let mut template = TinyTemplate::new();
		template.add_template("export_template", &self.config.output_file)?;
		let context = ExportContext {
			type_: &key_type.to_string(),
			query: if patterns.len() == 1 {
				&patterns[0]
			} else {
				"out"
			},
			ext: if self.config.armor { "asc" } else { "pgp" },
		};
		let path = self
			.config
			.output_dir
			.join(template.render("export_template", &context)?);
		if !path.exists() {
			fs::create_dir_all(path.parent().expect("path has no parent"))?;
		}
		Ok(path)
	}

	/// Returns the public/secret key with the specified ID.
	pub fn get_key(
		&mut self,
		key_type: KeyType,
		key_id: String,
	) -> Result<Key> {
		match key_type {
			KeyType::Public => Ok(self.inner.get_key(key_id)?),
			KeyType::Secret => Ok(self.inner.get_secret_key(key_id)?),
		}
	}

	/// Returns an iterator over a list of all public/secret keys
	/// matching one or more of the specified patterns.
	fn get_keys_iter(
		&mut self,
		key_type: KeyType,
		patterns: Option<Vec<String>>,
	) -> Result<Keys> {
		Ok(match key_type {
			KeyType::Public => {
				self.inner.find_keys(patterns.unwrap_or_default())?
			}
			KeyType::Secret => {
				self.inner.find_secret_keys(patterns.unwrap_or_default())?
			}
		})
	}

	/// Returns a list of all public/secret keys matching
	/// one or more of the specified patterns.
	pub fn get_keys(
		&mut self,
		key_type: KeyType,
		patterns: Option<Vec<String>>,
		detail_level: KeyDetail,
	) -> Result<Vec<GpgKey>> {
		Ok(self
			.get_keys_iter(key_type, patterns)?
			.filter_map(|key| key.ok())
			.map(|v| GpgKey::new(v, detail_level))
			.collect())
	}

	/// Returns the all available keys and their types in a HashMap.
	pub fn get_all_keys(
		&mut self,
		detail_level: Option<KeyDetail>,
	) -> Result<HashMap<KeyType, Vec<GpgKey>>> {
		let mut keys = HashMap::new();
		keys.insert(
			KeyType::Public,
			self.get_keys(
				KeyType::Public,
				None,
				detail_level.unwrap_or_default(),
			)?,
		);
		keys.insert(
			KeyType::Secret,
			self.get_keys(
				KeyType::Secret,
				None,
				detail_level.unwrap_or_default(),
			)?,
		);
		Ok(keys)
	}

	/// Adds the given keys to the keyring.
	pub fn import_keys(
		&mut self,
		keys: Vec<String>,
		read_from_file: bool,
	) -> Result<u32> {
		let mut imported_keys = 0;
		for key in keys {
			if read_from_file {
				let input = File::open(key)?;
				let mut data = Data::from_seekable_stream(input)?;
				imported_keys += self.inner.import(&mut data)?.imported();
			} else {
				imported_keys += self.inner.import(key)?.imported();
			}
		}
		Ok(imported_keys)
	}

	/// Returns the exported public/secret keys
	/// matching one or more of the specified patterns.
	pub fn get_exported_keys(
		&mut self,
		key_type: KeyType,
		patterns: Option<Vec<String>>,
	) -> Result<Vec<u8>> {
		let mut output = Vec::new();
		let keys = self
			.get_keys_iter(key_type, patterns)?
			.filter_map(|key| key.ok())
			.collect::<Vec<Key>>();
		self.inner.export_keys(
			&keys,
			if key_type == KeyType::Secret {
				ExportMode::SECRET
			} else {
				ExportMode::empty()
			},
			&mut output,
		)?;
		if output.is_empty() {
			Err(anyhow!("nothing exported"))
		} else {
			Ok(output)
		}
	}

	/// Exports keys and saves them to the specified/default path.
	pub fn export_keys(
		&mut self,
		key_type: KeyType,
		patterns: Option<Vec<String>>,
	) -> Result<String> {
		let output = self.get_exported_keys(key_type, patterns.clone())?;
		let path =
			self.get_output_file(key_type, patterns.unwrap_or_default())?;
		File::create(&path)?.write_all(&output)?;
		Ok(path.to_string_lossy().to_string())
	}

	/// Sends the given key to the default keyserver.
	pub fn send_key(&mut self, key_id: String) -> Result<String> {
		let keys = self
			.get_keys_iter(KeyType::Public, Some(vec![key_id]))?
			.filter_map(|key| key.ok())
			.collect::<Vec<Key>>();
		if let Some(key) = &keys.first() {
			self.inner
				.export_keys_extern(vec![*key], ExportMode::EXTERN)
				.map_err(|e| anyhow!("failed to send key(s): {:?}", e))?;
			Ok(key.id().unwrap_or_default().to_string())
		} else {
			Err(anyhow!("key not found"))
		}
	}

	/// Deletes the specified public/secret key.
	///
	/// Searches the keyring for finding the specified
	/// key ID for deleting it.
	pub fn delete_key(
		&mut self,
		key_type: KeyType,
		key_id: String,
	) -> Result<()> {
		match self.get_key(key_type, key_id) {
			Ok(key) => match key_type {
				KeyType::Public => {
					self.inner.delete_key(&key)?;
					Ok(())
				}
				KeyType::Secret => {
					self.inner.delete_secret_key(&key)?;
					Ok(())
				}
			},
			Err(e) => Err(e),
		}
	}
}

#[cfg(feature = "gpg-tests")]
#[cfg(test)]
mod tests {
	use super::*;
	use crate::args::Args;
	use pretty_assertions::assert_eq;
	use std::env;
	use std::fs;
	#[test]
	fn test_gpg_context() -> Result<()> {
		env::set_var(
			"GNUPGHOME",
			dirs_next::cache_dir()
				.unwrap()
				.join(env!("CARGO_PKG_NAME"))
				.to_str()
				.unwrap(),
		);
		let args = Args::default();
		let config = GpgConfig::new(&args)?;
		let mut context = GpgContext::new(config)?;
		assert_eq!(false, context.config.armor);
		context.config.armor = true;
		context.apply_config();
		assert_eq!(true, context.config.armor);
		let keys = context.get_all_keys(None)?;
		let key_count = keys.get(&KeyType::Public).unwrap().len();
		assert!(context
			.get_key(
				KeyType::Secret,
				keys.get(&KeyType::Public).unwrap()[0].get_id()
			)
			.is_ok());
		let key_id = keys.get(&KeyType::Public).unwrap()[1].get_id();
		assert!(context.get_key(KeyType::Public, key_id.clone()).is_ok());
		context.config.output_file = String::from("{query}-{type}.{ext}");
		assert_eq!(
			context.config.output_dir.join(String::from("0x0-sec.asc")),
			context
				.get_output_file(KeyType::Secret, vec![String::from("0x0")])
				.unwrap()
		);
		let output_file = context.export_keys(KeyType::Public, None)?;
		context.delete_key(KeyType::Public, key_id)?;
		assert_eq!(
			key_count - 1,
			context
				.get_keys(KeyType::Public, None, KeyDetail::default())
				.unwrap()
				.len()
		);
		assert_eq!(
			1,
			context
				.import_keys(vec![output_file.clone()], true)
				.unwrap_or_default()
		);
		assert_eq!(
			key_count,
			context
				.get_keys(KeyType::Public, None, KeyDetail::default())
				.unwrap()
				.len()
		);
		fs::remove_file(output_file)?;
		Ok(())
	}
}
