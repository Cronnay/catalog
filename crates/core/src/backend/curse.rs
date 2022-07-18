use isahc::config::RedirectPolicy;
use isahc::{prelude::*, HttpClient};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::backend::{Addon, Flavor, Source, Version};
use crate::error::Error;

fn get_flavor_from_game_version_type_id(game_id: i32) -> Flavor {
    match game_id {
        73246 => Flavor::ClassicTbc,
        67408 => Flavor::ClassicEra,
        517 => Flavor::Retail,
        73713 => Flavor::ClassicWotlk,
        _ => panic!("Unsupported game id {}", game_id),
    }
}

impl From<Package> for Addon {
    fn from(package: Package) -> Self {
        let package_cloned = package.clone();

        let files = package
            .latest_files_indexes
            .into_iter()
            .filter(|f| {
                (f.release_type == 1 || f.release_type == 2)
                    && f.game_version_type_id.unwrap_or(0) > 0
            })
            .collect::<Vec<LatestFilesIndexes>>();
        let files_cloned = files.clone();

        let versions = files
            .into_iter()
            .filter(|f| {
                // We only want the newest for each flavor.
                !files_cloned.iter().any(|b| {
                    get_flavor_from_game_version_type_id(b.game_version_type_id.unwrap_or(0))
                        == get_flavor_from_game_version_type_id(f.game_version_type_id.unwrap_or(0))
                        && b.file_id > f.file_id
                })
            })
            .map(|file| {
                let file_date: String = {
                    let found = package_cloned
                        .latest_files
                        .iter()
                        .find(|&p| p.id == file.file_id as i64);
                    if let Some(fd) = found {
                        fd.file_date.to_owned()
                    } else {
                        "1971-01-01T01:01:01.01Z".to_string()
                    }
                };
                Version {
                    game_version: Some(file.game_version.to_owned()),
                    flavor: get_flavor_from_game_version_type_id(
                        file.game_version_type_id.unwrap_or(0),
                    ),
                    date: file_date,
                }
            })
            .collect();
        Addon {
            id: package.id,
            name: package.name,
            url: package.links.website_url.unwrap_or(format!(
                "https://www.curseforge.com/wow/addons/{}",
                package.slug
            )),
            number_of_downloads: package.download_count.round() as u64,
            summary: package.summary,
            versions,
            categories: package.categories.into_iter().map(|c| c.name).collect(),
            source: Source::Curse,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Category {
    name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct File {
    pub id: i64,
    pub display_name: String,
    pub file_name: String,
    pub file_date: String,
    pub download_url: Option<String>,
    pub release_type: u32,
    pub modules: Vec<Module>,
    #[serde(alias = "isAvailable", alias = "isAlternate")]
    pub is_available: bool,
    #[serde(alias = "gameVersion", alias = "gameVersions")]
    pub game_versions: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    #[serde(alias = "name", alias = "foldername")]
    pub foldername: String,
    pub fingerprint: i64,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Packages {
    data: Vec<Package>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Package {
    id: i32,
    game_id: i32,
    name: String,
    slug: String,
    summary: String,
    download_count: f64,
    links: Links,
    latest_files: Vec<File>,
    latest_files_indexes: Vec<LatestFilesIndexes>,
    categories: Vec<Category>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct LatestFilesIndexes {
    game_version: String,
    file_id: i32,
    filename: String,
    release_type: i32,
    game_version_type_id: Option<i32>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Links {
    website_url: Option<String>,
}

const API_KEY: Option<&'static str> = option_env!("CURSE_API_KEY");

fn base_endpoint(page_size: usize, index: usize) -> String {
    format!(
        "https://api.curseforge.com/v1/mods/search?gameId=1&pageSize={}&index={}",
        page_size, index
    )
}
static HTTP_CLIENT: Lazy<HttpClient> = Lazy::new(|| {
    HttpClient::builder()
        .redirect_policy(RedirectPolicy::Follow)
        .max_connections_per_host(6)
        .build()
        .unwrap()
});

pub async fn get_addons() -> Result<Vec<Addon>, Error> {
    if let Some(api_key) = API_KEY {
        let mut index: usize = 0;
        let page_size: usize = 50;
        let mut number_of_addons = page_size;
        let mut addons: Vec<Addon> = vec![];
        while page_size == number_of_addons {
            let endpoint = base_endpoint(page_size, index);
            let mut request = isahc::Request::builder().uri(endpoint);
            request = request.header("x-api-key", api_key);
            let mut response = HTTP_CLIENT.send_async(request.body(())?).await?;
            if response.status().is_success() {
                let packages = response.json::<Packages>().await?;
                let partials_addons = packages
                    .data
                    .into_iter()
                    .map(Addon::from)
                    .collect::<Vec<Addon>>();

                addons.extend_from_slice(&partials_addons);
                number_of_addons = partials_addons.len();
                index += page_size;
            } else {
                panic!("{}", response.status())
            }
        }

        Ok(addons)
    } else {
        panic!("API Key not provided");
    }
}
