# emojistealer
This is made for a friend 

Discord bot that steals emojis and stickers from other servers.

Right-click a message -> "Steal Emoji/Sticker". Also works on emojis used as reactions.

Or use `/steal` and paste emoji(s) directly, e.g. `<:name:id>`.

If there's multiple hits you get to pick which ones (up to 5) and rename them before upload.

Commands are restricted to server admins by default. Bot itself only needs View Channel, Read Message History and Manage Emojis and Stickers, not full admin.

## Setup

Create a `.env` file with `DISCORD_TOKEN=your_token` (see `.env.example`).

Then `cargo run`.

### Or with Docker

``` docker-compose.yml
services:
  bot:
    image: ghcr.io/fthomys/emojistealer-bot:latest
    env_file:
      - .env
    restart: unless-stopped

```

`docker compose up -d` uses the prebuilt image from `ghcr.io/fthomys/emojistealer-bot`. In the same folder where the docker compose file is, put the `.env` with the token.

For a local build instead, use `docker compose -f docker-compose.dev.yml up -d --build`.

## License

WTFPL, see LICENSE.
