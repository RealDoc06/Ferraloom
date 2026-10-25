# Networking and login

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| networking-login.transport.frame-varint | Decode VarInt packet lengths and reject oversized or malformed frames | `Varint21FrameDecoder#decode; Varint21LengthFieldPrepender#encode` |  | - | partial |
| networking-login.transport.codec-state | Switch inbound and outbound codecs at handshake, login, configuration and play transitions | `Connection#setupInboundProtocol; Connection#setupOutboundProtocol` | `join::offline_client_joins_walks_chats_and_saves`; `ferraloom-parity::starter::parity_join_and_spawn` | pass | partial |
| networking-login.transport.invalid-packet | Disconnect on invalid packet ids, trailing data and codec failures | `PacketDecoder#decode; Connection#exceptionCaught` |  | - | partial |
| networking-login.transport.compression-threshold | Negotiate compression before the following login packets; compressed and uncompressed threshold branches | `Connection#setupCompression; CompressionDecoder#decode; CompressionEncoder#encode` | `join::offline_client_joins_walks_chats_and_saves` | - | partial |
| networking-login.transport.compression-bounds | Reject decompressed lengths outside protocol limits and invalid compressed payloads | `CompressionDecoder#decode` |  | - | partial |
| networking-login.transport.encryption | RSA challenge and shared secret; install AES stream encryption in both directions | `ServerLoginPacketListenerImpl#handleKey; Connection#setEncryptionKey; Crypt` |  | - | partial |
| networking-login.transport.flush-order | Maintain queued packet and flush order through listener and protocol transitions | `Connection#send; Connection#flushChannel; Connection#runOnceConnected` | `join::offline_client_joins_walks_chats_and_saves` | - | partial |
| networking-login.transport.rate-limit | Apply configured per-connection packet rate limiting and disconnect excessive senders | `RateKickingConnection#tickSecond` |  | - | partial |
| networking-login.transport.timeout | Apply read timeout and listener keepalive timeout disconnects | `ServerConnectionListener; ServerCommonPacketListenerImpl#keepConnectionAlive` |  | - | partial |
| networking-login.transport.bind | Bind configured address and port with native transport selection and connection acceptance | `ServerConnectionListener#startTcpServerListener` |  | - | partial |
| networking-login.handshake.version | Reject incompatible protocol versions with vanilla disconnect text | `ServerHandshakePacketListenerImpl#handleIntention` |  | - | partial |
| networking-login.handshake.intention | Route status, login and transfer intentions and reject unsupported intentions | `ServerHandshakePacketListenerImpl#handleIntention` |  | - | partial |
| networking-login.login.name-validation | Validate player name before authentication | `ServerLoginPacketListenerImpl#handleHello` |  | - | partial |
| networking-login.login.online-auth | Authenticate signed server hash and join address with the session service | `ServerLoginPacketListenerImpl#handleKey` |  | - | partial |
| networking-login.login.auth-failure | Handle authentication service failure and unverified usernames | `ServerLoginPacketListenerImpl#handleKey` |  | - | partial |
| networking-login.login.offline-profile | Derive offline UUID and profile when online authentication is disabled | `ServerLoginPacketListenerImpl#handleHello` | `join::offline_client_joins_walks_chats_and_saves`; `ferraloom-parity::starter::parity_join_and_spawn` | pass | partial |
| networking-login.login.duplicate-profile | Disconnect duplicate UUID sessions before completing login | `ServerLoginPacketListenerImpl; PlayerList#disconnectAllPlayersWithProfile` |  | - | partial |
| networking-login.login.admission | Check bans, whitelist, player limit and reserved operator admission | `PlayerList#canPlayerLogin` |  | - | partial |
| networking-login.login.timeout | Abort login that does not finish within the login tick limit | `ServerLoginPacketListenerImpl#tick` |  | - | partial |
| networking-login.login.profile-key | Validate player chat public key, signature and expiry under secure-profile policy | `RemoteChatSession#validate; ServerGamePacketListenerImpl#handleChatSessionUpdate` | `chat::tests::signed_chain_rejects_tampering_and_acknowledgement_retraction` | - | partial |
| networking-login.configuration.registry-sync | Send registry data and reconcile known datapacks before entering play | `ServerConfigurationPacketListenerImpl; SynchronizeRegistriesTask` | `join::join_and_configuration_sequence_matches_vanilla`; `ferraloom-parity::starter::parity_join_and_spawn` | pass | partial |
| networking-login.configuration.feature-flags | Send enabled feature flags and tags for the configured world | `ServerConfigurationPacketListenerImpl` | `join::join_and_configuration_sequence_matches_vanilla` | pass | partial |
| networking-login.configuration.code-of-conduct | Send configured code of conduct and wait for its acceptance | `ServerConfigurationPacketListenerImpl; ServerCodeOfConductConfigurationTask` |  | - | partial |
| networking-login.configuration.client-info | Apply locale, view distance, chat visibility, skin parts, hand and listing preference | `ServerConfigurationPacketListenerImpl#handleClientInformation; ServerPlayer#updateOptions` |  | - | partial |
| networking-login.configuration.task-order | Complete configuration tasks in order and reject unexpected task acknowledgements | `ServerConfigurationPacketListenerImpl#finishCurrentTask` | `join::join_and_configuration_sequence_matches_vanilla` | pass | partial |
| networking-login.configuration.reconfigure | Return an existing play connection to configuration and resume the session | `ServerGamePacketListenerImpl#handleConfigurationAcknowledged` | | - | todo |
| networking-login.resource-pack.push-pop | Send server resource-pack UUID, URL, hash, prompt and required flag; push and remove packs | `ServerCommonPacketListenerImpl; ClientboundResourcePackPushPacket; ClientboundResourcePackPopPacket` |  | - | partial |
| networking-login.resource-pack.required | Disconnect on refused or failed mandatory resource pack and process accepted and loaded states | `ServerCommonPacketListenerImpl#handleResourcePackResponse; ServerConfigurationPacketListenerImpl` |  | - | partial |
| networking-login.cookies.request-store | Request client cookies, handle bounded cookie responses and send stored cookies | `ServerCommonPacketListenerImpl#handleCookieResponse; ClientboundStoreCookiePacket` | | - | todo |
| networking-login.transfer.policy | Respect accepts-transfers for transfer-intention logins | `ServerHandshakePacketListenerImpl#handleIntention` |  | - | partial |
| networking-login.transfer.destination | Send transfer host and port to selected players | `TransferCommand#transfer; ClientboundTransferPacket` | | - | todo |
| networking-login.server-links | Send server links and bug-report links to connecting players | `ServerLinks; ServerConfigurationPacketListenerImpl` |  | - | partial |
| networking-login.custom-payload.brand | Process custom payloads and advertise vanilla server brand | `ServerCommonPacketListenerImpl#handleCustomPayload; BrandPayload` |  | - | partial |
| networking-login.status.response | Return MOTD, version, player count and sample, favicon and secure-chat enforcement | `ServerStatusPacketListenerImpl#handleStatusRequest; MinecraftServer#buildServerStatus` |  | - | partial |
| networking-login.status.sample-privacy | Hide listing-disabled profiles and honor hide-online-players | `MinecraftServer#buildPlayerStatus` |  | - | partial |
| networking-login.status.ping | Echo ping timestamp and close the status connection | `ServerStatusPacketListenerImpl#handlePingRequest` |  | - | partial |
| networking-login.status.disabled | Reject status requests when enable-status is false | `ServerHandshakePacketListenerImpl#handleIntention` |  | - | partial |
| networking-login.legacy-ping | Respond to legacy ping variants with their expected strings and framing | `LegacyQueryHandler#channelRead` |  | - | partial |
| networking-login.rcon.authentication | Authenticate RCON requests and reject incorrect passwords | `RconClient#run` | | - | todo |
| networking-login.rcon.command-response | Execute RCON commands and split long responses into bounded packets | `RconClient; DedicatedServer#runCommand` | | - | todo |
| networking-login.query.challenge | Issue UDP query challenges and validate challenge tokens | `QueryThreadGs4` | | - | todo |
| networking-login.query.status-rules | Return query basic and full server and player responses | `QueryThreadGs4` | | - | todo |
| networking-login.custom-payload.unknown-bounds | Discard unknown payload contents while enforcing 32767-byte serverbound and 1048576-byte clientbound limits | `ServerboundCustomPayloadPacket; ClientboundCustomPayloadPacket; DiscardedPayload` | | - | todo |
| networking-login.bundle.delimiter | Decode and encode packet bundles using delimiters and enforce bundle packet constraints | `PacketBundlePacker; PacketBundleUnpacker; BundlerInfo` | | - | todo |
| networking-login.login.custom-query | Reject unexpected login custom-query replies with the vanilla login behavior | `ServerLoginPacketListenerImpl#handleCustomQueryPacket` | | - | todo |
| networking-login.security.chat-certificate | Verify profile certificates, signed message chains and last-seen acknowledgement windows | `ProfilePublicKey; SignedMessageChain; LastSeenMessagesValidator` | `chat::tests::signed_chain_rejects_tampering_and_acknowledgement_retraction` | - | partial |
