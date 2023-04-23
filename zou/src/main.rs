use std::path::PathBuf;

use clap::{Parser, Subcommand};
use zou::registry::Registry;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, short, help = "Debug mode", default_value = "false")]
    debug: bool,

    #[clap(long, help = "SSH user", env = "ZOU_USER")]
    user: String,

    #[clap(long, help = "SSH host", env = "ZOU_HOST")]
    host: String,

    #[clap(
        long,
        help = "Path to registry's upload directory",
        env = "ZOU_UPLOAD_DIR"
    )]
    upload_dir: String,

    #[clap(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    #[clap(aliases = ["p", "pub"])]
    Publish {
        #[clap(index = 1, help = "Directory to publish")]
        dir: Option<PathBuf>,

        #[clap(long, short, help = "Name of the project")]
        name: Option<String>,
    },

    #[clap(aliases = ["d", "rm", "del"])]
    Delete {
        #[clap(long, short, help = "Name of the project")]
        name: String,
    },

    #[clap(aliases = ["l", "ls"])]
    List,
}

fn main() -> anyhow::Result<()> {
    dotenv::dotenv()?;

    let args = Args::parse();

    let mut registry = Registry::new(&args.user, &args.host, &args.upload_dir);
    registry.debug = args.debug;

    match args.cmd {
        None => registry.publish_cwd()?,
        Some(Cmd::Publish { dir, name }) => {
            registry.publish(name.as_deref(), dir)?;
        }
        Some(Cmd::Delete { name }) => {
            registry.delete(&name)?;
        }
        Some(Cmd::List) => registry.list()?,
    }

    Ok(())
}