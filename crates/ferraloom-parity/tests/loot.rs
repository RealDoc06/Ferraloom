use ferraloom_parity::{
    LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::component::Persistent,
};

// Live integration is pending commands.loot/random/clear and inventory insertion.
// The loot library's real-jar vectors independently cover evaluation and fill RNG.
async fn named_sequence_give(s: &mut Session) -> Result<ObservationLog> {
    let player = s.join("ParityBot").await?;
    let mut log = ObservationLog::default();
    let mut first = None;
    for round in 0..2 {
        let bot = s.bot(player)?;
        let had_items = bot
            .snapshot()
            .inventory
            .values()
            .any(|item| !item.is_empty());
        bot.command("clear ParityBot").await?;
        if had_items {
            bot.wait_for("cleared inventory", Ticks::DEFAULT, |state, _| {
                state.inventory.values().all(|item| item.is_empty())
            })
            .await?;
        }
        bot.command("random reset minecraft:chests/spawn_bonus_chest 1234 true true")
            .await?;
        bot.command("loot give ParityBot loot minecraft:chests/spawn_bonus_chest")
            .await?;
        bot.wait_for("loot inventory insertion", Ticks::DEFAULT, |state, _| {
            state.inventory.values().any(|item| !item.is_empty())
        })
        .await?;
        bot.wait_ticks(Ticks(2)).await?;
        bot.synchronize().await?;
        let items = bot
            .snapshot()
            .inventory
            .iter()
            .filter(|(_, item)| !item.is_empty())
            .map(|(slot, item)| Ok((*slot, item.to_json()?)))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            !items.is_empty(),
            "bonus chest table produced no inserted items"
        );
        if let Some(first) = &first {
            ensure!(
                first == &items,
                "reset did not reproduce the inventory layout"
            );
        } else {
            first = Some(items.clone());
        }
        log.record(format!("reset.{round}.inventory"), items)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "pending commands.loot, commands.random.reset, commands.clear and inventory.insertion"]
async fn named_sequence_loot_give() -> Result<()> {
    ferraloom_parity::run(
        LaunchOptions::default(),
        RunMode::from_env()?,
        named_sequence_give,
    )
    .await
}
