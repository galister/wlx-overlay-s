#[derive(rust_embed::Embed)]
#[folder = "src/assets/"]
pub struct GuiAsset;

impl wgui::assets::AssetProvider for GuiAsset {
    fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
        match Self::get(path) {
            Some(data) => Ok(data.data.to_vec()),
            None => anyhow::bail!("embedded file {} not found", path),
        }
    }
}
