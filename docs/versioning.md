# Versioning

How releases are numbered from 0.2 on and how that maps onto the release workflow. The workflow itself is in
[release-flow.md](release-flow.md).

Until 0.2 releases were tagged by date (`v2026.10.19.15`). From 0.2 they follow
[semantic versioning](https://semver.org) on a release line.

## Tags

| | Tag | Release title | The programs say |
| --- | --- | --- | --- |
| From 0.2 | `v0.2.0`, `v0.2.1`, … | `Horizon 0.2.3: <what changed>` | `horizon-server 0.2.3`, `v0.2.3` in crash reports |
| Before 0.2 | `v2026.10.19`, `v2026.10.19.15`, … | `Horizon 2026.10.19.15: …` | the release folder's name |

- **The line** (`MAJOR.MINOR`) is in `.github/release-line.txt`, one line such as `0.2`. Moving it is a one-line pull
  request (`0.3`), and the new line's first release (`v0.3.0`) goes out as soon as it merges. A `feature` label bumps
  nothing: it only lists the change under **New**.
- **Patch releases** are every release the train makes: the patch after the newest release tagged on the line.
- **One channel**: every release is published under its final tag as the release and GitHub's latest, once no
  `release-blocker` is open ([One channel](release-flow.md#one-channel)). The pre-releases from Horizon's old Alpha
  channel (v0.2.1 on) keep their tags and stay pre-releases.
- **Pre-release tags** (`v0.3.0-rc.1`, `v0.3.0-beta.2`) are semver's and are reserved for a hand-made preview of a
  new line; the train never makes them. Every reader orders them before their release and after the release before
  (`v0.2.9` < `v0.3.0-beta.1` < `v0.3.0-rc.1` < `v0.3.0`).
- Horizon's history also holds `launcher-v…` and `launcher@…` tags from when the repository released a launcher;
  every reader ignores them.

### The order every reader uses

Every 0.2 tag is newer than every tag from before 0.2. Within each:

- before 0.2: number by number, missing numbers as zero (`v2026.10.19` = `v2026.10.19.0`);
- from 0.2: semver: major, minor, patch as numbers, then a release after its pre-releases, whose identifiers
  compare one by one (numbers before words, numbers as numbers, a shorter list first).

A tag is a 0.2 tag when it is `v` followed by exactly three numbers (and an optional `-PRE`) with a major below
1000; a date's first number is a year, so `v2026.10.21` never is. The rule is written once per language, with the
same tests: `crates/horizon-server/src/update.rs` (`newer`) and `tools/release_train.py` (`tag_key`).

## The workflow

`release.yml`: `tools/release_train.py decide` picks the tag (`next_game_tag`); the build gets the version without its
`v` in `HORIZON_VERSION`, so `horizon_core::release::VERSION` (and `--version`, the user agents and crash reports) say
`0.2.3`. No version-bump commits: `Cargo.toml` versions are what builds from source say. The build checks that
`horizon-server --version` and `horizon-prepare --version` say it. A line's first release (`X.Y.0`) opens its notes
with a paragraph saying so, goes out as soon as the release train runs after the line changes, without waiting for a
pull request, but never while `RELEASES_PAUSED` is set.

A server from before 0.2 compares tags number by number, so it never takes `v0.2.0` by itself: its owner runs
`horizon-server update --version v0.2.0` once, as 0.2.0's notes say.

## Irreversible

- Once 0.2 is published, dated tags can never be used again: every reader ranks them below `v0.2.0`.
- `.github/release-line.txt` must never be removed.
- The last dated release, built with the new ordering, must never be withdrawn or deleted: servers from before 0.2
  update through it.
