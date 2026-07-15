use std::collections::HashSet;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serenity::all::{CreateAttachment, Emoji, GuildId, Http};

static EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(a?):(\w{2,32}):(\d+)>").unwrap());

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
    let attachment = CreateAttachment::url(http, &emoji.cdn_url()).await?;
    let image = attachment.to_base64();
    let created = guild_id.create_emoji(http, name, &image).await?;
    Ok(created)
}
