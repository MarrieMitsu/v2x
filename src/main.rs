use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::builder::TypedValueParser;
use clap::{Parser, ValueEnum, ValueHint};
use image::ExtendedColorType;
use rayon::prelude::*;
use tiny_skia::Pixmap;

/// Format
#[derive(Clone, Debug, PartialEq, Eq, Hash, ValueEnum)]
enum Format {
    Avif,
    Jpeg,
    Png,
    Tiff,
    Webp,
}

impl Format {
    fn extension(&self) -> String {
        match self {
            Self::Avif => String::from("avif"),
            Self::Jpeg => String::from("jpeg"),
            Self::Png => String::from("png"),
            Self::Tiff => String::from("tiff"),
            Self::Webp => String::from("webp"),
        }
    }

    fn has_alpha_channel(&self) -> bool {
        match self {
            Format::Avif | Format::Png | Format::Tiff | Format::Webp => true,
            Format::Jpeg => false,
        }
    }
}

/// An input that is either stdin or a real path.
#[derive(Debug, Clone)]
enum Input {
    /// Stdin, represented by `-`.
    Stdin,
    /// A non-empty path.
    Path(PathBuf),
}

fn input_value_parser() -> impl TypedValueParser<Value = Input> {
    clap::builder::OsStringValueParser::new().try_map(|v| {
        if v.is_empty() {
            Err(clap::Error::new(clap::error::ErrorKind::InvalidValue))
        } else if v == "-" {
            Ok(Input::Stdin)
        } else {
            Ok(Input::Path(v.into()))
        }
    })
}

fn read_from_stdin() -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf)?;

    Ok(buf)
}

/// Config
#[derive(Clone, Debug, Parser)]
#[command(version, about, long_about = None)]
struct Config {
    /// Path to input SVG file. Use `-` to read input from stdin.
    #[clap(value_parser = input_value_parser(), value_hint = ValueHint::FilePath)]
    input: Input,

    /// Output directory. If not specified it will use current working directory.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Custom output filename without an extension. When input is from 'stdin', this option is
    /// required.
    #[arg(long)]
    filename: Option<String>,

    /// Explicitly specify a list of formats, separated by <commas>. By default, all available
    /// formats are generated.
    #[arg(short, long, value_delimiter = ',', value_enum)]
    format: Option<Vec<Format>>,

    /// Output width in pixels (overrides '--scale').
    #[arg(long)]
    width: Option<u32>,

    /// Output height in pixels (overrides '--scale').
    #[arg(long)]
    height: Option<u32>,

    /// Scale factor relative to the SVG's intrinsic size.
    #[arg(long, default_value_t = 1.0)]
    scale: f32,

    /// Background color in hex ('#RRGGBB' or '#RRGGBBAA'). By default, for formats that support alpha channel it will be
    /// transparent, otherwise it will be filled with solid white.
    #[arg(long)]
    background: Option<String>,
}

/// Simple file validation if input file exists and has a valid '.svg' extension file.
///
/// Actually, it doesn't really matter. I just put an extra layer here, above the SVG parser.
fn is_svg_file(path: &PathBuf) -> bool {
    let is_valid_ext = match path.extension().and_then(|s| s.to_str()) {
        Some("svg") => true,
        _ => false,
    };

    path.is_file() && is_valid_ext
}

fn parse_color(s: &str) -> Result<tiny_skia::Color> {
    let s = s.trim_start_matches('#');

    let (r, g, b, a) = match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16)?;
            let g = u8::from_str_radix(&s[2..4], 16)?;
            let b = u8::from_str_radix(&s[4..6], 16)?;

            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16)?;
            let g = u8::from_str_radix(&s[2..4], 16)?;
            let b = u8::from_str_radix(&s[4..6], 16)?;
            let a = u8::from_str_radix(&s[6..8], 16)?;

            (r, g, b, a)
        }
        _ => bail!("Invalid color format (expected '#RRGGBB' or '#RRGGBBAA')"),
    };

    Ok(tiny_skia::Color::from_rgba8(r, g, b, a))
}

fn pixmap_to_rgb_buffer(pixmap: &Pixmap) -> Vec<u8> {
    let width = pixmap.width();
    let height = pixmap.height();
    let buf = pixmap.data();

    // pre-allocates buffer size.
    let mut rgb: Vec<u8> = Vec::with_capacity((width * height * 3) as usize);

    for px in buf.chunks_exact(4) {
        let (r, g, b, a) = (px[0] as f32, px[1] as f32, px[2] as f32, px[3] as f32);

        // tiny-skia pixmaps are premultiplied, so we need to unpremultiply it.
        let alpha = a / 255.0;
        let (r, g, b) = if alpha > 0.0 {
            (
                (r / alpha).min(255.0),
                (g / alpha).min(255.0),
                (b / alpha).min(255.0),
            )
        } else {
            (0.0, 0.0, 0.0)
        };

        rgb.push(r as u8);
        rgb.push(g as u8);
        rgb.push(b as u8);
    }

    rgb
}

fn main() -> Result<()> {
    let env = env_logger::Env::default()
        .filter_or("V2X_LOG_LEVEL", "info")
        .write_style_or("V2X_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    let config = Config::parse();
    let formats = config.format.map_or_else(
        || {
            vec![
                Format::Avif,
                Format::Jpeg,
                Format::Png,
                Format::Tiff,
                Format::Webp,
            ]
        },
        |mut v| {
            let mut seen = HashSet::new();
            v.retain(|e| seen.insert(e.clone()));
            v
        },
    );

    let output = config.output.map_or_else(
        || std::env::current_dir(),
        |v| {
            if !v.exists() {
                std::fs::create_dir_all(&v)?;
            }

            Ok(v)
        },
    )?;

    let background = if let Some(v) = &config.background {
        Some(parse_color(v)?)
    } else {
        None
    };

    let filename = match &config.input {
        Input::Stdin => match config.filename {
            Some(f) => OsString::from(f),
            _ => bail!("'--filename' is required because the input comes from stdin."),
        },
        Input::Path(p) => {
            if !is_svg_file(&p) {
                bail!(
                    "Invalid SVG file: '{}'. Please provide a valid SVG input.",
                    p.display()
                );
            }

            match config.filename {
                Some(f) => OsString::from(f),
                _ => p
                    .file_stem()
                    .expect("filename should not be empty.")
                    .to_owned(),
            }
        }
    };

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();

    let data = match &config.input {
        Input::Stdin => read_from_stdin().context("Failed to read from stdin.")?,
        Input::Path(p) => {
            std::fs::read(&p).with_context(|| format!("Failed to read file '{}'.", p.display()))?
        }
    };
    let tree = usvg::Tree::from_data(&data, &opt)?;

    let size = tree.size().to_int_size();
    let base_width = size.width();
    let base_height = size.height();

    let (width, height) = if config.width.is_some() || config.height.is_some() {
        let w = config.width.unwrap_or_else(|| {
            config.height.map_or_else(
                || base_width,
                |v| {
                    let ratio = v as f32 / base_height as f32;
                    (base_width as f32 * ratio) as u32
                },
            )
        });

        let h = config.height.unwrap_or_else(|| {
            config.width.map_or_else(
                || base_height,
                |v| {
                    let ratio = v as f32 / base_width as f32;
                    (base_height as f32 * ratio) as u32
                },
            )
        });

        (w, h)
    } else {
        (
            (base_width as f32 * config.scale).round() as u32,
            (base_height as f32 * config.scale).round() as u32,
        )
    };

    let scale_x = width as f32 / base_width as f32;
    let scale_y = height as f32 / base_height as f32;

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);

    log::info!("Detected {} CPU cores for parallelization.", cores);

    formats.par_iter().for_each(|f| {
        let id =
            rayon::current_thread_index().expect("should be called from a Rayon worker thread.");
        let start = std::time::Instant::now();

        let o = {
            let ext = f.extension();
            let mut o = PathBuf::from(&output);
            o.push(&filename);
            o.set_extension(ext);
            o
        };

        let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
        let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("size should not be zero.");

        let bg_color = if let Some(v) = background {
            v
        } else if f.has_alpha_channel() {
            tiny_skia::Color::from_rgba8(0, 0, 0, 0)
        } else {
            tiny_skia::Color::from_rgba8(255, 255, 255, 255)
        };

        pixmap.fill(bg_color);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let res = match f {
            Format::Jpeg => {
                let buf = pixmap_to_rgb_buffer(&pixmap);
                image::save_buffer(&o, &buf, width, height, ExtendedColorType::Rgb8)
            }
            _ => image::save_buffer(&o, pixmap.data(), width, height, ExtendedColorType::Rgba8),
        };

        if let Err(e) = res {
            log::error!(
                "[thread_id={}] Failed to generate '{}' Caused by: {}",
                id,
                o.file_name()
                    .expect("path should not be terminates in `..`.")
                    .display(),
                e
            );
        } else {
            let elapsed = if start.elapsed().as_secs() > 0 {
                format!("{}s", start.elapsed().as_secs())
            } else {
                format!(
                    "{}ms",
                    start.elapsed().as_millis().min(u64::MAX as u128) as u64
                )
            };

            log::info!(
                "[thread_id={}] Generated: '{}' in {}",
                id,
                o.file_name()
                    .expect("path should not be terminates in `..`.")
                    .display(),
                elapsed,
            );
        }
    });

    Ok(())
}
