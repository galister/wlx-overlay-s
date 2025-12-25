use wlx_common::cache_dir;

use crate::util::{http_client, steam_utils::AppID, various::AsyncExecutor};

pub struct CoverArt {
	// can be empty in case if data couldn't be fetched (use a fallback image then)
	pub compressed_image_data: Vec<u8>,
}

pub async fn request_image(executor: AsyncExecutor, app_id: AppID) -> anyhow::Result<CoverArt> {
	let cache_file_path = format!("cover_arts/{}.bin", app_id);

	// check if file already exists in cache directory
	if let Some(data) = cache_dir::get_data(&cache_file_path).await {
		return Ok(CoverArt {
			compressed_image_data: data,
		});
	}

	let url = format!(
		"https://shared.steamstatic.com/store_item_assets/steam/apps/{}/library_600x900.jpg",
		app_id
	);

	match http_client::get(&executor, &url).await {
		Ok(response) => {
			log::info!("Success");
			cache_dir::set_data(&cache_file_path, &response.data).await?;
			Ok(CoverArt {
				compressed_image_data: response.data,
			})
		}
		Err(e) => {
			// fetch failed, write an empty file
			log::error!("CoverArtFetcher: failed fetch for AppID {}: {}", app_id, e);
			cache_dir::set_data(&cache_file_path, &[]).await?;
			Ok(CoverArt {
				compressed_image_data: Vec::new(),
			})
		}
	}
}
