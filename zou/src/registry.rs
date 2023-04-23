use std::{path::PathBuf, process::Command};

use anyhow::bail;

pub struct Registry {
    user: String,
    host: String,
    root_dir: PathBuf,
    pub debug: bool,
}

impl Registry {
    pub fn new(user: &str, host: &str, root_dir: impl Into<PathBuf>) -> Self {
        Self {
            user: user.to_string(),
            host: host.to_string(),
            root_dir: root_dir.into(),
            debug: false,
        }
    }

    /// Publishes current working dir with an auto-generated name
    pub fn publish_cwd(&self) -> anyhow::Result<()> {
        self.publish(None, None)
    }

    pub fn publish(&self, name: Option<&str>, source: Option<PathBuf>) -> anyhow::Result<()> {
        let source = source.unwrap_or(std::env::current_dir()?);
        if !source.exists() || !source.is_dir() {
            bail!("file does not exist or is not a directory");
        }

        let source = format!("{}/", source.to_string_lossy());
        let user = &self.user;
        let host = &self.host;
        let mut path = self.root_dir.clone();
        let name = name.map_or(gen_name(), Into::into);
        path.push(&name);
        let path = path.to_string_lossy();
        let target = format!("{user}@{host}:{path}");

        let mut rsync = Command::new("rsync");
        rsync.arg("-zr").arg(source).arg(target);
        if self.debug {
            rsync.arg("--progress");
        }
        let status = rsync.status()?;

        if !status.success() {
            bail!("unable to sync");
        }

        println!("✔ http://{name}.{host}", host = self.host);
        Ok(())
    }

    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        let Self { user, host, .. } = self;

        let mut path = self.root_dir.clone();
        path.push(name);

        let status = Command::new("ssh")
            .arg(format!("{user}@{host}"))
            .arg(format!(
                "rm -rf {}; certbot -n delete --cert-name {name}.{host}",
                path.to_string_lossy()
            ))
            .status()?;

        if !status.success() {
            bail!("unable to delete");
        }

        println!("✘ deleted \"{name}\"");
        Ok(())
    }

    pub fn list(&self) -> anyhow::Result<()> {
        let Self { user, host, .. } = self;
        let path = self.root_dir.to_string_lossy();

        let status = Command::new("ssh")
            .arg(format!("{user}@{host}"))
            .arg(format!("ls {path}"))
            .status()?;

        if !status.success() {
            bail!("unable to delete");
        }

        Ok(())
    }
}

fn gen_name() -> String {
    names::Generator::with_naming(names::Name::Numbered)
        .next()
        .unwrap()
}
