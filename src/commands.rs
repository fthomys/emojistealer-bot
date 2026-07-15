use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::Result;
use serenity::all::{
    ActionRowComponent, ComponentInteractionCollector, ComponentInteractionDataKind,
    CreateActionRow, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateModal, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, EmojiId, GuildId,
    InputTextStyle, Message, ModalInteractionCollector, ReactionType,
};

use crate::emoji::{self, ParsedEmoji, ParsedSticker};

pub struct Data {}
pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Clone)]
enum StealItem {
    Emoji(ParsedEmoji),
    Sticker(ParsedSticker),
}

impl StealItem {
    fn display_name(&self) -> &str {
        match self {
            StealItem::Emoji(e) => &e.name,
            StealItem::Sticker(s) => &s.name,
        }
    }

    fn kind_label(&self) -> &'static str {
        match self {
            StealItem::Emoji(_) => "emoji",
            StealItem::Sticker(_) => "sticker",
        }
    }
}

enum StealSource {
    Message(String),
    Pasted,
}

impl StealSource {
    fn describe(&self) -> String {
        match self {
            StealSource::Message(link) => link.clone(),
            StealSource::Pasted => "Pasted via `/steal`".to_string(),
        }
    }
}

fn build_modal(picked: &[StealItem]) -> CreateModal {
    let rows: Vec<CreateActionRow> = picked
        .iter()
        .enumerate()
        .map(|(i, item)| {
            CreateActionRow::InputText(
                CreateInputText::new(
                    InputTextStyle::Short,
                    format!("Name for {} :{}:", item.kind_label(), item.display_name()),
                    i.to_string(),
                )
                .value(item.display_name())
                .required(true)
                .min_length(2)
                .max_length(32),
            )
        })
        .collect();
    CreateModal::new("steal_modal", "Name the item(s)").components(rows)
}

async fn run_steal_flow(
    ctx: Context<'_>,
    guild_id: GuildId,
    items: Vec<StealItem>,
    source: StealSource,
) -> Result<(), Error> {
    if items.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("No custom emojis or stickers found."),
        )
        .await?;
        return Ok(());
    }

    let serenity_ctx = ctx.serenity_context();

    let Context::Application(app_ctx) = ctx else {
        return Ok(());
    };

    let picked: Vec<StealItem> = if items.len() == 1 {
        app_ctx
            .interaction
            .create_response(
                serenity_ctx,
                CreateInteractionResponse::Modal(build_modal(&items)),
            )
            .await?;
        items
    } else {
        let options: Vec<CreateSelectMenuOption> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let label = format!("{} ({})", item.display_name(), item.kind_label());
                let mut option = CreateSelectMenuOption::new(label, i.to_string());
                if let StealItem::Emoji(e) = item {
                    option = option.emoji(ReactionType::Custom {
                        animated: e.animated,
                        id: EmojiId::new(e.id),
                        name: Some(e.name.clone()),
                    });
                }
                option
            })
            .collect();

        let max = items.len().min(5) as u8;
        let menu = CreateSelectMenu::new("steal_select", CreateSelectMenuKind::String { options })
            .min_values(1)
            .max_values(max);

        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("Select up to 5 items to steal:")
                .components(vec![CreateActionRow::SelectMenu(menu)]),
        )
        .await?;

        let Some(interaction) = ComponentInteractionCollector::new(serenity_ctx)
            .author_id(ctx.author().id)
            .custom_ids(vec!["steal_select".to_string()])
            .timeout(Duration::from_secs(120))
            .await
        else {
            return Ok(());
        };

        let ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind else {
            return Ok(());
        };

        let picked: Vec<StealItem> = values
            .iter()
            .filter_map(|v| v.parse::<usize>().ok())
            .filter_map(|i| items.get(i).cloned())
            .collect();

        if picked.is_empty() {
            return Ok(());
        }

        interaction
            .create_response(
                serenity_ctx,
                CreateInteractionResponse::Modal(build_modal(&picked)),
            )
            .await?;
        picked
    };

    let Some(modal) = ModalInteractionCollector::new(serenity_ctx)
        .author_id(ctx.author().id)
        .custom_ids(vec!["steal_modal".to_string()])
        .timeout(Duration::from_secs(300))
        .await
    else {
        return Ok(());
    };

    let mut names: HashMap<String, String> = HashMap::new();
    for row in &modal.data.components {
        for component in &row.components {
            if let ActionRowComponent::InputText(input) = component {
                names.insert(
                    input.custom_id.clone(),
                    input.value.clone().unwrap_or_default(),
                );
            }
        }
    }

    let reason = format!(
        "Stolen by {} via {}",
        ctx.author().name,
        source.describe()
    );

    let mut results = Vec::new();
    for (i, item) in picked.iter().enumerate() {
        let name = names
            .get(&i.to_string())
            .filter(|n| !n.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| item.display_name().to_string());
        let name = name.trim().to_string();

        let outcome = match item {
            StealItem::Emoji(e) => emoji::steal(&serenity_ctx.http, guild_id, e, &name, &reason)
                .await
                .map(|created| format!("Stole {}", created)),
            StealItem::Sticker(s) => {
                emoji::steal_sticker(&serenity_ctx.http, guild_id, s, &name, &reason)
                    .await
                    .map(|created| format!("Stole sticker `{}`", created.name))
            }
        };

        match outcome {
            Ok(msg) => results.push(msg),
            Err(err) => results.push(format!("Failed `{}`: {}", name, err)),
        }
    }

    modal
        .create_response(
            serenity_ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content(results.join("\n")),
            ),
        )
        .await?;

    Ok(())
}

#[poise::command(
    context_menu_command = "Steal Emoji/Sticker",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn steal_emoji(ctx: Context<'_>, message: Message) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("This command only works inside a server."),
        )
        .await?;
        return Ok(());
    };

    let mut emojis = emoji::parse(&message.content);
    let mut seen: HashSet<u64> = emojis.iter().map(|e| e.id).collect();
    for e in emoji::parse_reactions(&message.reactions) {
        if seen.insert(e.id) {
            emojis.push(e);
        }
    }

    let mut items: Vec<StealItem> = emojis.into_iter().map(StealItem::Emoji).collect();
    items.extend(
        emoji::parse_stickers(&message)
            .into_iter()
            .map(StealItem::Sticker),
    );

    run_steal_flow(ctx, guild_id, items, StealSource::Message(message.link())).await
}

#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn steal(
    ctx: Context<'_>,
    #[description = "Paste one or more custom emoji, e.g. <:name:id>"] emojis: String,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("This command only works inside a server."),
        )
        .await?;
        return Ok(());
    };

    let items: Vec<StealItem> = emoji::parse(&emojis)
        .into_iter()
        .map(StealItem::Emoji)
        .collect();

    run_steal_flow(ctx, guild_id, items, StealSource::Pasted).await
}
