use std::collections::HashSet;
use std::io::Cursor;
use std::sync::LazyLock;

use anyhow::{Result, anyhow, bail};
use image::codecs::gif::{GifDecoder, GifEncoder};
use image::codecs::png::{CompressionType as PngCompressionType, FilterType as PngFilterType, PngEncoder};
use image::{AnimationDecoder, Frame, imageops};
use regex::Regex;
use serenity::all::{CreateAttachment, Emoji, GuildId, Http};

static EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(a?):(\w{2,32}):(\d+)>").unwrap());

const EMOJI_LIMIT: usize = 256 * 1024;
const MIN_DIMENSION: u32 = 32;
const SCALE_STEP: f32 = 0.85;

#[derive(Clone)]
pub struct ParsedEmoji {
    pub animated: bool,
    pub name: String,
    pub id: u64,
}

impl ParsedEmoji {
    pub fn cdn_url(&self) -> String {
        let ext = if self.animated { "gif" } else { "png" };
        format!("https://cdn.discordapp.com/emojis/{}.{}", self.id, ext)
    }
}

pub fn parse(content: &str) -> Vec<ParsedEmoji> {
    let mut seen = HashSet::new();
    let mut emojis = Vec::new();
    for caps in EMOJI_RE.captures_iter(content) {
        let id: u64 = match caps[3].parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        if !seen.insert(id) {
            continue;
        }
        emojis.push(ParsedEmoji {
            animated: &caps[1] == "a",
            name: caps[2].to_string(),
            id,
        });
    }
    emojis
}

pub async fn steal(
    http: &Http,
    guild_id: GuildId,
    emoji: &ParsedEmoji,
    name: &str,
) -> Result<Emoji> {
    let mut attachment = CreateAttachment::url(http, &emoji.cdn_url()).await?;
    if attachment.data.len() > EMOJI_LIMIT {
        attachment.data = shrink_to_limit(&attachment.data, emoji.animated, EMOJI_LIMIT)?;
    }
    let image = attachment.to_base64();
    let created = guild_id.create_emoji(http, name, &image).await?;
    Ok(created)
}

fn shrink_to_limit(bytes: &[u8], animated: bool, limit: usize) -> Result<Vec<u8>> {
    if animated {
        shrink_gif(bytes, limit)
    } else {
        shrink_static(bytes, limit)
    }
}

fn encode_png(img: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut buf, PngCompressionType::Best, PngFilterType::Adaptive);
    img.write_with_encoder(encoder)?;
    Ok(buf)
}

fn shrink_static(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)?;
    let recompressed = encode_png(&img)?;
    if recompressed.len() <= limit {
        return Ok(recompressed);
    }

    let mut width = img.width();
    let mut height = img.height();
    loop {
        width = ((width as f32) * SCALE_STEP).max(MIN_DIMENSION as f32) as u32;
        height = ((height as f32) * SCALE_STEP).max(MIN_DIMENSION as f32) as u32;

        let resized = img.resize(width, height, imageops::FilterType::Lanczos3);
        let out = encode_png(&resized)?;

        if out.len() <= limit {
            return Ok(out);
        }
        if width <= MIN_DIMENSION || height <= MIN_DIMENSION {
            bail!(
                "still {} KB after shrinking to {}x{} (minimum size reached)",
                out.len() / 1024,
                width,
                height
            );
        }
    }
}

fn encode_gif(frames: Vec<Frame>) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut buf);
        encoder.encode_frames(frames)?;
    }
    Ok(buf)
}

fn shrink_gif(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    let decoder = GifDecoder::new(Cursor::new(bytes))?;
    let frames = decoder.into_frames().collect_frames()?;
    let (orig_width, orig_height) = frames
        .first()
        .map(|f| f.buffer().dimensions())
        .ok_or_else(|| anyhow!("GIF has no frames"))?;

    let mut width = orig_width;
    let mut height = orig_height;
    loop {
        width = ((width as f32) * SCALE_STEP).max(MIN_DIMENSION as f32) as u32;
        height = ((height as f32) * SCALE_STEP).max(MIN_DIMENSION as f32) as u32;

        let resized: Vec<Frame> = frames
            .iter()
            .map(|f| {
                let buffer = imageops::resize(f.buffer(), width, height, imageops::FilterType::Lanczos3);
                Frame::from_parts(buffer, 0, 0, f.delay())
            })
            .collect();
        let out = encode_gif(resized)?;

        if out.len() <= limit {
            return Ok(out);
        }
        if width <= MIN_DIMENSION || height <= MIN_DIMENSION {
            bail!(
                "still {} KB after shrinking to {}x{} (minimum size reached)",
                out.len() / 1024,
                width,
                height
            );
        }
    }
}
