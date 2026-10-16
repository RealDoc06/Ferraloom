// Independent Minecraft 26.3 bytecode/report-derived test oracle, not runtime Java.
// These literal state IDs were copied from the root-owned primary catalog, not
// calculated from PoiType::from_state or generated block/property conditions.
// Registry names and state IDs were also checked against the exact cached reports.
// Evidence manifest: /home/doc/code/Ferraloom/.cache/reviews/r25-poi-source-inventory/hashes.json
// SHA256 (paths below are relative to /home/doc/code/Ferraloom):
// .cache/reviews/r25-poi-source-inventory/inventory.md
//   39e1abfea2eb6b5d7a51794271ef89bedcd70eaa5dc89484255cb921bf329d54
// .cache/reviews/r25-poi-source-inventory/catalog-primary.json
//   82620ccf91ee015d425018c868edd33a204081c11fc83ea4134a848717584f10
// .cache/reviews/r25-poi-source-inventory/mapping-inputs.json
//   be01c4cfa304f804f60b750c770f6032de7f26196ff95914a8ef3815a362a86d
// .cache/reviews/r25-poi-primary/PoiSection-PoiManager-PoiTypes.javap.txt
//   e6618e1431dee9d8a126718bdee06ee0772c1485c2c39106749d8489efa08f0f
// .cache/reviews/r25-poi-primary/Blocks-Collections-PoiType.javap.txt
//   170c6af657b6c9d0e9569885cd258aa29183763649b2a6f0e87973c4c647a7c1
// Exact inner JAR: a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6
// .cache/vanilla/26.3/generated/reports/registries.json
//   51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6
// .cache/vanilla/26.3/generated/reports/blocks.json
//   7a0de8aaf7b04d00c40b8ffdc4cfe75184cff2e821d336a020573201867105a4
// PoiTypes.bootstrap/getBlockStates supply the registrations; its static HEAD
// predicate and the ColorCollection/WeatheringCopperCollection memberships supply
// the bed/rod sets. PoiType's constructor labels maxTickets and validRange.
// Controls: 21 types, 46 blocks, 496 unique registered states, zero overlap;
// HOME 16 x 8 = 128; rods 8 x 24 = 192; excluded FOOT 128 and straw 16.

pub(super) struct Definition {
    pub(super) identifier: &'static str,
    pub(super) max_tickets: i32,
    pub(super) valid_range: i32,
    pub(super) blocks: &'static [(&'static str, &'static [u16])],
}

pub(super) const CATALOG: &[Definition] = &[
    Definition {
        identifier: "minecraft:armorer",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "blast_furnace",
            &[24119, 24120, 24121, 24122, 24123, 24124, 24125, 24126],
        )],
    },
    Definition {
        identifier: "minecraft:butcher",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "smoker",
            &[24111, 24112, 24113, 24114, 24115, 24116, 24117, 24118],
        )],
    },
    Definition {
        identifier: "minecraft:cartographer",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[("cartography_table", &[24127])],
    },
    Definition {
        identifier: "minecraft:cleric",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "brewing_stand",
            &[11186, 11187, 11188, 11189, 11190, 11191, 11192, 11193],
        )],
    },
    Definition {
        identifier: "minecraft:farmer",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "composter",
            &[
                25100, 25101, 25102, 25103, 25104, 25105, 25106, 25107, 25108,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:fisherman",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "barrel",
            &[
                24099, 24100, 24101, 24102, 24103, 24104, 24105, 24106, 24107, 24108, 24109, 24110,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:fletcher",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[("fletching_table", &[24128])],
    },
    Definition {
        identifier: "minecraft:leatherworker",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[
            ("cauldron", &[11194]),
            ("lava_cauldron", &[11198]),
            ("water_cauldron", &[11195, 11196, 11197]),
            ("powder_snow_cauldron", &[11199, 11200, 11201]),
        ],
    },
    Definition {
        identifier: "minecraft:librarian",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "lectern",
            &[
                24141, 24142, 24143, 24144, 24145, 24146, 24147, 24148, 24149, 24150, 24151, 24152,
                24153, 24154, 24155, 24156,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:mason",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[("stonecutter", &[24158, 24159, 24160, 24161])],
    },
    Definition {
        identifier: "minecraft:shepherd",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[("loom", &[24095, 24096, 24097, 24098])],
    },
    Definition {
        identifier: "minecraft:toolsmith",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[("smithing_table", &[24157])],
    },
    Definition {
        identifier: "minecraft:weaponsmith",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[(
            "grindstone",
            &[
                24129, 24130, 24131, 24132, 24133, 24134, 24135, 24136, 24137, 24138, 24139, 24140,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:home",
        max_tickets: 1,
        valid_range: 1,
        blocks: &[
            (
                "white_bed",
                &[2030, 2032, 2034, 2036, 2038, 2040, 2042, 2044],
            ),
            (
                "orange_bed",
                &[2046, 2048, 2050, 2052, 2054, 2056, 2058, 2060],
            ),
            (
                "magenta_bed",
                &[2062, 2064, 2066, 2068, 2070, 2072, 2074, 2076],
            ),
            (
                "light_blue_bed",
                &[2078, 2080, 2082, 2084, 2086, 2088, 2090, 2092],
            ),
            (
                "yellow_bed",
                &[2094, 2096, 2098, 2100, 2102, 2104, 2106, 2108],
            ),
            (
                "lime_bed",
                &[2110, 2112, 2114, 2116, 2118, 2120, 2122, 2124],
            ),
            (
                "pink_bed",
                &[2126, 2128, 2130, 2132, 2134, 2136, 2138, 2140],
            ),
            (
                "gray_bed",
                &[2142, 2144, 2146, 2148, 2150, 2152, 2154, 2156],
            ),
            (
                "light_gray_bed",
                &[2158, 2160, 2162, 2164, 2166, 2168, 2170, 2172],
            ),
            (
                "cyan_bed",
                &[2174, 2176, 2178, 2180, 2182, 2184, 2186, 2188],
            ),
            (
                "purple_bed",
                &[2190, 2192, 2194, 2196, 2198, 2200, 2202, 2204],
            ),
            (
                "blue_bed",
                &[2206, 2208, 2210, 2212, 2214, 2216, 2218, 2220],
            ),
            (
                "brown_bed",
                &[2222, 2224, 2226, 2228, 2230, 2232, 2234, 2236],
            ),
            (
                "green_bed",
                &[2238, 2240, 2242, 2244, 2246, 2248, 2250, 2252],
            ),
            ("red_bed", &[2254, 2256, 2258, 2260, 2262, 2264, 2266, 2268]),
            (
                "black_bed",
                &[2270, 2272, 2274, 2276, 2278, 2280, 2282, 2284],
            ),
        ],
    },
    Definition {
        identifier: "minecraft:meeting",
        max_tickets: 32,
        valid_range: 6,
        blocks: &[(
            "bell",
            &[
                24162, 24163, 24164, 24165, 24166, 24167, 24168, 24169, 24170, 24171, 24172, 24173,
                24174, 24175, 24176, 24177, 24178, 24179, 24180, 24181, 24182, 24183, 24184, 24185,
                24186, 24187, 24188, 24189, 24190, 24191, 24192, 24193,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:beehive",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[(
            "beehive",
            &[
                25149, 25150, 25151, 25152, 25153, 25154, 25155, 25156, 25157, 25158, 25159, 25160,
                25161, 25162, 25163, 25164, 25165, 25166, 25167, 25168, 25169, 25170, 25171, 25172,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:bee_nest",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[(
            "bee_nest",
            &[
                25125, 25126, 25127, 25128, 25129, 25130, 25131, 25132, 25133, 25134, 25135, 25136,
                25137, 25138, 25139, 25140, 25141, 25142, 25143, 25144, 25145, 25146, 25147, 25148,
            ],
        )],
    },
    Definition {
        identifier: "minecraft:nether_portal",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[("nether_portal", &[8687, 8688])],
    },
    Definition {
        identifier: "minecraft:lodestone",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[("lodestone", &[25187])],
    },
    Definition {
        identifier: "minecraft:test_instance",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[("test_instance_block", &[25099])],
    },
    Definition {
        identifier: "minecraft:lightning_rod",
        max_tickets: 0,
        valid_range: 1,
        blocks: &[
            (
                "lightning_rod",
                &[
                    33373, 33374, 33375, 33376, 33377, 33378, 33379, 33380, 33381, 33382, 33383,
                    33384, 33385, 33386, 33387, 33388, 33389, 33390, 33391, 33392, 33393, 33394,
                    33395, 33396,
                ],
            ),
            (
                "exposed_lightning_rod",
                &[
                    33397, 33398, 33399, 33400, 33401, 33402, 33403, 33404, 33405, 33406, 33407,
                    33408, 33409, 33410, 33411, 33412, 33413, 33414, 33415, 33416, 33417, 33418,
                    33419, 33420,
                ],
            ),
            (
                "weathered_lightning_rod",
                &[
                    33421, 33422, 33423, 33424, 33425, 33426, 33427, 33428, 33429, 33430, 33431,
                    33432, 33433, 33434, 33435, 33436, 33437, 33438, 33439, 33440, 33441, 33442,
                    33443, 33444,
                ],
            ),
            (
                "oxidized_lightning_rod",
                &[
                    33445, 33446, 33447, 33448, 33449, 33450, 33451, 33452, 33453, 33454, 33455,
                    33456, 33457, 33458, 33459, 33460, 33461, 33462, 33463, 33464, 33465, 33466,
                    33467, 33468,
                ],
            ),
            (
                "waxed_lightning_rod",
                &[
                    33469, 33470, 33471, 33472, 33473, 33474, 33475, 33476, 33477, 33478, 33479,
                    33480, 33481, 33482, 33483, 33484, 33485, 33486, 33487, 33488, 33489, 33490,
                    33491, 33492,
                ],
            ),
            (
                "waxed_exposed_lightning_rod",
                &[
                    33493, 33494, 33495, 33496, 33497, 33498, 33499, 33500, 33501, 33502, 33503,
                    33504, 33505, 33506, 33507, 33508, 33509, 33510, 33511, 33512, 33513, 33514,
                    33515, 33516,
                ],
            ),
            (
                "waxed_weathered_lightning_rod",
                &[
                    33517, 33518, 33519, 33520, 33521, 33522, 33523, 33524, 33525, 33526, 33527,
                    33528, 33529, 33530, 33531, 33532, 33533, 33534, 33535, 33536, 33537, 33538,
                    33539, 33540,
                ],
            ),
            (
                "waxed_oxidized_lightning_rod",
                &[
                    33541, 33542, 33543, 33544, 33545, 33546, 33547, 33548, 33549, 33550, 33551,
                    33552, 33553, 33554, 33555, 33556, 33557, 33558, 33559, 33560, 33561, 33562,
                    33563, 33564,
                ],
            ),
        ],
    },
];

// Report properties select all colored FOOT states and every straw state.
pub(super) const EXCLUDED_BEDS: &[(&str, &[u16])] = &[
    (
        "black_bed",
        &[2271, 2273, 2275, 2277, 2279, 2281, 2283, 2285],
    ),
    (
        "blue_bed",
        &[2207, 2209, 2211, 2213, 2215, 2217, 2219, 2221],
    ),
    (
        "brown_bed",
        &[2223, 2225, 2227, 2229, 2231, 2233, 2235, 2237],
    ),
    (
        "cyan_bed",
        &[2175, 2177, 2179, 2181, 2183, 2185, 2187, 2189],
    ),
    (
        "gray_bed",
        &[2143, 2145, 2147, 2149, 2151, 2153, 2155, 2157],
    ),
    (
        "green_bed",
        &[2239, 2241, 2243, 2245, 2247, 2249, 2251, 2253],
    ),
    (
        "light_blue_bed",
        &[2079, 2081, 2083, 2085, 2087, 2089, 2091, 2093],
    ),
    (
        "light_gray_bed",
        &[2159, 2161, 2163, 2165, 2167, 2169, 2171, 2173],
    ),
    (
        "lime_bed",
        &[2111, 2113, 2115, 2117, 2119, 2121, 2123, 2125],
    ),
    (
        "magenta_bed",
        &[2063, 2065, 2067, 2069, 2071, 2073, 2075, 2077],
    ),
    (
        "orange_bed",
        &[2047, 2049, 2051, 2053, 2055, 2057, 2059, 2061],
    ),
    (
        "pink_bed",
        &[2127, 2129, 2131, 2133, 2135, 2137, 2139, 2141],
    ),
    (
        "purple_bed",
        &[2191, 2193, 2195, 2197, 2199, 2201, 2203, 2205],
    ),
    ("red_bed", &[2255, 2257, 2259, 2261, 2263, 2265, 2267, 2269]),
    (
        "straw_bed",
        &[
            2286, 2287, 2288, 2289, 2290, 2291, 2292, 2293, 2294, 2295, 2296, 2297, 2298, 2299,
            2300, 2301,
        ],
    ),
    (
        "white_bed",
        &[2031, 2033, 2035, 2037, 2039, 2041, 2043, 2045],
    ),
    (
        "yellow_bed",
        &[2095, 2097, 2099, 2101, 2103, 2105, 2107, 2109],
    ),
];
