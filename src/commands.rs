use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use serenity::all::{
    ActionRowComponent, ComponentInteractionCollector, ComponentInteractionDataKind,
    CreateActionRow, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateModal, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, EmojiId,
    InputTextStyle, Message, ModalInteractionCollector, ReactionType,
};

use crate::emoji::{self, ParsedEmoji};

pub struct Data {}
pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

fn build_modal(picked: &[ParsedEmoji]) -> CreateModal {
    let rows: Vec<CreateActionRow> = picked
        .iter()
        .enumerate()
        .map(|(i, e)| {
            CreateActionRow::InputText(
                CreateInputText::new(
                    InputTextStyle::Short,
                    format!("Name for :{}:", e.name),
                    i.to_string(),
                )
                .value(&e.name)
                .required(true)
                .min_length(2)
                .max_length(32),
            )
        })
        .collect();
    CreateModal::new("steal_modal", "Name the emoji(s)").components(rows)
}

#[poise::command(context_menu_command = "Steal Emoji", guild_only)]
pub async fn steal_emoji(ctx: Context<'_>, message: Message) -> Result<(), Error> {
    let serenity_ctx = ctx.serenity_context();

    let Some(guild_id) = ctx.guild_id() else {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("This command only works inside a server."),
        )
        .await?;
        return Ok(());
    };

    let emojis = emoji::parse(&message.content);
    if emojis.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("No custom emojis found in that message."),
        )
        .await?;
        return Ok(());
    }

    let Context::Application(app_ctx) = ctx else {
        return Ok(());
    };

    let picked: Vec<ParsedEmoji> = if emojis.len() == 1 {
        app_ctx
            .interaction
            .create_response(
                serenity_ctx,
                CreateInteractionResponse::Modal(build_modal(&emojis)),
            )
            .await?;
        emojis
    } else {
        let options: Vec<CreateSelectMenuOption> = emojis
            .iter()
            .enumerate()
            .map(|(i, e)| {
                CreateSelectMenuOption::new(&e.name, i.to_string()).emoji(ReactionType::Custom {
                    animated: e.animated,
                    id: EmojiId::new(e.id),
                    name: Some(e.name.clone()),
                })
            })
            .collect();

        let max = emojis.len().min(5) as u8;
        let menu = CreateSelectMenu::new("steal_select", CreateSelectMenuKind::String { options })
            .min_values(1)
            .max_values(max);

        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content("Select up to 5 emojis to steal:")
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

        let picked: Vec<ParsedEmoji> = values
            .iter()
            .filter_map(|v| v.parse::<usize>().ok())
            .filter_map(|i| emojis.get(i).cloned())
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

    let mut results = Vec::new();
    for (i, e) in picked.iter().enumerate() {
        let name = names
            .get(&i.to_string())
            .filter(|n| !n.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| e.name.clone());

        match emoji::steal(&serenity_ctx.http, guild_id, e, name.trim()).await {
            Ok(created) => results.push(format!("Stole {}", created)),
            Err(err) => results.push(format!("Failed `{}`: {}", name.trim(), err)),
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
