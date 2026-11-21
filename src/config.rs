use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Session>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub cookies: Vec<Cookie>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
}

impl Session {
    pub fn cookie_header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        Some(
            self.cookies
                .iter()
                .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

#[derive(Debug)]
pub struct ConfigStore {
    root: PathBuf,
}

impl ConfigStore {
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os("APOLLO_CONFIG_DIR") {
            if root.is_empty() {
                bail!("APOLLO_CONFIG_DIR cannot be empty");
            }
            return Ok(Self::at(root));
        }
        let directories = ProjectDirs::from("", "", "apollo")
            .context("could not determine the user configuration directory")?;
        Ok(Self::at(directories.config_dir()))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> PathBuf {
        self.root.join("session.json")
    }

    pub fn load(&self) -> Result<Option<Config>> {
        let bytes = match fs::read(self.path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not read config at {}", self.path().display())
                });
            }
        };
        serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid config at {}", self.path().display()))
            .map(Some)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        fs::create_dir_all(&self.root).with_context(|| {
            format!("could not create config directory {}", self.root.display())
        })?;
        restrict_directory(&self.root)?;

        let mut temporary = NamedTempFile::new_in(&self.root)
            .with_context(|| format!("could not create config file in {}", self.root.display()))?;
        restrict_file(temporary.as_file())?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), config)?;
        temporary.as_file_mut().write_all(b"\n")?;
        temporary.as_file_mut().flush()?;
        temporary
            .as_file()
            .sync_all()
            .context("could not sync config file")?;
        temporary
            .persist(self.path())
            .map_err(|error| error.error)
            .with_context(|| format!("could not save config at {}", self.path().display()))?;
        restrict_path(&self.path())
    }
}

#[cfg(unix)]
fn restrict_file(file: &fs::File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file(_file: &fs::File) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_path(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_path(_path: &Path) -> Result<()> {
    Ok(())
}
