#[derive(rust_embed::Embed)]
#[folder = "assets/"]
pub struct AssetInternal;

impl crate::assets::AssetProvider for AssetInternal {
	fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
		match AssetInternal::get(path) {
			Some(data) => Ok(data.data.to_vec()),
			None => anyhow::bail!("internal file {path} not found"),
		}
	}
}
