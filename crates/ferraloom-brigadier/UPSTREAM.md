Reference: Mojang Brigadier **1.3.11**, shipped with Minecraft 26.3.

Upstream repository: https://github.com/Mojang/brigadier

There is no published 1.3.11 Git tag (the repository has only 1.0.15,
1.0.17 and 1.0.18 tags). The Java main sources at commit
`9ba4f13c0fe82b07c08c2dc2d8043f075ffd0d98` match every file in the published
`brigadier-1.3.11-sources.jar` byte for byte. Tests are ported from that commit.

The upstream MIT notice is retained in LICENSE.brigadier. The Rust implementation
uses an owned node arena with stable indices instead of mutable Java node objects.
Sources and erased argument values use Arc; no Clone bound is imposed on sources.
Exception providers are scoped to readers/dispatchers rather than mutable globals.
Suggestions are synchronous. Cursors and ranges count UTF-16 code units.
