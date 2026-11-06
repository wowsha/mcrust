use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub release_time: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub compliance_level: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    pub main_class: String,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexInfo>,
    #[serde(default)]
    pub downloads: HashMap<String, DownloadInfo>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub release_time: Option<String>,
    #[serde(default)]
    pub compliance_level: Option<u32>,
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
}

impl VersionInfo {
    pub fn client_download(&self) -> Option<&DownloadInfo> {
        self.downloads.get("client")
    }

    pub fn required_java_major(&self) -> u32 {
        self.java_version
            .as_ref()
            .map_or(8, |java| java.major_version)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    #[serde(default)]
    pub total_size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadInfo {
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<crate::Rule>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: HashMap<String, DownloadInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<crate::Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

impl ArgumentValue {
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        match self {
            Self::One(value) => std::slice::from_ref(value).iter(),
            Self::Many(values) => values.iter(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: DownloadInfo,
    #[serde(rename = "type", default)]
    pub kind: String,
}

/// `assets/<index>.json` (the "asset index").
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: HashMap<String, AssetObject>,
    #[serde(default)]
    pub map_to_resources: bool,
    #[serde(default)]
    #[serde(rename = "virtual")]
    pub is_virtual: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_fixture() {
        let json = r#"{
            "id": "26.3",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "34",
            "assetIndex": {"id":"34","sha1":"aa","size":10,"totalSize":20,"url":"https://example.invalid/34.json"},
            "downloads": {"client":{"url":"https://example.invalid/client.jar","sha1":"bb","size":42}},
            "libraries": [{
                "name": "org.lwjgl:lwjgl:3.4.3",
                "downloads": {"artifact": {"path":"org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3.jar","url":"https://example.invalid/u","sha1":"cc","size":1}}
            }],
            "arguments": {
                "game": ["--username", {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}],
                "jvm": ["-cp", "${classpath}"]
            },
            "javaVersion": {"component":"java-runtime-epsilon","majorVersion":25}
        }"#;
        let info: VersionInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.required_java_major(), 25);
        assert_eq!(info.client_download().unwrap().size, Some(42));
        assert_eq!(info.asset_index.as_ref().unwrap().id, "34");
        assert_eq!(info.arguments.as_ref().unwrap().game.len(), 2);
    }
}
