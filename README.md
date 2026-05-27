# Neurobox API

A tiny Rust API for Discord presence and activity data.

It runs a bot gateway connection, keeps recent presence updates in memory, and exposes them through HTTP and websockets.

## Getting Started

In order to use this, simply join the [discord server](https://discord.gg/MH7qGJzBPc) and your presence will start getting tracked by Neurobox. Simply `GET neurobox.app/v1/users/:userid`!

Users are also able to set their own KV. To do so, join the aforementioned [discord server](https://discord.gg/MH7qGJzBPc), go into any channel and run the `/kv set` slash command, it's that simple!

There is no need for you to host anything if you want to use this, though you are able to do so with next to no setup required.

## Example Response

```json
{
  "success": true,
  "data": {
    "user_id": "719186341878825000",
    "discord_user": {
      "id": "719186341878825000",
      "username": "0yaw",
      "discriminator": "0",
      "avatar": "https://cdn.discordapp.com/avatars/719186341878825000/5813076fc1d91d3063d05440efcb3f3a.png?size=1024",
      "display_name": "Matthew"
    },
    "discord_status": "dnd",
    "activities": [
      {
        "name": "Custom Status",
        "type": 4,
        "state": "This is my regular status! Hello!",
        "details": null,
        "application_id": null,
        "sync_id": null,
        "timestamps": {
          "start": null,
          "end": 1779918857138
        },
        "assets": null,
        "emoji": {
          "name": "✅"
        },
        "party": null,
        "flags": null,
        "buttons": null
      },
      {
        "name": "Visual Studio Code",
        "type": 0,
        "state": "Workspace: Neurobox-API",
        "details": "Editing .env",
        "application_id": "383226320970055681",
        "sync_id": null,
        "timestamps": {
          "start": 1779916950858,
          "end": null
        },
        "assets": {
          "large_image": "1359298813033971723",
          "large_text": "Editing a ENV file",
          "small_image": "1359299466493956258",
          "small_text": "Visual Studio Code"
        },
        "emoji": null,
        "party": null,
        "flags": null,
        "buttons": null
      }
    ],
    "active_on_discord_desktop": true,
    "active_on_discord_mobile": false,
    "active_on_discord_web": false,
    "listening_to_spotify": false,
    "spotify": null,
    "kv": {
      "site": "yaw.cx"
    }
  }
}
```

## HTTP Endpoints

```http
GET /health
GET /v1/users/{user_id}
POST /v1/users
```

`/v1/users` - Batch request:
```json
{
  "user_ids": ["123456789012345678", "234567890123456789"]
}
```

## Websocket

Connect to:

```text
ws://neurobox.app/socket
```

Watch one user:

```json
{ "op": 2, "d": { "user_id": "123456789012345678" } }
```

Watch multiple users:

```json
{ "op": 2, "d": { "user_ids": ["123456789012345678"] } }
```

Watch every cached user:

```json
{ "op": 2, "d": { "all": true } }
```

The socket sends `HELLO`, `INIT_STATE`, and `PRESENCE_UPDATE` messages. Send opcode `3` as a heartbeat at the interval from `HELLO`.
