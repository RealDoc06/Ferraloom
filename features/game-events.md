# Game events and vibrations

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| game-events.notification-radius | Use each event's actual notification radius, including jukebox and shriek exceptions | `GameEvent#notificationRadius` | `tests/java/blocks/plants/sculk/extract_game_events.py --check` (61 native registry entries) | pass | partial |
