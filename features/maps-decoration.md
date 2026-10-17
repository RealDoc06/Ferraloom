# Maps, decoration and variants

Includes map and decoration mechanics, animal variants and sound variants, painting variants, banner and decorated-pot patterns, instruments and armor-trim resources.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| maps-decoration.maps.color-sampling | Sample terrain height, dominant map color and water depth into map pixels | `MapItem#update` | | - | todo |
| maps-decoration.maps.tracking | Maintain per-player map tracking, dirty rectangles, cursors and update packets | `MapItemSavedData` | | - | todo |
| maps-decoration.maps.banners-frames | Add, update and remove banner and item-frame map decorations | `MapItemSavedData; ItemFrame` | | - | todo |
| maps-decoration.maps.exploration | Generate exploration maps with structure destinations and preview terrain | `ExplorationMapFunction` | | - | todo |
| maps-decoration.decorations.paintings | Choose valid painting variants and enforce hanging attachment and space checks | `Painting` | | - | todo |
| maps-decoration.decorations.item-frames | Store, rotate, damage and drop frame items and expose comparator signal | `ItemFrame` | | - | todo |
| maps-decoration.decorations.armor-stands | Place and manipulate armor-stand equipment and poses with disabled-slot restrictions | `ArmorStand; ArmorStandItem` | | - | todo |
| maps-decoration.decorations.mannequins | Persist mannequin profile and equipment and synchronize its display state | `Mannequin` | | - | todo |
| maps-decoration.variants.spawn-selection | Select animal variants and sound variants from spawn conditions and biome tags | `SpawnConditions; VariantUtils` | | - | todo |
| maps-decoration.banners.patterns | Preserve banner base color and ordered pattern layers across item and block forms | `BannerBlockEntity; BannerPatternLayers` | | - | todo |
| maps-decoration.pots.decorations | Preserve decorated pot sherd assignments through rotation, item form and breaking | `PotDecorations; DecoratedPotBlockEntity` | | - | todo |
| maps-decoration.banner-pattern.base | `minecraft:base` (banner pattern) | `data/minecraft/banner_pattern/base.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.border | `minecraft:border` (banner pattern) | `data/minecraft/banner_pattern/border.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.bricks | `minecraft:bricks` (banner pattern) | `data/minecraft/banner_pattern/bricks.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.circle | `minecraft:circle` (banner pattern) | `data/minecraft/banner_pattern/circle.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.creeper | `minecraft:creeper` (banner pattern) | `data/minecraft/banner_pattern/creeper.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.cross | `minecraft:cross` (banner pattern) | `data/minecraft/banner_pattern/cross.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.curly-border | `minecraft:curly_border` (banner pattern) | `data/minecraft/banner_pattern/curly_border.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.diagonal-left | `minecraft:diagonal_left` (banner pattern) | `data/minecraft/banner_pattern/diagonal_left.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.diagonal-right | `minecraft:diagonal_right` (banner pattern) | `data/minecraft/banner_pattern/diagonal_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.diagonal-up-left | `minecraft:diagonal_up_left` (banner pattern) | `data/minecraft/banner_pattern/diagonal_up_left.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.diagonal-up-right | `minecraft:diagonal_up_right` (banner pattern) | `data/minecraft/banner_pattern/diagonal_up_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.flow | `minecraft:flow` (banner pattern) | `data/minecraft/banner_pattern/flow.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.flower | `minecraft:flower` (banner pattern) | `data/minecraft/banner_pattern/flower.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.globe | `minecraft:globe` (banner pattern) | `data/minecraft/banner_pattern/globe.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.gradient | `minecraft:gradient` (banner pattern) | `data/minecraft/banner_pattern/gradient.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.gradient-up | `minecraft:gradient_up` (banner pattern) | `data/minecraft/banner_pattern/gradient_up.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.guster | `minecraft:guster` (banner pattern) | `data/minecraft/banner_pattern/guster.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.half-horizontal | `minecraft:half_horizontal` (banner pattern) | `data/minecraft/banner_pattern/half_horizontal.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.half-horizontal-bottom | `minecraft:half_horizontal_bottom` (banner pattern) | `data/minecraft/banner_pattern/half_horizontal_bottom.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.half-vertical | `minecraft:half_vertical` (banner pattern) | `data/minecraft/banner_pattern/half_vertical.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.half-vertical-right | `minecraft:half_vertical_right` (banner pattern) | `data/minecraft/banner_pattern/half_vertical_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.mojang | `minecraft:mojang` (banner pattern) | `data/minecraft/banner_pattern/mojang.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.piglin | `minecraft:piglin` (banner pattern) | `data/minecraft/banner_pattern/piglin.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.rhombus | `minecraft:rhombus` (banner pattern) | `data/minecraft/banner_pattern/rhombus.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.skull | `minecraft:skull` (banner pattern) | `data/minecraft/banner_pattern/skull.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.small-stripes | `minecraft:small_stripes` (banner pattern) | `data/minecraft/banner_pattern/small_stripes.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.square-bottom-left | `minecraft:square_bottom_left` (banner pattern) | `data/minecraft/banner_pattern/square_bottom_left.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.square-bottom-right | `minecraft:square_bottom_right` (banner pattern) | `data/minecraft/banner_pattern/square_bottom_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.square-top-left | `minecraft:square_top_left` (banner pattern) | `data/minecraft/banner_pattern/square_top_left.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.square-top-right | `minecraft:square_top_right` (banner pattern) | `data/minecraft/banner_pattern/square_top_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.straight-cross | `minecraft:straight_cross` (banner pattern) | `data/minecraft/banner_pattern/straight_cross.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-bottom | `minecraft:stripe_bottom` (banner pattern) | `data/minecraft/banner_pattern/stripe_bottom.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-center | `minecraft:stripe_center` (banner pattern) | `data/minecraft/banner_pattern/stripe_center.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-downleft | `minecraft:stripe_downleft` (banner pattern) | `data/minecraft/banner_pattern/stripe_downleft.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-downright | `minecraft:stripe_downright` (banner pattern) | `data/minecraft/banner_pattern/stripe_downright.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-left | `minecraft:stripe_left` (banner pattern) | `data/minecraft/banner_pattern/stripe_left.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-middle | `minecraft:stripe_middle` (banner pattern) | `data/minecraft/banner_pattern/stripe_middle.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-right | `minecraft:stripe_right` (banner pattern) | `data/minecraft/banner_pattern/stripe_right.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.stripe-top | `minecraft:stripe_top` (banner pattern) | `data/minecraft/banner_pattern/stripe_top.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.triangle-bottom | `minecraft:triangle_bottom` (banner pattern) | `data/minecraft/banner_pattern/triangle_bottom.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.triangle-top | `minecraft:triangle_top` (banner pattern) | `data/minecraft/banner_pattern/triangle_top.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.triangles-bottom | `minecraft:triangles_bottom` (banner pattern) | `data/minecraft/banner_pattern/triangles_bottom.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.banner-pattern.triangles-top | `minecraft:triangles_top` (banner pattern) | `data/minecraft/banner_pattern/triangles_top.json`; `RegistryDataLoader; BannerPattern` | | - | todo |
| maps-decoration.cat-sound-variant.classic | `minecraft:classic` (cat sound variant) | `data/minecraft/cat_sound_variant/classic.json`; `RegistryDataLoader; CatSoundVariant` | | - | todo |
| maps-decoration.cat-sound-variant.royal | `minecraft:royal` (cat sound variant) | `data/minecraft/cat_sound_variant/royal.json`; `RegistryDataLoader; CatSoundVariant` | | - | todo |
| maps-decoration.cat-variant.all-black | `minecraft:all_black` (cat variant) | `data/minecraft/cat_variant/all_black.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.black | `minecraft:black` (cat variant) | `data/minecraft/cat_variant/black.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.british-shorthair | `minecraft:british_shorthair` (cat variant) | `data/minecraft/cat_variant/british_shorthair.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.calico | `minecraft:calico` (cat variant) | `data/minecraft/cat_variant/calico.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.jellie | `minecraft:jellie` (cat variant) | `data/minecraft/cat_variant/jellie.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.persian | `minecraft:persian` (cat variant) | `data/minecraft/cat_variant/persian.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.ragdoll | `minecraft:ragdoll` (cat variant) | `data/minecraft/cat_variant/ragdoll.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.red | `minecraft:red` (cat variant) | `data/minecraft/cat_variant/red.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.siamese | `minecraft:siamese` (cat variant) | `data/minecraft/cat_variant/siamese.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.tabby | `minecraft:tabby` (cat variant) | `data/minecraft/cat_variant/tabby.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.cat-variant.white | `minecraft:white` (cat variant) | `data/minecraft/cat_variant/white.json`; `RegistryDataLoader; CatVariant` | | - | todo |
| maps-decoration.chicken-sound-variant.classic | `minecraft:classic` (chicken sound variant) | `data/minecraft/chicken_sound_variant/classic.json`; `RegistryDataLoader; ChickenSoundVariant` | | - | todo |
| maps-decoration.chicken-sound-variant.picky | `minecraft:picky` (chicken sound variant) | `data/minecraft/chicken_sound_variant/picky.json`; `RegistryDataLoader; ChickenSoundVariant` | | - | todo |
| maps-decoration.chicken-variant.cold | `minecraft:cold` (chicken variant) | `data/minecraft/chicken_variant/cold.json`; `RegistryDataLoader; ChickenVariant` | | - | todo |
| maps-decoration.chicken-variant.temperate | `minecraft:temperate` (chicken variant) | `data/minecraft/chicken_variant/temperate.json`; `RegistryDataLoader; ChickenVariant` | | - | todo |
| maps-decoration.chicken-variant.warm | `minecraft:warm` (chicken variant) | `data/minecraft/chicken_variant/warm.json`; `RegistryDataLoader; ChickenVariant` | | - | todo |
| maps-decoration.cow-sound-variant.classic | `minecraft:classic` (cow sound variant) | `data/minecraft/cow_sound_variant/classic.json`; `RegistryDataLoader; CowSoundVariant` | | - | todo |
| maps-decoration.cow-sound-variant.moody | `minecraft:moody` (cow sound variant) | `data/minecraft/cow_sound_variant/moody.json`; `RegistryDataLoader; CowSoundVariant` | | - | todo |
| maps-decoration.cow-variant.cold | `minecraft:cold` (cow variant) | `data/minecraft/cow_variant/cold.json`; `RegistryDataLoader; CowVariant` | | - | todo |
| maps-decoration.cow-variant.temperate | `minecraft:temperate` (cow variant) | `data/minecraft/cow_variant/temperate.json`; `RegistryDataLoader; CowVariant` | | - | todo |
| maps-decoration.cow-variant.warm | `minecraft:warm` (cow variant) | `data/minecraft/cow_variant/warm.json`; `RegistryDataLoader; CowVariant` | | - | todo |
| maps-decoration.decorated-pot-pattern.angler | `minecraft:angler` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/angler.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.archer | `minecraft:archer` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/archer.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.arms-up | `minecraft:arms_up` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/arms_up.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.blade | `minecraft:blade` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/blade.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.brewer | `minecraft:brewer` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/brewer.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.burn | `minecraft:burn` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/burn.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.danger | `minecraft:danger` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/danger.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.explorer | `minecraft:explorer` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/explorer.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.flow | `minecraft:flow` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/flow.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.friend | `minecraft:friend` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/friend.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.guster | `minecraft:guster` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/guster.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.heart | `minecraft:heart` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/heart.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.heartbreak | `minecraft:heartbreak` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/heartbreak.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.howl | `minecraft:howl` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/howl.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.miner | `minecraft:miner` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/miner.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.mourner | `minecraft:mourner` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/mourner.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.plenty | `minecraft:plenty` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/plenty.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.prize | `minecraft:prize` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/prize.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.scrape | `minecraft:scrape` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/scrape.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.sheaf | `minecraft:sheaf` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/sheaf.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.shelter | `minecraft:shelter` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/shelter.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.skull | `minecraft:skull` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/skull.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.decorated-pot-pattern.snort | `minecraft:snort` (decorated pot pattern) | `data/minecraft/decorated_pot_pattern/snort.json`; `RegistryDataLoader; DecoratedPotPattern` | | - | todo |
| maps-decoration.frog-variant.cold | `minecraft:cold` (frog variant) | `data/minecraft/frog_variant/cold.json`; `RegistryDataLoader; FrogVariant` | | - | todo |
| maps-decoration.frog-variant.temperate | `minecraft:temperate` (frog variant) | `data/minecraft/frog_variant/temperate.json`; `RegistryDataLoader; FrogVariant` | | - | todo |
| maps-decoration.frog-variant.warm | `minecraft:warm` (frog variant) | `data/minecraft/frog_variant/warm.json`; `RegistryDataLoader; FrogVariant` | | - | todo |
| maps-decoration.instrument.admire-goat-horn | `minecraft:admire_goat_horn` (instrument) | `data/minecraft/instrument/admire_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.call-goat-horn | `minecraft:call_goat_horn` (instrument) | `data/minecraft/instrument/call_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.dream-goat-horn | `minecraft:dream_goat_horn` (instrument) | `data/minecraft/instrument/dream_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.feel-goat-horn | `minecraft:feel_goat_horn` (instrument) | `data/minecraft/instrument/feel_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.ponder-goat-horn | `minecraft:ponder_goat_horn` (instrument) | `data/minecraft/instrument/ponder_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.seek-goat-horn | `minecraft:seek_goat_horn` (instrument) | `data/minecraft/instrument/seek_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.sing-goat-horn | `minecraft:sing_goat_horn` (instrument) | `data/minecraft/instrument/sing_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.instrument.yearn-goat-horn | `minecraft:yearn_goat_horn` (instrument) | `data/minecraft/instrument/yearn_goat_horn.json`; `RegistryDataLoader; Instrument` | | - | todo |
| maps-decoration.map-decoration-type.abandoned-camp | `minecraft:abandoned_camp` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.ancient-city | `minecraft:ancient_city` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-black | `minecraft:banner_black` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-blue | `minecraft:banner_blue` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-brown | `minecraft:banner_brown` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-cyan | `minecraft:banner_cyan` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-gray | `minecraft:banner_gray` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-green | `minecraft:banner_green` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-light-blue | `minecraft:banner_light_blue` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-light-gray | `minecraft:banner_light_gray` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-lime | `minecraft:banner_lime` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-magenta | `minecraft:banner_magenta` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-orange | `minecraft:banner_orange` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-pink | `minecraft:banner_pink` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-purple | `minecraft:banner_purple` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-red | `minecraft:banner_red` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-white | `minecraft:banner_white` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.banner-yellow | `minecraft:banner_yellow` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.blue-marker | `minecraft:blue_marker` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.desert-pyramid | `minecraft:desert_pyramid` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.frame | `minecraft:frame` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.jungle-temple | `minecraft:jungle_temple` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.mansion | `minecraft:mansion` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.mineshaft | `minecraft:mineshaft` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.monument | `minecraft:monument` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.ocean-ruin-warm | `minecraft:ocean_ruin_warm` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.player | `minecraft:player` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.player-off-limits | `minecraft:player_off_limits` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.player-off-map | `minecraft:player_off_map` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.red-marker | `minecraft:red_marker` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.red-x | `minecraft:red_x` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.swamp-hut | `minecraft:swamp_hut` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.target-point | `minecraft:target_point` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.target-x | `minecraft:target_x` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.trial-chambers | `minecraft:trial_chambers` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.village-desert | `minecraft:village_desert` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.village-plains | `minecraft:village_plains` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.village-savanna | `minecraft:village_savanna` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.village-snowy | `minecraft:village_snowy` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.map-decoration-type.village-taiga | `minecraft:village_taiga` (map decoration type) | `net.minecraft.world.level.saveddata.maps.MapDecorationTypes#bootstrap` | | - | todo |
| maps-decoration.painting-variant.alban | `minecraft:alban` (painting variant) | `data/minecraft/painting_variant/alban.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.aztec | `minecraft:aztec` (painting variant) | `data/minecraft/painting_variant/aztec.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.aztec2 | `minecraft:aztec2` (painting variant) | `data/minecraft/painting_variant/aztec2.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.backyard | `minecraft:backyard` (painting variant) | `data/minecraft/painting_variant/backyard.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.baroque | `minecraft:baroque` (painting variant) | `data/minecraft/painting_variant/baroque.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.bomb | `minecraft:bomb` (painting variant) | `data/minecraft/painting_variant/bomb.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.bouquet | `minecraft:bouquet` (painting variant) | `data/minecraft/painting_variant/bouquet.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.burning-skull | `minecraft:burning_skull` (painting variant) | `data/minecraft/painting_variant/burning_skull.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.bust | `minecraft:bust` (painting variant) | `data/minecraft/painting_variant/bust.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.cavebird | `minecraft:cavebird` (painting variant) | `data/minecraft/painting_variant/cavebird.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.changing | `minecraft:changing` (painting variant) | `data/minecraft/painting_variant/changing.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.cotan | `minecraft:cotan` (painting variant) | `data/minecraft/painting_variant/cotan.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.courbet | `minecraft:courbet` (painting variant) | `data/minecraft/painting_variant/courbet.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.creebet | `minecraft:creebet` (painting variant) | `data/minecraft/painting_variant/creebet.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.dennis | `minecraft:dennis` (painting variant) | `data/minecraft/painting_variant/dennis.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.donkey-kong | `minecraft:donkey_kong` (painting variant) | `data/minecraft/painting_variant/donkey_kong.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.earth | `minecraft:earth` (painting variant) | `data/minecraft/painting_variant/earth.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.endboss | `minecraft:endboss` (painting variant) | `data/minecraft/painting_variant/endboss.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.fern | `minecraft:fern` (painting variant) | `data/minecraft/painting_variant/fern.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.fighters | `minecraft:fighters` (painting variant) | `data/minecraft/painting_variant/fighters.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.finding | `minecraft:finding` (painting variant) | `data/minecraft/painting_variant/finding.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.fire | `minecraft:fire` (painting variant) | `data/minecraft/painting_variant/fire.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.graham | `minecraft:graham` (painting variant) | `data/minecraft/painting_variant/graham.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.humble | `minecraft:humble` (painting variant) | `data/minecraft/painting_variant/humble.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.kebab | `minecraft:kebab` (painting variant) | `data/minecraft/painting_variant/kebab.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.lowmist | `minecraft:lowmist` (painting variant) | `data/minecraft/painting_variant/lowmist.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.match | `minecraft:match` (painting variant) | `data/minecraft/painting_variant/match.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.meditative | `minecraft:meditative` (painting variant) | `data/minecraft/painting_variant/meditative.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.orb | `minecraft:orb` (painting variant) | `data/minecraft/painting_variant/orb.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.owlemons | `minecraft:owlemons` (painting variant) | `data/minecraft/painting_variant/owlemons.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.passage | `minecraft:passage` (painting variant) | `data/minecraft/painting_variant/passage.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.pigscene | `minecraft:pigscene` (painting variant) | `data/minecraft/painting_variant/pigscene.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.plant | `minecraft:plant` (painting variant) | `data/minecraft/painting_variant/plant.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.pointer | `minecraft:pointer` (painting variant) | `data/minecraft/painting_variant/pointer.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.pond | `minecraft:pond` (painting variant) | `data/minecraft/painting_variant/pond.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.pool | `minecraft:pool` (painting variant) | `data/minecraft/painting_variant/pool.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.prairie-ride | `minecraft:prairie_ride` (painting variant) | `data/minecraft/painting_variant/prairie_ride.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.sea | `minecraft:sea` (painting variant) | `data/minecraft/painting_variant/sea.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.skeleton | `minecraft:skeleton` (painting variant) | `data/minecraft/painting_variant/skeleton.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.skull-and-roses | `minecraft:skull_and_roses` (painting variant) | `data/minecraft/painting_variant/skull_and_roses.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.stage | `minecraft:stage` (painting variant) | `data/minecraft/painting_variant/stage.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.sunflowers | `minecraft:sunflowers` (painting variant) | `data/minecraft/painting_variant/sunflowers.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.sunset | `minecraft:sunset` (painting variant) | `data/minecraft/painting_variant/sunset.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.tides | `minecraft:tides` (painting variant) | `data/minecraft/painting_variant/tides.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.unpacked | `minecraft:unpacked` (painting variant) | `data/minecraft/painting_variant/unpacked.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.void | `minecraft:void` (painting variant) | `data/minecraft/painting_variant/void.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.wanderer | `minecraft:wanderer` (painting variant) | `data/minecraft/painting_variant/wanderer.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.wasteland | `minecraft:wasteland` (painting variant) | `data/minecraft/painting_variant/wasteland.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.water | `minecraft:water` (painting variant) | `data/minecraft/painting_variant/water.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.wind | `minecraft:wind` (painting variant) | `data/minecraft/painting_variant/wind.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.painting-variant.wither | `minecraft:wither` (painting variant) | `data/minecraft/painting_variant/wither.json`; `RegistryDataLoader; PaintingVariant` | | - | todo |
| maps-decoration.pig-sound-variant.big | `minecraft:big` (pig sound variant) | `data/minecraft/pig_sound_variant/big.json`; `RegistryDataLoader; PigSoundVariant` | | - | todo |
| maps-decoration.pig-sound-variant.classic | `minecraft:classic` (pig sound variant) | `data/minecraft/pig_sound_variant/classic.json`; `RegistryDataLoader; PigSoundVariant` | | - | todo |
| maps-decoration.pig-sound-variant.mini | `minecraft:mini` (pig sound variant) | `data/minecraft/pig_sound_variant/mini.json`; `RegistryDataLoader; PigSoundVariant` | | - | todo |
| maps-decoration.pig-variant.cold | `minecraft:cold` (pig variant) | `data/minecraft/pig_variant/cold.json`; `RegistryDataLoader; PigVariant` | | - | todo |
| maps-decoration.pig-variant.temperate | `minecraft:temperate` (pig variant) | `data/minecraft/pig_variant/temperate.json`; `RegistryDataLoader; PigVariant` | | - | todo |
| maps-decoration.pig-variant.warm | `minecraft:warm` (pig variant) | `data/minecraft/pig_variant/warm.json`; `RegistryDataLoader; PigVariant` | | - | todo |
| maps-decoration.trim-material.amethyst | `minecraft:amethyst` (trim material) | `data/minecraft/trim_material/amethyst.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.copper | `minecraft:copper` (trim material) | `data/minecraft/trim_material/copper.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.diamond | `minecraft:diamond` (trim material) | `data/minecraft/trim_material/diamond.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.emerald | `minecraft:emerald` (trim material) | `data/minecraft/trim_material/emerald.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.gold | `minecraft:gold` (trim material) | `data/minecraft/trim_material/gold.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.iron | `minecraft:iron` (trim material) | `data/minecraft/trim_material/iron.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.lapis | `minecraft:lapis` (trim material) | `data/minecraft/trim_material/lapis.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.netherite | `minecraft:netherite` (trim material) | `data/minecraft/trim_material/netherite.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.quartz | `minecraft:quartz` (trim material) | `data/minecraft/trim_material/quartz.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.redstone | `minecraft:redstone` (trim material) | `data/minecraft/trim_material/redstone.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-material.resin | `minecraft:resin` (trim material) | `data/minecraft/trim_material/resin.json`; `RegistryDataLoader; TrimMaterial` | | - | todo |
| maps-decoration.trim-pattern.bolt | `minecraft:bolt` (trim pattern) | `data/minecraft/trim_pattern/bolt.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.coast | `minecraft:coast` (trim pattern) | `data/minecraft/trim_pattern/coast.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.dune | `minecraft:dune` (trim pattern) | `data/minecraft/trim_pattern/dune.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.eye | `minecraft:eye` (trim pattern) | `data/minecraft/trim_pattern/eye.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.flow | `minecraft:flow` (trim pattern) | `data/minecraft/trim_pattern/flow.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.host | `minecraft:host` (trim pattern) | `data/minecraft/trim_pattern/host.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.raiser | `minecraft:raiser` (trim pattern) | `data/minecraft/trim_pattern/raiser.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.rib | `minecraft:rib` (trim pattern) | `data/minecraft/trim_pattern/rib.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.sentry | `minecraft:sentry` (trim pattern) | `data/minecraft/trim_pattern/sentry.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.shaper | `minecraft:shaper` (trim pattern) | `data/minecraft/trim_pattern/shaper.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.silence | `minecraft:silence` (trim pattern) | `data/minecraft/trim_pattern/silence.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.snout | `minecraft:snout` (trim pattern) | `data/minecraft/trim_pattern/snout.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.spire | `minecraft:spire` (trim pattern) | `data/minecraft/trim_pattern/spire.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.tide | `minecraft:tide` (trim pattern) | `data/minecraft/trim_pattern/tide.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.vex | `minecraft:vex` (trim pattern) | `data/minecraft/trim_pattern/vex.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.ward | `minecraft:ward` (trim pattern) | `data/minecraft/trim_pattern/ward.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.wayfinder | `minecraft:wayfinder` (trim pattern) | `data/minecraft/trim_pattern/wayfinder.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.trim-pattern.wild | `minecraft:wild` (trim pattern) | `data/minecraft/trim_pattern/wild.json`; `RegistryDataLoader; TrimPattern` | | - | todo |
| maps-decoration.wolf-sound-variant.angry | `minecraft:angry` (wolf sound variant) | `data/minecraft/wolf_sound_variant/angry.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.big | `minecraft:big` (wolf sound variant) | `data/minecraft/wolf_sound_variant/big.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.classic | `minecraft:classic` (wolf sound variant) | `data/minecraft/wolf_sound_variant/classic.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.cute | `minecraft:cute` (wolf sound variant) | `data/minecraft/wolf_sound_variant/cute.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.grumpy | `minecraft:grumpy` (wolf sound variant) | `data/minecraft/wolf_sound_variant/grumpy.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.puglin | `minecraft:puglin` (wolf sound variant) | `data/minecraft/wolf_sound_variant/puglin.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-sound-variant.sad | `minecraft:sad` (wolf sound variant) | `data/minecraft/wolf_sound_variant/sad.json`; `RegistryDataLoader; WolfSoundVariant` | | - | todo |
| maps-decoration.wolf-variant.ashen | `minecraft:ashen` (wolf variant) | `data/minecraft/wolf_variant/ashen.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.black | `minecraft:black` (wolf variant) | `data/minecraft/wolf_variant/black.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.chestnut | `minecraft:chestnut` (wolf variant) | `data/minecraft/wolf_variant/chestnut.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.pale | `minecraft:pale` (wolf variant) | `data/minecraft/wolf_variant/pale.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.rusty | `minecraft:rusty` (wolf variant) | `data/minecraft/wolf_variant/rusty.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.snowy | `minecraft:snowy` (wolf variant) | `data/minecraft/wolf_variant/snowy.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.spotted | `minecraft:spotted` (wolf variant) | `data/minecraft/wolf_variant/spotted.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.striped | `minecraft:striped` (wolf variant) | `data/minecraft/wolf_variant/striped.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.wolf-variant.woods | `minecraft:woods` (wolf variant) | `data/minecraft/wolf_variant/woods.json`; `RegistryDataLoader; WolfVariant` | | - | todo |
| maps-decoration.zombie-nautilus-variant.temperate | `minecraft:temperate` (zombie nautilus variant) | `data/minecraft/zombie_nautilus_variant/temperate.json`; `RegistryDataLoader; ZombieNautilusVariant` | | - | todo |
| maps-decoration.zombie-nautilus-variant.warm | `minecraft:warm` (zombie nautilus variant) | `data/minecraft/zombie_nautilus_variant/warm.json`; `RegistryDataLoader; ZombieNautilusVariant` | | - | todo |
