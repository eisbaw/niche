use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use post2html::{computed, config, render};

#[derive(Parser)]
#[command(name = "post2html", about = "Static site generator pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a single post from content + config into standalone HTML
    Render {
        /// Path to post config JSON
        #[arg(long)]
        config: PathBuf,

        /// Path to content file (md, rst, html, txt)
        #[arg(long)]
        content: PathBuf,

        /// Output directory
        #[arg(long)]
        out: PathBuf,
    },

    /// Resolve inter-post links using a links registry
    Link {
        /// Path to links registry JSON
        #[arg(long)]
        links: PathBuf,

        /// Directory of compiled posts
        #[arg(long)]
        posts_dir: PathBuf,

        /// Output directory
        #[arg(long)]
        out: PathBuf,
    },

    /// Compose final site from linked posts, templates, and static assets
    Compose {
        /// Path to site config JSON
        #[arg(long)]
        config: PathBuf,

        /// Directory of linked posts
        #[arg(long)]
        posts_dir: PathBuf,

        /// Directory of Tera templates
        #[arg(long)]
        template_dir: PathBuf,

        /// Directory of static assets
        #[arg(long)]
        static_dir: PathBuf,

        /// Output directory
        #[arg(long)]
        out: PathBuf,
    },

    /// Remove build output directory
    Clean {
        /// Directory to remove
        #[arg(long, default_value = "output")]
        out: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Render {
            config,
            content,
            out,
        } => {
            let post_config = match config::PostConfig::from_file(config) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            };

            let html = match render::render_file(content) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            };

            match render::write_html(&html, out) {
                Ok(path) => {
                    println!("{}", path.display());
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }

            let computed_json = computed::build_computed_json(&post_config, &html);
            match computed::write_computed_json(&computed_json, out) {
                Ok(path) => {
                    println!("{}", path.display());
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        Command::Link {
            links,
            posts_dir,
            out,
        } => {
            println!(
                "link: links={} posts_dir={} out={}",
                links.display(),
                posts_dir.display(),
                out.display()
            );
        }
        Command::Compose {
            config,
            posts_dir,
            template_dir,
            static_dir,
            out,
        } => {
            println!(
                "compose: config={} posts_dir={} template_dir={} static_dir={} out={}",
                config.display(),
                posts_dir.display(),
                template_dir.display(),
                static_dir.display(),
                out.display()
            );
        }
        Command::Clean { out } => {
            println!("clean: out={}", out.display());
        }
    }
}
