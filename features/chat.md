# Chat and text components

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| chat.signing.chain | Validate player message signature chains and reset the chain on session changes | `SignedMessageChain; RemoteChatSession` | | - | todo |
| chat.signing.last-seen | Unpack last-seen message acknowledgements and reject invalid acknowledgement state | `LastSeenMessagesValidator; ServerGamePacketListenerImpl#unpackAndApplyLastSeen` | | - | todo |
| chat.signing.timestamp | Check signed-message timestamp ordering and expire untrusted messages | `ServerGamePacketListenerImpl#tryHandleChat; PlayerChatMessage` | | - | todo |
| chat.signing.command-arguments | Validate signed command arguments and reject unsigned or mismatched signed arguments | `ServerGamePacketListenerImpl#collectSignedArguments; SignableCommand` | | - | todo |
| chat.signing.session-key | Validate session public key certificates and reject expired or invalid keys | `ProfilePublicKey; RemoteChatSession` | | - | todo |
| chat.broadcast.visibility | Respect player chat visibility, sender filtering and recipient-specific trust state | `PlayerList#broadcastChatMessage; ServerGamePacketListenerImpl` | | - | todo |
| chat.broadcast.types | Decorate player chat, disguised chat and system messages with registered chat types | `ChatType; ChatTypeDecoration; OutgoingChatMessage` | | - | todo |
| chat.broadcast.spam | Apply independent command and chat spam thresholds and disconnect spammers | `ServerGamePacketListenerImpl#detectCommandRateSpam; ServerGamePacketListenerImpl#detectChatRateSpam` | | - | todo |
| chat.filtering.external | Apply configured external text filter and per-player filtering preferences to chat, books and signs | `ServerTextFilter; ServerGamePacketListenerImpl#filterTextPacket` | | - | todo |
| chat.components.serialization | Encode and decode literal, translatable, score, selector, keybind and NBT text components | `ComponentSerialization; ComponentContents` | | - | todo |
| chat.components.resolve-context | Resolve contextual text against command source, entity selectors and NBT sources | `ComponentUtils#resolve; NbtContents; SelectorContents` | | - | todo |
| chat.components.style-events | Preserve styles, fonts, insertion, click events and hover events in text codecs | `Style; ClickEvent; HoverEvent` | | - | todo |
| chat.components.translation | Resolve translation arguments, fallback text and invalid argument cases | `TranslatableContents` | | - | todo |
| chat.commands.private-team | Route private and team messages with their distinct chat types and signed argument context | `MsgCommand; TeamMsgCommand` | | - | todo |
| chat.death-messages.rules | Publish death messages according to gamerule and team death-message visibility | `ServerPlayer#die; PlayerTeam` | | - | todo |
| chat.chat-type.chat | `minecraft:chat` (chat type) | `data/minecraft/chat_type/chat.json`; `ChatType#DIRECT_CODEC` | `ferraloom-parity::starter::parity_chat_formatting` | - | todo |
| chat.chat-type.emote-command | `minecraft:emote_command` (chat type) | `data/minecraft/chat_type/emote_command.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.chat-type.msg-command-incoming | `minecraft:msg_command_incoming` (chat type) | `data/minecraft/chat_type/msg_command_incoming.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.chat-type.msg-command-outgoing | `minecraft:msg_command_outgoing` (chat type) | `data/minecraft/chat_type/msg_command_outgoing.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.chat-type.say-command | `minecraft:say_command` (chat type) | `data/minecraft/chat_type/say_command.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.chat-type.team-msg-command-incoming | `minecraft:team_msg_command_incoming` (chat type) | `data/minecraft/chat_type/team_msg_command_incoming.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.chat-type.team-msg-command-outgoing | `minecraft:team_msg_command_outgoing` (chat type) | `data/minecraft/chat_type/team_msg_command_outgoing.json`; `ChatType#DIRECT_CODEC` | | - | todo |
| chat.broadcast.offline-multiple-clients | Deliver unsigned offline chat to a second client with exact sender display-name decoration | `PlayerList#broadcastChatMessage; ChatType#bind; Player#getDisplayName` | `ferraloom-parity::starter::parity_chat_formatting` | - | partial |
