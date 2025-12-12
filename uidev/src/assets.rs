#[derive(rust_embed::Embed)]
#[folder = "assets/"]
pub struct Asset;

impl wgui::assets::AssetProvider for Asset {
	fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
		match Asset::get(path) {
			Some(data) => Ok(data.data.to_vec()),
			None => anyhow::bail!("embedded file {} not found", path),
		}
	}
}
